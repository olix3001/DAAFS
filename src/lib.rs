use std::sync::Mutex;

use cache::Cache;
use metadata::MetadataBlock;
use nbdkit::Server;
use serenity::Client;
use serenity::http::Http;
use serenity::{model::prelude::ChannelId, prelude::GatewayIntents};

use crate::cache::CacheBlock;

pub mod utils;
pub mod metadata;
pub mod cache;

/// Basic struct representing this plugin.
struct DiscordDrivePlugin {
    client: Client,
    rt: tokio::runtime::Runtime,
    meta: Mutex<Vec<MetadataBlock>>,
    channel: ChannelId,

    cache: Cache<4>
}

impl DiscordDrivePlugin {
    pub fn http(&self) -> &Http {
        &self.client.cache_and_http.http
    }

    pub fn cache(&self, block: CacheBlock) {
        self.cache.push(block);
    }

    pub fn read(&self, offset: u64) -> Vec<u8> {
        // Try to read from cache first.
        if let Some(data) = self.cache.read(offset) {
            return data.to_vec();
        }

        // If cache miss occurs, try to read from metadata blocks.
        let meta = self.meta.lock().unwrap();
        for block in meta.iter() {
            if let Some(data) = self.rt.block_on(async {
                block.try_read(&self.channel, self.http(), offset).await
            }) {
                // Cache the data.
                self.cache(CacheBlock::new(offset / (1024*1024*8), data.0, data.1));

                // Return the data. Now from the cache.
                return self.cache.read(offset).unwrap();
            }
        }

        vec![0; 4096]
    }

    pub fn write(&self, offset: u64, data: &[u8]) {
        // Try to write to cache first.
        if self.cache.write(offset, data) {
            return;
        }

        let mut meta = self.meta.lock().unwrap();
        for block in meta.iter_mut() {
            if let Some(data) = self.rt.block_on(async {
                block.try_write(&self.channel, self.http(), offset, data).await
            }) {
                // Cache the data.
                self.cache(CacheBlock::new(offset / (1024*1024*8), data.0, data.1));

                // Return the data. Now from the cache.
                return;
            }
        }

        let mut block = MetadataBlock::empty(0);

        if let Some(data) = self.rt.block_on(async {
            block.try_write(&self.channel, self.http(), offset, data).await
        }) {
            // Cache the data.
            self.cache(CacheBlock::new(offset / (1024*1024*8), data.0, data.1));
        }

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

        Self {
            rt,
            meta: Mutex::new(meta),
            client,
            channel,

            cache: Cache::new()
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
}

// Entry point for the plugin.
nbdkit::plugin!(DiscordDrivePlugin { write_at });