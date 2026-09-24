//! Bounded world drop snapshots and deterministic pickup candidates.
mod expiry;
pub(in crate::server) mod journal;
mod persistence;
mod physics;
mod planning;
mod spatial;

pub(super) use planning::DropPlan;

#[cfg(test)]
use crate::inventory::STACK_LIMIT;
use crate::inventory::{ComponentPayload, Stack};
#[cfg(test)]
use crate::items::ItemId;
use crate::protocol::DroppedItem;
use std::collections::{BTreeSet, HashMap};
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
    item: DroppedItem,
    components: Option<Arc<ComponentPayload>>,
    vertical_speed: f32,
    age_at_load: Duration,
    age_since: Instant,
    created_unix_ms: u64,
    pickup_delay: Duration,
}

#[derive(Clone)]
pub(super) struct Drops {
    entries: HashMap<u64, Entry>,
    active: BTreeSet<u64>,
    spatial: spatial::DropSpatialIndex,
    expiry: expiry::ExpiryIndex,
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
                    entry.item.item == item
                        && entry.item.count < STACK_LIMIT
                        && distance_sq(entry.item.position, position) < 1.0
                })
            });
            if let Some(id) = target {
                let entry = self.entries.get_mut(&id).expect("spatial drop exists");
                let taken = count.min(STACK_LIMIT - entry.item.count);
                entry.item.count += taken;
                entry.age_at_load = Duration::ZERO;
                entry.age_since = Instant::now();
                entry.created_unix_ms = unix_ms();
                self.expiry.insert(id, Duration::ZERO, entry.age_since);
                count -= taken;
                self.revision = self.revision.wrapping_add(1);
                continue;
            }
            let taken = count.min(STACK_LIMIT);
            let id = self.next_id;
            self.next_id = self.next_id.wrapping_add(1).max(1);
            let age_since = Instant::now();
            self.entries.insert(
                id,
                Entry {
                    item: DroppedItem {
                        id,
                        item,
                        count: taken,
                        position,
                        age_ms: 0,
                    },
                    components: None,
                    vertical_speed: 0.0,
                    age_at_load: Duration::ZERO,
                    age_since,
                    created_unix_ms: unix_ms(),
                    pickup_delay,
                },
            );
            self.spatial.insert(id, position);
            self.expiry.insert(id, Duration::ZERO, age_since);
            self.active.insert(id);
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
            .filter(|entry| distance_sq(entry.item.position, position) <= VIEW_RANGE_SQ)
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
                age >= entry.pickup_delay
                    && age < LIFETIME
                    && distance_sq(entry.item.position, position) <= PICKUP_RANGE_SQ
            })
            .map(Entry::snapshot)
            .collect();
        items.sort_by_key(|item| item.id);
        items.truncate(256);
        items
    }

    pub(super) fn stack(&self, id: u64) -> Option<Stack> {
        self.entries.get(&id).map(|entry| Stack {
            item: entry.item.item,
            count: entry.item.count,
            components: entry.components.clone(),
        })
    }
    #[cfg(test)]
    pub(super) fn take(&mut self, id: u64, count: u16) {
        if let Some(entry) = self.entries.get_mut(&id) {
            if count >= entry.item.count {
                self.entries.remove(&id);
                self.remove_entry_indexes(id);
                self.active.remove(&id);
            } else {
                entry.item.count -= count;
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
}

impl Entry {
    fn age(&self) -> Duration {
        self.age_at_load.saturating_add(self.age_since.elapsed())
    }

    fn snapshot(&self) -> DroppedItem {
        DroppedItem {
            age_ms: self.age().as_millis().min(u32::MAX as u128) as u32,
            ..self.item
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

fn distance_sq(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

#[cfg(test)]
mod tests;
