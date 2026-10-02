//! Bounded resident-chunk cache with an intrusive, exact LRU list.

use std::collections::HashMap;
use std::ops::Index;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use super::{BlockId, Chunk, ChunkKey};

pub(super) struct OwnerState {
    pub(super) chunk: Arc<Chunk>,
    pub(super) edits: std::collections::BTreeMap<u16, BlockId>,
}

pub(super) struct CacheEntry {
    // Each resident owner has its own short critical section. Fire apply
    // workers mutate disjoint slots after WAL sync; the cache's LRU metadata
    // remains coordinator-owned and is never locked by those workers.
    pub(super) state: Arc<RwLock<OwnerState>>,
    previous: Option<usize>,
    next: Option<usize>,
    unpinned_previous: Option<usize>,
    unpinned_next: Option<usize>,
    pins: usize,
}

impl CacheEntry {
    pub(super) fn read(&self) -> RwLockReadGuard<'_, OwnerState> {
        self.state
            .read()
            .expect("authoritative owner state poisoned; restart is required")
    }

    pub(super) fn write(&self) -> RwLockWriteGuard<'_, OwnerState> {
        self.state
            .write()
            .expect("authoritative owner state poisoned; restart is required")
    }
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
    oldest_unpinned: Option<usize>,
    newest_unpinned: Option<usize>,
    pinned_entries: usize,
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
            oldest_unpinned: None,
            newest_unpinned: None,
            pinned_entries: 0,
            capacity,
        }
    }

    pub(super) fn len(&self) -> usize {
        self.indices.len()
    }

    pub(super) fn capacity(&self) -> usize {
        self.capacity
    }

    /// A new resident owner may replace only an owner with no subscribers.
    pub(super) fn can_admit(&self) -> bool {
        self.len() < self.capacity || self.oldest_unpinned.is_some()
    }

    pub(super) fn pinned_len(&self) -> usize {
        self.pinned_entries
    }

    /// One pin per client subscription. A snapshot may be advertised only
    /// while its authoritative owner remains resident.
    pub(super) fn pin(&mut self, key: ChunkKey) -> bool {
        let Some(&slot) = self.indices.get(&key) else {
            return false;
        };
        let pins = self.slots[slot].as_ref().unwrap().pins;
        if pins == 0 {
            self.unlink_unpinned(slot);
            self.pinned_entries += 1;
        }
        self.slots[slot].as_mut().unwrap().pins = pins
            .checked_add(1)
            .expect("chunk subscription count exhausted");
        true
    }

    pub(super) fn unpin(&mut self, key: ChunkKey) -> bool {
        let Some(&slot) = self.indices.get(&key) else {
            return false;
        };
        let entry = self.slots[slot].as_mut().unwrap();
        if entry.pins == 0 {
            return false;
        }
        entry.pins -= 1;
        if entry.pins == 0 {
            self.push_unpinned(slot);
            self.pinned_entries -= 1;
        }
        true
    }

    pub(super) fn contains_key(&self, key: &ChunkKey) -> bool {
        self.indices.contains_key(key)
    }

    pub(super) fn get(&self, key: &ChunkKey) -> Option<&CacheEntry> {
        let slot = *self.indices.get(key)?;
        self.slots[slot].as_ref()
    }

    /// Bounded resident inventory for immutable worker captures; no LRU touch.
    pub(super) fn entries(&self) -> impl Iterator<Item = (&ChunkKey, &CacheEntry)> {
        self.indices
            .iter()
            .map(|(key, &slot)| (key, self.slots[slot].as_ref().expect("resident cache slot")))
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
    ) -> bool {
        debug_assert_eq!(chunk.key, key);

        if let Some(&slot) = self.indices.get(&key) {
            let entry = self.slots[slot]
                .as_mut()
                .expect("cache index must point to an occupied slot");
            *entry.write() = OwnerState { chunk, edits };
            self.touch_slot(slot);
            return true;
        }

        if self.indices.len() == self.capacity {
            let Some(slot) = self.oldest_unpinned else {
                return false;
            };
            self.evict_slot(slot);
        }

        let slot = if let Some(slot) = self.free_slots.pop() {
            self.slots[slot] = Some(CacheEntry {
                state: Arc::new(RwLock::new(OwnerState { chunk, edits })),
                previous: None,
                next: None,
                unpinned_previous: None,
                unpinned_next: None,
                pins: 0,
            });
            slot
        } else {
            let slot = self.slots.len();
            self.slots.push(Some(CacheEntry {
                state: Arc::new(RwLock::new(OwnerState { chunk, edits })),
                previous: None,
                next: None,
                unpinned_previous: None,
                unpinned_next: None,
                pins: 0,
            }));
            slot
        };
        let replaced = self.indices.insert(key, slot);
        debug_assert!(replaced.is_none());
        self.push_newest(slot);
        self.push_unpinned(slot);
        true
    }

    pub(super) fn remove(&mut self, key: &ChunkKey) {
        let Some(slot) = self.indices.remove(key) else {
            return;
        };
        if self.slots[slot].as_ref().unwrap().pins == 0 {
            self.unlink_unpinned(slot);
        } else {
            self.pinned_entries -= 1;
        }
        self.unlink(slot);
        self.slots[slot].take();
        self.free_slots.push(slot);
    }

    fn evict_slot(&mut self, slot: usize) {
        let key = self.slots[slot]
            .as_ref()
            .expect("oldest cache slot must be occupied")
            .read()
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
        if self.slots[slot].as_ref().unwrap().pins == 0 {
            self.unlink_unpinned(slot);
            self.push_unpinned(slot);
        }
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

    fn unlink_unpinned(&mut self, slot: usize) {
        let entry = self.slots[slot].as_ref().unwrap();
        let previous = entry.unpinned_previous;
        let next = entry.unpinned_next;
        if let Some(previous) = previous {
            self.slots[previous].as_mut().unwrap().unpinned_next = next;
        } else {
            self.oldest_unpinned = next;
        }
        if let Some(next) = next {
            self.slots[next].as_mut().unwrap().unpinned_previous = previous;
        } else {
            self.newest_unpinned = previous;
        }
        let entry = self.slots[slot].as_mut().unwrap();
        entry.unpinned_previous = None;
        entry.unpinned_next = None;
    }

    fn push_unpinned(&mut self, slot: usize) {
        let previous = self.newest_unpinned;
        let entry = self.slots[slot].as_mut().unwrap();
        debug_assert_eq!(entry.pins, 0);
        entry.unpinned_previous = previous;
        entry.unpinned_next = None;
        if let Some(previous) = previous {
            self.slots[previous].as_mut().unwrap().unpinned_next = Some(slot);
        } else {
            self.oldest_unpinned = Some(slot);
        }
        self.newest_unpinned = Some(slot);
    }
}

impl Index<&ChunkKey> for ChunkCache {
    type Output = CacheEntry;

    fn index(&self, key: &ChunkKey) -> &Self::Output {
        self.get(key).expect("indexed cache key must be resident")
    }
}
