//! Bounded resident-chunk cache with an intrusive, exact LRU list.

use std::collections::HashMap;
use std::ops::Index;
use std::sync::Arc;

use super::{BlockId, Chunk, ChunkKey};

pub(super) struct CacheEntry {
    // Readers of an active simulation phase retain this allocation while a
    // later edit replaces the resident version. No full chunk copy is needed
    // just to hand immutable terrain to a worker.
    pub(super) chunk: Arc<Chunk>,
    pub(super) edits: std::collections::BTreeMap<u16, BlockId>,
    previous: Option<usize>,
    next: Option<usize>,
}

/// The key map provides direct resident lookup; the slot links maintain exact
/// recency without a cache-wide scan or a second hash lookup on a touched read.
/// All three metadata collections are bounded by `capacity`.
pub(super) struct ChunkCache {
    slots: Vec<Option<CacheEntry>>,
    free_slots: Vec<usize>,
    indices: HashMap<ChunkKey, usize>,
    oldest: Option<usize>,
    newest: Option<usize>,
    capacity: usize,
}

impl ChunkCache {
    pub(super) fn new(capacity: usize) -> Self {
        debug_assert!(capacity > 0);
        Self {
            slots: Vec::new(),
            free_slots: Vec::new(),
            indices: HashMap::new(),
            oldest: None,
            newest: None,
            capacity,
        }
    }

    pub(super) fn len(&self) -> usize {
        self.indices.len()
    }

    pub(super) fn contains_key(&self, key: &ChunkKey) -> bool {
        self.indices.contains_key(key)
    }

    pub(super) fn get(&self, key: &ChunkKey) -> Option<&CacheEntry> {
        let slot = *self.indices.get(key)?;
        self.slots[slot].as_ref()
    }

    pub(super) fn get_mut(&mut self, key: &ChunkKey) -> Option<&mut CacheEntry> {
        let slot = *self.indices.get(key)?;
        self.slots[slot].as_mut()
    }

    /// Looks up and promotes one resident entry. The key map is queried once;
    /// list maintenance uses compact slot indices rather than more hash probes.
    pub(super) fn get_mut_and_touch(&mut self, key: ChunkKey) -> Option<&mut CacheEntry> {
        let slot = *self.indices.get(&key)?;
        self.touch_slot(slot);
        self.slots[slot].as_mut()
    }

    pub(super) fn insert(
        &mut self,
        key: ChunkKey,
        chunk: Arc<Chunk>,
        edits: std::collections::BTreeMap<u16, BlockId>,
    ) {
        debug_assert_eq!(chunk.key, key);

        if let Some(&slot) = self.indices.get(&key) {
            let entry = self.slots[slot]
                .as_mut()
                .expect("cache index must point to an occupied slot");
            entry.chunk = chunk;
            entry.edits = edits;
            self.touch_slot(slot);
            return;
        }

        if self.indices.len() == self.capacity {
            self.evict_oldest();
        }

        let slot = if let Some(slot) = self.free_slots.pop() {
            self.slots[slot] = Some(CacheEntry {
                chunk,
                edits,
                previous: None,
                next: None,
            });
            slot
        } else {
            let slot = self.slots.len();
            self.slots.push(Some(CacheEntry {
                chunk,
                edits,
                previous: None,
                next: None,
            }));
            slot
        };
        let replaced = self.indices.insert(key, slot);
        debug_assert!(replaced.is_none());
        self.push_newest(slot);
    }

    pub(super) fn remove(&mut self, key: &ChunkKey) {
        let Some(slot) = self.indices.remove(key) else {
            return;
        };
        self.unlink(slot);
        self.slots[slot].take();
        self.free_slots.push(slot);
    }

    fn evict_oldest(&mut self) {
        let Some(slot) = self.oldest else {
            return;
        };
        let key = self.slots[slot]
            .as_ref()
            .expect("oldest cache slot must be occupied")
            .chunk
            .key;
        self.remove(&key);
    }

    fn touch_slot(&mut self, slot: usize) {
        if self.newest == Some(slot) {
            return;
        }
        self.unlink(slot);
        self.push_newest(slot);
    }

    fn unlink(&mut self, slot: usize) {
        let entry = self.slots[slot]
            .as_ref()
            .expect("linked cache slot must be occupied");
        let previous = entry.previous;
        let next = entry.next;

        if let Some(previous) = previous {
            self.slots[previous]
                .as_mut()
                .expect("previous cache slot must be occupied")
                .next = next;
        } else {
            self.oldest = next;
        }
        if let Some(next) = next {
            self.slots[next]
                .as_mut()
                .expect("next cache slot must be occupied")
                .previous = previous;
        } else {
            self.newest = previous;
        }

        let entry = self.slots[slot]
            .as_mut()
            .expect("unlinked cache slot must be occupied");
        entry.previous = None;
        entry.next = None;
    }

    fn push_newest(&mut self, slot: usize) {
        let previous = self.newest;
        let entry = self.slots[slot]
            .as_mut()
            .expect("new cache slot must be occupied");
        entry.previous = previous;
        entry.next = None;

        if let Some(previous) = previous {
            self.slots[previous]
                .as_mut()
                .expect("previous cache slot must be occupied")
                .next = Some(slot);
        } else {
            self.oldest = Some(slot);
        }
        self.newest = Some(slot);
    }
}

impl Index<&ChunkKey> for ChunkCache {
    type Output = CacheEntry;

    fn index(&self, key: &ChunkKey) -> &Self::Output {
        self.get(key).expect("indexed cache key must be resident")
    }
}
