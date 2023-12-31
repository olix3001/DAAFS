use std::sync::{Mutex, Arc, atomic::AtomicBool};

use serenity::{http::Http, model::prelude::ChannelId};

use crate::metadata::{Page, MetadataBlock};

/// This queue is used to sync data between drive and discord.
pub struct Queue<const S: usize> {
    pub data: Arc<Mutex<Vec<QueueBlock>>>,
    pub thread: Option<std::thread::JoinHandle<()>>,
    pub is_syncing: Arc<AtomicBool>,
}

pub struct QueueBlock {
    pub page: Page,
    pub data: Vec<u8>,
}

impl QueueBlock {
    pub fn new(page: Page, data: Vec<u8>) -> Self {
        Self {
            page,
            data,
        }
    }

    pub async fn sync(&mut self, http: &Http, channel_id: ChannelId) {
        self.page.update_message(http, &channel_id, &self.data).await;
    }
}

impl<const S: usize> Queue<S> {
    pub fn new() -> Self {
        Self {
            data: Arc::new(Mutex::new(Vec::with_capacity(S))),
            thread: None,
            is_syncing: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn push(&self, page: Page, data: Vec<u8>) {
        let mut sdl;
        {
            let sdata = self.data.lock().unwrap();
            sdl = sdata.len();
        }

        while sdl >= S {
            // Wait for the queue to be empty.
            std::thread::sleep(std::time::Duration::from_millis(100));
            {
                let sdata = self.data.lock().unwrap();
                sdl = sdata.len();
            }
        }

        let mut sdata = self.data.lock().unwrap();
        sdata.push(QueueBlock::new(page, data));
    }

    /// Tries to release the offset from the queue and returns the data if it exists.
    pub fn release_offset(&self, offset: u64) -> Option<(Page, Vec<u8>)> {
        let mut sdata = self.data.lock().unwrap();
        for i in 0..sdata.len() {
            if sdata[i].page.offset == offset {
                let block = sdata.remove(i);
                return Some((block.page, block.data));
            }
        }

        None
    }

    pub fn pop(&self) -> Option<QueueBlock> {
        let mut sdata = self.data.lock().unwrap();
        sdata.pop()
    }

    /// Flushes the queue. This will block until the queue is empty.
    pub fn flush(&self) {
        let mut slen;
        {
            let sdata = self.data.lock().unwrap();
            slen = sdata.len();
        }

        while slen > 0 {
            // Wait for the queue to be empty.
            std::thread::sleep(std::time::Duration::from_millis(100));
            {
                let sdata = self.data.lock().unwrap();
                slen = sdata.len();
            }
        }

        println!("Waiting for last block to sync.");

        // Wait for the thread to finish syncing.
        while self.is_syncing.load(std::sync::atomic::Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        println!("Queue flushed.");
    }

    pub fn start_sync_thread(mut self, http: Arc<Http>, channel_id: ChannelId, metadata: Arc<Mutex<Vec<MetadataBlock>>>) -> Self {
        let data = self.data.clone();
        let is_syncing = Arc::clone(&self.is_syncing);
        let t = std::thread::spawn(move || {
            // TODO: Await multiple blocks at once.
            let rt = tokio::runtime::Runtime::new().unwrap();
            loop {
                let mut sdata = data.lock().unwrap();
                if sdata.len() == 0 {
                    // Ensure that the thread doesn't spinlock.
                    drop(sdata);
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    continue;
                }

                // Sync the data.
                is_syncing.store(true, std::sync::atomic::Ordering::SeqCst);
                let mut block = sdata.pop().unwrap();
                // We don't need the lock anymore. Drop it.
                drop(sdata);

                // Sync the data.
                let channel_id = channel_id.clone();
                rt.block_on(async {
                    block.sync(&http, channel_id).await;

                    let mut meta = metadata.lock().unwrap();
                    for m in meta.iter_mut() {
                        if m.update_page(&http, &channel_id, block.page.clone()).await {
                            break;
                        }
                    }
                }); 
                is_syncing.store(false, std::sync::atomic::Ordering::SeqCst);

                println!("Synced block at offset {}.", block.page.offset);
            }
        });

        self.thread = Some(t);
        self
    }
}