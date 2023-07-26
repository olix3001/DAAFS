use std::sync::Mutex;

use crate::utils::BitMask;

pub struct Cache<const S: usize> {
    pub data: Mutex<Vec<CacheBlock>>,
}

pub struct CacheBlock {
    pub offset: u64,
    pub data: Vec<u8>,
    pub mask: BitMask<256>,
}

impl CacheBlock {
    pub fn new(offset: u64, data: Vec<u8>, mask: BitMask<256>) -> Self {
        Self {
            offset,
            data,
            mask,
        }
    }
}

impl<const S: usize> Cache<S> {
    pub fn new() -> Self {
        Self {
            data: Mutex::new(Vec::with_capacity(S)),
        }
    }

    pub fn read(&self, offset: u64) -> Option<Vec<u8>> {
        let data = self.data.lock().unwrap();
        for block in data.iter() {
            if offset >= block.offset && offset + 4096 < block.offset + block.data.len() as u64 {
                let offset = (offset - block.offset) as usize;
                // Use mask
                if block.mask.get(offset / 4096) {
                    return Some(vec![0; 4096]);
                }
                // Return data
                return Some(block.data[offset..offset + 4096].to_vec());
            }
        }

        None
    }

    /// Returns true if the write was successful.
    pub fn write(&self, offset: u64, data: &[u8]) -> bool {
        let mut sdata = self.data.lock().unwrap();
        for block in sdata.iter_mut() {
            if offset >= block.offset && offset + 4096 < block.offset + block.data.len() as u64 {
                let offset = (offset - block.offset) as usize;
                block.data[offset..offset + 4096].copy_from_slice(data);

                // Flip mask if needed
                if data.iter().all(|byte| *byte == 0) {
                    block.mask.set(offset / 4096, true);
                } else {
                    block.mask.set(offset / 4096, false);
                }

                return true;
            }
        }

        false
    }

    /// Pushes a new block to the cache. If the cache is full, the oldest block is removed and returned.
    pub fn push(&self, block: CacheBlock) -> Option<CacheBlock> {
        let mut data = self.data.lock().unwrap();
        if data.len() >= S {
            let removed = Some(data.remove(0));
            data.push(block);
            return removed;
        }

        data.push(block);

        None
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const MB: usize = 1024 * 1024;

    #[test]
    fn test_cache() {
        let cache = Cache::<2>::new();

        cache.push(CacheBlock {
            offset: 0,
            data: vec![0; 8*MB],
            mask: BitMask::new(),
        });

        cache.push(CacheBlock {
            offset: 8*MB as u64,
            data: vec![1; 8*MB],
            mask: BitMask::new(),
        });

        assert_eq!(cache.read(0).unwrap(), vec![0; 4096].as_slice());

        cache.write(0, &[1; 4096]);

        assert_eq!(cache.read(0).unwrap(), vec![1; 4096].as_slice());

        cache.write(4096, &[0; 4096]);

        assert_eq!(cache.read(4096).unwrap(), vec![0; 4096].as_slice());

        cache.push(CacheBlock {
            offset: 16*MB as u64,
            data: vec![2; 8*MB],
            mask: BitMask::new(),
        });

        assert_eq!(cache.read(16*MB as u64+4096).unwrap(), vec![2; 4096].as_slice());
    }
}