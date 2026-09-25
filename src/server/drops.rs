//! Bounded world drop snapshots and deterministic pickup candidates.
mod entity;
mod expiry;
pub(in crate::server) mod journal;
mod persistence;
mod physics;
mod planning;
mod shards;
mod spatial;

pub(super) use entity::{DropEntityPayload, register_entity_type};
pub(in crate::server) use shards::{
    SHARD_ALLOCATOR_LEN, write_allocator_snapshot, write_shard_snapshot,
};
pub(super) use planning::DropPlan;

#[cfg(test)]
use crate::inventory::STACK_LIMIT;
use crate::inventory::Stack;
#[cfg(test)]
use crate::items::ItemId;
use crate::protocol::DroppedItem;
use crate::server::entities::EntityPayload;
use crate::world::{ChunkKey, world_to_chunk};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const PICKUP_RANGE_SQ: f32 = 2.25 * 2.25;
const VIEW_RANGE: f32 = 64.0;
const VIEW_RANGE_SQ: f32 = VIEW_RANGE * VIEW_RANGE;
pub(super) const LIFETIME: Duration = Duration::from_secs(600);
pub(super) const DROP_RADIUS: f32 = 0.18;
pub(super) const GRAVITY: f32 = 24.0;
pub(super) const TERMINAL_SPEED: f32 = 30.0;

#[derive(Clone)]
struct Entry {
    id: u64,
    position: [f32; 3],
    payload: EntityPayload,
    vertical_speed: f32,
    age_at_load: Duration,
    age_since: Instant,
}

#[derive(Clone)]
pub(super) struct Drops {
    entries: HashMap<u64, Entry>,
    active: BTreeSet<u64>,
    spatial: spatial::DropSpatialIndex,
    expiry: expiry::ExpiryIndex,
    /// Chunk-owner membership for bounded per-chunk checkpoints. Every live
    /// entry belongs to exactly one chunk set; empty chunk sets are removed.
    chunk_members: BTreeMap<ChunkKey, BTreeSet<u64>>,
    /// Chunks whose checkpoint shard no longer matches live state. Marked at
    /// every mutation site (spawn, removal, count change, motion) so the
    /// checkpoint path serializes only dirty chunks instead of all drops.
    chunk_dirty: BTreeSet<ChunkKey>,
    /// The allocator checkpoint no longer matches `next_id`.
    allocator_dirty: bool,
    next_id: u64,
    revision: u64,
    path: Option<PathBuf>,
    catalog: Arc<crate::content::Catalog>,
    last_gc: Instant,
}

impl Drops {
    pub(super) fn new() -> Self {
        Self::new_with_catalog(Arc::new(crate::content::catalog().clone()))
    }

    pub(super) fn new_with_catalog(catalog: Arc<crate::content::Catalog>) -> Self {
        Self {
            entries: HashMap::new(),
            active: BTreeSet::new(),
            spatial: spatial::DropSpatialIndex::new(),
            expiry: expiry::ExpiryIndex::default(),
            chunk_members: BTreeMap::new(),
            chunk_dirty: BTreeSet::new(),
            allocator_dirty: false,
            next_id: 1,
            revision: 0,
            path: None,
            catalog,
            last_gc: Instant::now(),
        }
    }
    pub(super) fn revision(&self) -> u64 {
        self.revision
    }

    pub(super) fn active_len(&self) -> usize {
        self.active.len()
    }

    pub(super) fn owner_snapshot(&self, id: u64) -> Vec<u8> {
        self.entries
            .get(&id)
            .map(journal::encode_owner)
            .unwrap_or_default()
    }

    pub(super) fn allocator_snapshot(&self) -> Vec<u8> {
        self.next_id.to_le_bytes().to_vec()
    }

    #[cfg(test)]
    pub(super) fn spawn(
        &mut self,
        position: [f32; 3],
        item: ItemId,
        mut count: u16,
        pickup_delay: Duration,
    ) {
        while count > 0 {
            let min = position.map(|coordinate| coordinate - 1.0);
            let max = position.map(|coordinate| coordinate + 1.0);
            let target = self.spatial.query_aabb(min, max).into_iter().find(|id| {
                self.entries.get(id).is_some_and(|entry| {
                    let stack = &entry.drop_payload().stack;
                    stack.item == item
                        && stack.count < STACK_LIMIT
                        && distance_sq(entry.position, position) < 1.0
                })
            });
            if let Some(id) = target {
                let entry = self.entries.get_mut(&id).expect("spatial drop exists");
                let mut payload = entry.drop_payload().clone();
                let taken = count.min(STACK_LIMIT - payload.stack.count);
                payload.stack.count += taken;
                payload.created_unix_ms = unix_ms();
                entry.payload = payload.into_entity_payload();
                entry.age_at_load = Duration::ZERO;
                entry.age_since = Instant::now();
                self.expiry.insert(id, Duration::ZERO, entry.age_since);
                let position = entry.position;
                count -= taken;
                self.chunk_dirty.insert(chunk_of(position));
                self.revision = self.revision.wrapping_add(1);
                continue;
            }
            let taken = count.min(STACK_LIMIT);
            let id = self.next_id;
            self.next_id = self.next_id.wrapping_add(1).max(1);
            self.allocator_dirty = true;
            let age_since = Instant::now();
            self.entries.insert(
                id,
                Entry::new(
                    id,
                    position,
                    DropEntityPayload::new(Stack::new(item, taken), unix_ms(), pickup_delay),
                    0.0,
                    Duration::ZERO,
                    age_since,
                ),
            );
            self.spatial.insert(id, position);
            self.expiry.insert(id, Duration::ZERO, age_since);
            self.active.insert(id);
            self.index_insert(id, position);
            count -= taken;
            self.revision = self.revision.wrapping_add(1);
        }
    }
    pub(super) fn nearby(&self, position: [f32; 3]) -> Vec<DroppedItem> {
        let min = position.map(|coordinate| coordinate - VIEW_RANGE);
        let max = position.map(|coordinate| coordinate + VIEW_RANGE);
        let mut items: Vec<_> = self
            .spatial
            .query_aabb(min, max)
            .into_iter()
            .filter_map(|id| self.entries.get(&id))
            .filter(|entry| distance_sq(entry.position, position) <= VIEW_RANGE_SQ)
            .map(Entry::snapshot)
            .collect();
        items.sort_by(|a, b| {
            distance_sq(a.position, position)
                .total_cmp(&distance_sq(b.position, position))
                .then(a.id.cmp(&b.id))
        });
        items.truncate(256);
        items
    }
    pub(super) fn pickup_candidates(&self, position: [f32; 3]) -> Vec<DroppedItem> {
        let radius = PICKUP_RANGE_SQ.sqrt();
        let min = position.map(|coordinate| coordinate - radius);
        let max = position.map(|coordinate| coordinate + radius);
        let mut items: Vec<_> = self
            .spatial
            .query_aabb(min, max)
            .into_iter()
            .filter_map(|id| self.entries.get(&id))
            .filter(|entry| {
                let age = entry.age();
                age >= entry.drop_payload().pickup_delay
                    && age < LIFETIME
                    && distance_sq(entry.position, position) <= PICKUP_RANGE_SQ
            })
            .map(Entry::snapshot)
            .collect();
        items.sort_by_key(|item| item.id);
        items.truncate(256);
        items
    }

    pub(super) fn stack(&self, id: u64) -> Option<Stack> {
        self.entries
            .get(&id)
            .map(|entry| entry.drop_payload().stack.clone())
    }
    #[cfg(test)]
    pub(super) fn take(&mut self, id: u64, count: u16) {
        if let Some(entry) = self.entries.get_mut(&id) {
            let position = entry.position;
            let mut payload = entry.drop_payload().clone();
            if count >= payload.stack.count {
                self.entries.remove(&id);
                self.remove_entry_indexes(id);
                self.index_remove(id, position);
                self.active.remove(&id);
            } else {
                payload.stack.count -= count;
                entry.payload = payload.into_entity_payload();
                self.chunk_dirty.insert(chunk_of(position));
            }
            self.revision = self.revision.wrapping_add(1);
        }
    }
    pub(super) fn has_expired(&mut self) -> bool {
        if self.last_gc.elapsed() < Duration::from_secs(1) {
            return false;
        }
        self.last_gc = Instant::now();
        self.expiry.has_expired(self.last_gc)
    }

    fn remove_entry_indexes(&mut self, id: u64) {
        self.spatial.remove(id);
        self.expiry.remove(id);
    }

    /// Rebuilds chunk membership from scratch. Load paths use this once the
    /// full entry set is known; the result starts checkpoint-clean.
    fn rebuild_chunk_members(&mut self) {
        self.chunk_members.clear();
        self.chunk_dirty.clear();
        self.allocator_dirty = false;
        for (&id, entry) in &self.entries {
            self.chunk_members
                .entry(chunk_of(entry.position))
                .or_default()
                .insert(id);
        }
    }

    fn index_insert(&mut self, id: u64, position: [f32; 3]) {
        self.chunk_members
            .entry(chunk_of(position))
            .or_default()
            .insert(id);
        self.chunk_dirty.insert(chunk_of(position));
    }

    fn index_remove(&mut self, id: u64, position: [f32; 3]) {
        let chunk = chunk_of(position);
        if let Some(members) = self.chunk_members.get_mut(&chunk) {
            members.remove(&id);
            if members.is_empty() {
                self.chunk_members.remove(&chunk);
            }
        }
        self.chunk_dirty.insert(chunk);
    }
}

impl Entry {
    fn new(
        id: u64,
        position: [f32; 3],
        payload: DropEntityPayload,
        vertical_speed: f32,
        age_at_load: Duration,
        age_since: Instant,
    ) -> Self {
        Self {
            id,
            position,
            payload: payload.into_entity_payload(),
            vertical_speed,
            age_at_load,
            age_since,
        }
    }

    fn drop_payload(&self) -> &DropEntityPayload {
        self.payload
            .downcast_ref()
            .expect("drop entry stores registered drop payload")
    }

    fn age(&self) -> Duration {
        self.age_at_load.saturating_add(self.age_since.elapsed())
    }

    fn snapshot(&self) -> DroppedItem {
        let stack = &self.drop_payload().stack;
        DroppedItem {
            id: self.id,
            item: stack.item,
            count: stack.count,
            position: self.position,
            age_ms: self.age().as_millis().min(u32::MAX as u128) as u32,
        }
    }
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// Chunk-owner shard for one drop position. Drops move vertically only, so a
/// move dirties at most its old and new chunk when it crosses a y boundary.
pub(super) fn chunk_of(position: [f32; 3]) -> ChunkKey {
    world_to_chunk(
        position[0].floor() as i32,
        position[1].floor() as i32,
        position[2].floor() as i32,
    )
    .0
}

/// Field-disjoint chunk transfer used on the physics hot loop, where the
/// active-set iteration already borrows the drop map.
fn transfer_chunk_member(
    members: &mut BTreeMap<ChunkKey, BTreeSet<u64>>,
    dirty: &mut BTreeSet<ChunkKey>,
    id: u64,
    old_position: [f32; 3],
    new_position: [f32; 3],
) {
    let old_chunk = chunk_of(old_position);
    let new_chunk = chunk_of(new_position);
    if old_chunk == new_chunk {
        dirty.insert(new_chunk);
        return;
    }
    // A cross-chunk move is one atomic membership transfer: the drop leaves
    // the old owner set and joins the new one together, and both shards
    // checkpoint before the move is considered durable.
    if let Some(set) = members.get_mut(&old_chunk) {
        set.remove(&id);
        if set.is_empty() {
            members.remove(&old_chunk);
        }
    }
    members.entry(new_chunk).or_default().insert(id);
    dirty.insert(old_chunk);
    dirty.insert(new_chunk);
}

fn distance_sq(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

#[cfg(test)]
mod tests;
