use serenity::{http::Http, model::prelude::ChannelId};

use crate::utils::{BitMask, ToBase32, byte_to_base_255, base_255_to_byte};

/// Block containing metadata about discord pages
pub struct MetadataBlock {
    /// Id of the message this block is currently associated with
    pub message_id: u64,
    /// Blocks that are linked to this block
    pub pages: Vec<Page>
}

/// Each page is 8MB of data that is stored in a discord message
#[derive(Clone)]
pub struct Page {
    /// Offset of this page (stored as a multiple of 8MB)
    pub offset: u64,
    /// Id of the message this page is currently associated with
    pub message_id: u64,
    /// Bitmask representing which blocks are zeroed out (1 = zeroed, 0 = not zeroed).
    /// This is used for faster reads/writes.
    pub zero_mask: BitMask<256>, // 256 bytes = 2048 bits (one for each 4KB block)
}

impl MetadataBlock {
    pub fn empty(message_id: u64) -> Self {
        Self {
            message_id,
            pages: Vec::new()
        }
    }

    /// Loads the metadata from text in a discord message
    pub fn from_text(message_id: u64, text: &str) -> Self {
        // Format:
        // METABLOCK
        // <offset>:<message_id>
        // ...

        let mut pages = Vec::new();

        let mut lines = text.lines();
        lines.next(); // Skip METABLOCK

        for line in lines {
            let mut split = line.split(':');
            let offset = u64::from_base32(split.next().unwrap());
            let message_id = u64::from_base32(split.next().unwrap());

            pages.push(Page::from_text(message_id, offset, split.next().unwrap()));
        }

        Self {
            message_id,
            pages
        }
    }

    /// Generates the text that should be stored in a discord message
    pub fn as_text(&self) -> String {
        // Format:
        // METABLOCK
        // <offset>:<message_id>:<page_data>
        // ...

        let mut text = String::new();

        text.push_str("METABLOCK\n");

        for page in &self.pages {
            let line = format!("{}:{}:{}\n", page.offset.to_base32(), page.message_id.to_base32(), page.as_text());
            text.push_str(&line);
        }

        text
    }

    pub async fn load_from_discord(http: &Http, channel_id: ChannelId, message_id: u64) -> Self {
        let message = channel_id.message(http, message_id).await.unwrap();

        Self::from_text(message_id, &message.content)
    }

    pub async fn load_all(http: &Http, channel_id: ChannelId, mut limit: usize) -> Vec<Self> {
        let mut blocks = Vec::new();

        let mut current_id = 0;

        while limit > 0 {
            let messages = 
                channel_id.messages(http, |retriever| {
                    retriever.limit(100);
                    if current_id != 0 {
                        retriever.before(current_id);
                    }
                    retriever
                })
                .await
                .unwrap();

            if messages.len() == 0 {
                break;
            }

            for message in messages.iter() {
                if message.content.starts_with("METABLOCK") {
                    blocks.push(Self::from_text(message.id.0, &message.content));
                }
            }

            limit -= messages.len();
            current_id = messages.last().unwrap().id.0;
        }

        blocks
    }

    pub async fn try_read(&self, channel: &ChannelId, http: &Http, offset: u64) -> Option<(Vec<u8>, Page)> {
        // Check if page exists
        let page = self.pages.iter().find(|page| page.offset == offset / (1024*1024*8));

        if let Some(page) = page {
            // Read page
            Some((page.read(channel, http, offset).await, page.clone()))
        } else {
            None
        }
    }

    pub async fn try_write(&mut self, channel: &ChannelId, http: &Http, offset: u64, data: &[u8]) -> Option<(Vec<u8>, Page)> {
        // Check if page with offset exists
        let page = self.pages.iter_mut().find(|page| page.offset == offset / (1024*1024*8));

        if let Some(page) = page {
            // Write page
            let d = page.write(channel, http, offset, data).await;
            return d;
        }

        // Check if there is enough space to create a new page
        if self.pages.len() >= 5 {
            return None;
        }

        // Create new page
        let mut page = Page::new(offset / (1024*1024*8));

        // Write page
        let d = page.write(channel, http, offset, data).await;
        self.pages.push(page);
        self.update_message(http, channel).await;
        d
    }

    pub async fn update_page(&mut self, http: &Http, channel: &ChannelId, page_new: Page) -> bool {
        // Check if page with offset exists
        let page = self.pages.iter_mut().find(|page| page.offset == page_new.offset);

        if let Some(page) = page {
            page.message_id = page_new.message_id;
            page.zero_mask = page_new.zero_mask;
        } else {
            return false;
        }

        self.update_message(http, channel).await;
        true
    }

    pub async fn update_message(&mut self, http: &Http, channel: &ChannelId) {
        if self.message_id == 0 {
            let message = channel.send_message(http, |m| {
                m.content(self.as_text())
            }).await.unwrap();
            self.message_id = message.id.0;
            return;
        }

        let mut message = channel.message(http, self.message_id).await.unwrap();

        message.edit(http, |m| {
            m.content(self.as_text())
        }).await.unwrap();
    }
}

impl Page {
    pub fn new(offset: u64) -> Self {
        Self {
            offset,
            message_id: 0,
            zero_mask: BitMask::new()
        }
    }

    /// Loads the metadata from text in a discord message
    pub fn from_text(message_id: u64, offset: u64, text: &str) -> Self {
        // Format:
        // <zero_mask>

        let mut zero_mask_bytes = [0; 256];

        for (i, byte) in text.chars().enumerate() {
            zero_mask_bytes[i] = base_255_to_byte(byte);
        }

        let zero_mask = BitMask::from_bytes(&zero_mask_bytes);

        Self {
            offset,
            message_id,
            zero_mask
        }
    }

    /// Generates the text that should be stored in a discord message
    pub fn as_text(&self) -> String {
        // Format:
        // <zero_mask>

        let mut text = String::new();

        let zero_mask = self.zero_mask.as_bytes();

        for byte in zero_mask {
            text.push(byte_to_base_255(*byte));
        }

        text
    }

    /// Read at relative offset
    pub async fn read(&self, channel: &ChannelId, http: &Http, offset: u64) -> Vec<u8> {
        let offset = offset - self.offset * 1024 * 1024 * 8;

        // Check mask
        if self.zero_mask.get((offset / 4096) as usize) {
            return vec![0; 1024*1024*8];
        }

        // Read message from discord
        let message = channel.message(http, self.message_id).await.unwrap();

        // Read data from message (using reqwest)
        let url = message.attachments[0].url.clone();

        let data = reqwest::get(&url).await.unwrap().bytes().await.unwrap();

        // Return data
        data.to_vec()
    }

    /// Write at relative offset. Returns new data if the page was modified.
    pub async fn write(&mut self, channel: &ChannelId, http: &Http, ooffset: u64, data: &[u8]) -> Option<(Vec<u8>, Page)> {
        let mut current_data = vec![0; 1024 * 1024 * 8];
        let offset = ooffset - self.offset * 1024 * 1024 * 8;

        // Check if page is already written
        if self.message_id != 0 {
            // Read current data
            current_data = self.read(channel, http, ooffset).await;
        }

        // Check if data is all zeroes
        if data.iter().all(|byte| *byte == 0) {
            // Set mask
            self.zero_mask.set((offset / 4096) as usize, true);

            return Some((current_data, self.clone()));
        }

        // Modify data
        for (i, byte) in data.iter().enumerate() {
            current_data[offset as usize + i as usize] = *byte;
        }

        // // Create message
        // let page_name = format!("page_{}.bin", self.offset);
        // let files = vec![(
        //     current_data.as_slice(),
        //     page_name.as_str(),
        // )];
        // let message = channel.send_files(http, files, |m| {
        //     m.content("DATA PAGE")
        // }).await.unwrap();

        // Set message id
        // self.message_id = message.id.0;

        // Return
        Some((current_data, self.clone()))
    }

    pub async fn update_message(&mut self, http: &Http, channel: &ChannelId, data: &[u8]) {
        let page_name = format!("page_{}.bin", self.offset);
        if self.message_id != 0 {
            // Delete old message
            channel.delete_message(http, self.message_id).await.ok();
        }

        // Create message
        let files = vec![(
            data,
            page_name.as_str(),
        )];
        let message = channel.send_files(http, files, |m| {
            m.content("DATA PAGE")
        }).await.unwrap();

        // Set message id
        self.message_id = message.id.0;
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn metadata_block() {
        let mut block = MetadataBlock::empty(1234567890);
        block.pages.push(Page {
            offset: 0,
            message_id: 1234567891,
            zero_mask: BitMask::new()
        });

        let text = block.as_text();

        let block = MetadataBlock::from_text(1234567890, &text);

        assert_eq!(block.message_id, 1234567890);
        assert_eq!(block.pages.len(), 1);
        assert_eq!(block.pages[0].offset, 0);
        assert_eq!(block.pages[0].message_id, 1234567891);
        assert_eq!(block.pages[0].zero_mask.as_bytes(), [0; 256]);
    }
}