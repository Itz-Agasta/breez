//! Bounded LRU cache of decoded frames, keyed by (take, frame index).
//! Frames are `Arc<[u8]>` so a cache hit is a pointer clone; the byte cap
//! bounds total decoded-frame memory (~13 frames at 1440p under 200 MB).

use std::collections::HashMap;

use breez_codec::VideoFrame;

pub const DEFAULT_CAP_BYTES: usize = 200 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameKey {
    pub take: u32,
    pub frame: u64,
}

struct Entry {
    frame: VideoFrame,
    last_used: u64,
}

pub struct FrameCache {
    entries: HashMap<FrameKey, Entry>,
    cap_bytes: usize,
    bytes: usize,
    tick: u64,
}

impl FrameCache {
    pub fn new(cap_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            cap_bytes,
            bytes: 0,
            tick: 0,
        }
    }

    pub fn get(&mut self, key: FrameKey) -> Option<VideoFrame> {
        self.tick += 1;
        let tick = self.tick;
        self.entries.get_mut(&key).map(|entry| {
            entry.last_used = tick;
            entry.frame.clone()
        })
    }

    pub fn insert(&mut self, key: FrameKey, frame: VideoFrame) {
        if frame.data.len() > self.cap_bytes {
            return;
        }
        self.tick += 1;
        if let Some(old) = self.entries.insert(
            key,
            Entry {
                frame,
                last_used: self.tick,
            },
        ) {
            self.bytes -= old.frame.data.len();
        }
        self.bytes += self.entries[&key].frame.data.len();
        // Evict least-recently-used until under cap. Linear scan is fine:
        // the cap keeps the entry count small (tens of frames).
        while self.bytes > self.cap_bytes {
            let Some((&key, _)) = self.entries.iter().min_by_key(|(_, entry)| entry.last_used)
            else {
                break;
            };
            let removed = self.entries.remove(&key).expect("key from iter");
            self.bytes -= removed.frame.data.len();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn frame(bytes: usize) -> VideoFrame {
        VideoFrame {
            data: Arc::from(vec![0u8; bytes]),
            width: 1,
            height: 1,
            pts_ns: 0,
        }
    }

    fn key(frame: u64) -> FrameKey {
        FrameKey { take: 0, frame }
    }

    #[test]
    fn insert_should_evict_least_recently_used_over_cap() {
        let mut cache = FrameCache::new(250);
        cache.insert(key(1), frame(100));
        cache.insert(key(2), frame(100));
        assert!(cache.get(key(1)).is_some()); // 1 now more recent than 2
        cache.insert(key(3), frame(100));
        assert!(cache.get(key(2)).is_none());
        assert!(cache.get(key(1)).is_some());
        assert!(cache.get(key(3)).is_some());
    }

    #[test]
    fn insert_should_ignore_frames_larger_than_cap() {
        let mut cache = FrameCache::new(10);
        cache.insert(key(1), frame(11));
        assert!(cache.get(key(1)).is_none());
    }
}
