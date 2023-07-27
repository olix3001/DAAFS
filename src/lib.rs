use std::sync::{Mutex, Arc};

use cache::Cache;
use metadata::{MetadataBlock, Page};
use nbdkit::Server;
use queue::Queue;
use serenity::Client;
use serenity::http::Http;
use serenity::{model::prelude::ChannelId, prelude::GatewayIntents};

use crate::cache::CacheBlock;

pub mod utils;
pub mod metadata;
pub mod cache;
pub mod queue;

/// Basic struct representing this plugin.
struct DiscordDrivePlugin {
    client: Client,
    rt: tokio::runtime::Runtime,
    meta: Arc<Mutex<Vec<MetadataBlock>>>,
    channel: ChannelId,

    cache: Cache<4>,
    queue: Queue<4>,
}

impl DiscordDrivePlugin {
    pub fn http(&self) -> &Http {
        &self.client.cache_and_http.http
    }

    pub fn cache(&self, block: CacheBlock) {
        if let Some(block) = self.cache.push(block) {
            self.queue.push(Page {
                offset: block.offset,
                message_id: block.message_id,
                zero_mask: block.mask,
            }, block.data);
        }
    }

    /// Tries to read from cache ensuring that the data is NOT in the queue.
    pub fn read_cache(&self, offset: u64) -> Option<Vec<u8>> {
        // Check if the data is in the queue.
        if let Some((page, data)) = self.queue.release_offset(offset / (1024*1024*8)) {
            // Cache the data.
            self.cache(CacheBlock::new(offset / (1024*1024*8), page.message_id, data, page.zero_mask));

            // Return the data. Now from the cache.
            return self.cache.read(offset);
        }

        if let Some(data) = self.cache.read(offset) {
            return Some(data.to_vec());
        }

        None
    }

    /// Tries to write to cache ensuring that the data is NOT in the queue.
    pub fn write_cache(&self, offset: u64, dataa: &[u8]) -> bool {
        // Check if the data is in the queue.
        if let Some((page, data)) = self.queue.release_offset(offset / (1024*1024*8)) {
            // Cache the data.
            self.cache(CacheBlock::new(offset / (1024*1024*8), page.message_id, data, page.zero_mask));

            // Return the data. Now from the cache.
            return self.cache.write(offset, dataa);
        }

        self.cache.write(offset, dataa)
    }

    pub fn read(&self, offset: u64) -> Vec<u8> {
        // Try to read from cache first.
        if let Some(data) = self.read_cache(offset) {
            return data.to_vec();
        }

        // If cache miss occurs, try to read from metadata blocks.
        let meta = self.meta.lock().unwrap();
        for block in meta.iter() {
            if let Some(data) = self.rt.block_on(async {
                block.try_read(&self.channel, self.http(), offset).await
            }) {
                // Drop the lock to prevent deadlock on the same thread.
                drop(meta);

                // Cache the data.
                self.cache(CacheBlock::new(offset / (1024*1024*8), data.1.message_id, data.0, data.1.zero_mask));

                // Return the data. Now from the cache.
                return self.cache.read(offset).unwrap();
            }
        }

        vec![0; 4096]
    }

    pub fn write(&self, offset: u64, data: &[u8]) {
        // Try to write to cache first.
        if self.write_cache(offset, data) {
            return;
        }

        let mut meta = self.meta.lock().unwrap();
        for block in meta.iter_mut() {
            if let Some(data) = self.rt.block_on(async {
                block.try_write(&self.channel, self.http(), offset, data).await
            }) {
                // Drop the lock to prevent deadlock on the same thread.
                drop(meta);

                // Cache the data.
                self.cache(CacheBlock::new(offset / (1024*1024*8), data.1.message_id, data.0, data.1.zero_mask));

                // Return.
                return;
            }
        }

        let mut block = MetadataBlock::empty(0);

        if let Some(data) = self.rt.block_on(async {
            block.try_write(&self.channel, self.http(), offset, data).await
        }) {
            // Drop the lock to prevent deadlock on the same thread.
            drop(meta);

            // Cache the data.
            self.cache(CacheBlock::new(offset / (1024*1024*8), data.1.message_id, data.0, data.1.zero_mask));
        }

        // Acquire the lock again.
        let mut meta = self.meta.lock().unwrap();

        meta.push(block);

        println!("Created new metadata block at offset {}", offset);
    }
}

/// Default implementation of the plugin.
impl Default for DiscordDrivePlugin {
    fn default() -> Self {
        let rt = tokio::runtime::Runtime::new().unwrap();

        let client = rt.block_on(async {
            Client::builder(&env!("BOT_TOKEN"), GatewayIntents::all())
                .await
                .expect("Failed to create client")            
        });
        
        let channel = ChannelId(
            env!("FS_CHANNEL_ID")
                .parse()
                .expect("Failed to parse CHANNEL_ID from env")
        );

        let meta = rt.block_on(async {
            MetadataBlock::load_all(&client.cache_and_http.http, channel, 500).await
        });

        let meta = Arc::new(Mutex::new(meta));

        let queue = Queue::new();
        let queue = queue.start_sync_thread(client.cache_and_http.http.clone(), channel.clone(), meta.clone());

        Self {
            rt,
            meta,
            client,
            channel,

            cache: Cache::new(),
            queue: queue,
        }
    }
}

/// Implementation of the plugin.
impl Server for DiscordDrivePlugin {
    fn get_size(&self) -> nbdkit::Result<i64> {
        Ok(
            env!("DEVICE_SIZE")
                .parse()
                .expect("Failed to parse DEVICE_SIZE from env")
        )
    }

    fn name() -> &'static str where Self: Sized {
        "discorddrive"
    }

    fn open(_readonly: bool) -> nbdkit::Result<Box<dyn Server>> where Self: Sized {
        Ok(Box::new(Self::default()))
    }

    fn read_at(&self, buf: &mut [u8], offset: u64) -> nbdkit::Result<()> {
        let data = self.read(offset);

        buf.copy_from_slice(&data);

        Ok(())
    }

    fn write_at(&self, buf: &[u8], offset: u64, _flags: nbdkit::Flags) -> nbdkit::Result<()> {
        self.write(offset, buf);

        Ok(())
    }

    fn flush(&self) -> nbdkit::Result<()> {
        for block in self.cache.data.lock().unwrap().drain(..) {
            self.queue.push(Page {
                offset: block.offset,
                message_id: block.message_id,
                zero_mask: block.mask,
            }, block.data.clone());
        }

        self.queue.flush();

        // Move all metadata blocks to the bottom of the channel.
        let mut meta = self.meta.lock().unwrap();
        for block in meta.iter_mut() {
            self.rt.block_on(async {
                block.move_to_bottom(self.http(), self.channel).await;
            });
        }

        Ok(())
    }
}

// Entry point for the plugin.
nbdkit::plugin!(DiscordDrivePlugin { write_at, flush });