//! Durable drop transaction planning and application.
use std::collections::{BTreeMap, HashSet};
use std::io;
use std::time::{Duration, Instant};

use crate::inventory::STACK_LIMIT;
use crate::items::valid_item;
use crate::protocol::DroppedItem;

use super::{Drops, Entry, LIFETIME, distance_sq, invalid, journal, spatial, unix_ms};

/// Exact logical ownership change for one drop. Position is only included for
/// newly allocated drops; motion remains a checkpointed simulation detail.
#[derive(Clone, Debug)]
pub(in crate::server) struct DropMutation {
    pub id: u64,
    pub before: Vec<u8>,
    pub after: Vec<u8>,
    pub initial_position: Option<[f32; 3]>,
}

#[derive(Clone, Debug, Default)]
pub(in crate::server) struct DropPlan {
    pub changes: Vec<DropMutation>,
    pub allocator: Option<(u64, u64)>,
}

impl Drops {
    /// Plans harvest/drop-stack creation without mutating authoritative state.
    /// Existing merge targets are considered in stable ID order, and only the
    /// affected entries are cloned.
    pub(in crate::server) fn plan_spawn(
        &self,
        position: [f32; 3],
        item: u8,
        count: u16,
        pickup_delay: Duration,
    ) -> io::Result<DropPlan> {
        self.plan_spawns(&[(position, item, count, pickup_delay)])
    }

    /// Plans a group of loot outputs as one per-action delta. Same-item outputs
    /// share merge targets and newly allocated IDs deterministically.
    pub(in crate::server) fn plan_spawns(
        &self,
        spawns: &[([f32; 3], u8, u16, Duration)],
    ) -> io::Result<DropPlan> {
        let mut changed = BTreeMap::<u64, Entry>::new();
        let mut spawned = spatial::DropSpatialIndex::with_bucket_size(self.spatial.bucket_size());
        let original_next = self.next_id;
        let mut next_id = original_next;
        let mut mutations = Vec::new();
        for &(position, item, mut count, pickup_delay) in spawns {
            if !valid_item(item) || position.iter().any(|coordinate| !coordinate.is_finite()) {
                return Err(invalid("invalid durable drop spawn"));
            }
            let min = position.map(|coordinate| coordinate - 1.0);
            let max = position.map(|coordinate| coordinate + 1.0);
            let mut ids = self.spatial.query_aabb(min, max);
            ids.extend(spawned.query_aabb(min, max));
            ids.sort_unstable();
            ids.dedup();
            while count > 0 {
                let target = ids.iter().copied().find(|id| {
                    let entry = changed.get(id).or_else(|| self.entries.get(id));
                    entry.is_some_and(|entry| {
                        entry.item.item == item
                            && entry.age() < LIFETIME
                            && entry.item.count < STACK_LIMIT
                            && distance_sq(entry.item.position, position) < 1.0
                    })
                });
                if let Some(id) = target {
                    let entry = changed
                        .entry(id)
                        .or_insert_with(|| self.entries[&id].clone());
                    let added = count.min(STACK_LIMIT - entry.item.count);
                    entry.item.count += added;
                    entry.age_at_load = Duration::ZERO;
                    entry.age_since = Instant::now();
                    entry.created_unix_ms = unix_ms();
                    entry.pickup_delay = pickup_delay;
                    count -= added;
                    continue;
                }

                let id = next_id;
                next_id = next_id
                    .checked_add(1)
                    .filter(|next| *next != 0)
                    .ok_or_else(|| io::Error::other("drop ID space exhausted"))?;
                let added = count.min(STACK_LIMIT);
                let born = unix_ms();
                changed.insert(
                    id,
                    Entry {
                        item: DroppedItem {
                            id,
                            item,
                            count: added,
                            position,
                            age_ms: 0,
                        },
                        vertical_speed: 0.0,
                        age_at_load: Duration::ZERO,
                        age_since: Instant::now(),
                        created_unix_ms: born,
                        pickup_delay,
                    },
                );
                spawned.insert(id, position);
                ids.push(id);
                count -= added;
            }
        }

        for (id, after_entry) in changed {
            let before = self
                .entries
                .get(&id)
                .map(journal::encode_owner)
                .unwrap_or_default();
            mutations.push(DropMutation {
                id,
                before,
                after: journal::encode_owner(&after_entry),
                initial_position: if self.entries.contains_key(&id) {
                    None
                } else {
                    Some(after_entry.item.position)
                },
            });
        }
        Ok(DropPlan {
            changes: mutations,
            allocator: (next_id != original_next).then_some((original_next, next_id)),
        })
    }

    /// Plans count transfers out of selected drops. A full removal is encoded
    /// as an empty after-value, so replay cannot resurrect a picked/expired ID.
    pub(in crate::server) fn plan_take(&self, takes: &[(u64, u16)]) -> io::Result<DropPlan> {
        let mut amounts = BTreeMap::<u64, u16>::new();
        for &(id, count) in takes {
            if count == 0 {
                continue;
            }
            let entry = self
                .entries
                .get(&id)
                .ok_or_else(|| invalid("drop changed before durable take"))?;
            let amount = amounts.entry(id).or_default();
            *amount = amount.saturating_add(count).min(entry.item.count);
        }
        let mut changes = Vec::new();
        for (id, amount) in amounts {
            let entry = &self.entries[&id];
            let mut after = entry.clone();
            after.item.count -= amount;
            changes.push(DropMutation {
                id,
                before: journal::encode_owner(entry),
                after: if after.item.count == 0 {
                    Vec::new()
                } else {
                    journal::encode_owner(&after)
                },
                initial_position: None,
            });
        }
        Ok(DropPlan {
            changes,
            allocator: None,
        })
    }

    pub(in crate::server) fn plan_expired(&self, limit: usize) -> DropPlan {
        let ids = self.expiry.expired_ids(Instant::now(), limit);
        DropPlan {
            changes: ids
                .into_iter()
                .map(|id| DropMutation {
                    id,
                    before: journal::encode_owner(&self.entries[&id]),
                    after: Vec::new(),
                    initial_position: None,
                })
                .collect(),
            allocator: None,
        }
    }

    /// Applies mutations only after their transaction has been fsynced. This
    /// touches only entries in the committed delta, not the entire drop map.
    pub(in crate::server) fn validate_plan(&self, plan: &DropPlan) -> io::Result<()> {
        if let Some((before, after)) = plan.allocator
            && self.next_id != before
            && self.next_id != after
        {
            return Err(invalid("drop allocator changed before durable apply"));
        }
        let mut seen = HashSet::with_capacity(plan.changes.len());
        for mutation in &plan.changes {
            if !seen.insert(mutation.id) {
                return Err(invalid("duplicate drop in durable plan"));
            }
            let current = self
                .entries
                .get(&mutation.id)
                .map(journal::encode_owner)
                .unwrap_or_default();
            if current != mutation.before && current != mutation.after {
                return Err(invalid("drop changed before durable apply"));
            }
            if !mutation.after.is_empty() {
                journal::decode_owner(mutation.id, &mutation.after)?;
                if !self.entries.contains_key(&mutation.id) && mutation.initial_position.is_none() {
                    return Err(invalid("new drop mutation has no initial position"));
                }
                if mutation.initial_position.is_some_and(|position| {
                    position.iter().any(|coordinate| !coordinate.is_finite())
                }) {
                    return Err(invalid("invalid durable drop position"));
                }
            }
        }
        Ok(())
    }

    pub(in crate::server) fn apply_plan(&mut self, plan: &DropPlan) -> io::Result<()> {
        self.validate_plan(plan)?;
        for mutation in &plan.changes {
            if mutation.after.is_empty() {
                if self.entries.remove(&mutation.id).is_some() {
                    self.remove_entry_indexes(mutation.id);
                }
                self.active.remove(&mutation.id);
                continue;
            }
            let (item, count, born, delay) = journal::decode_owner(mutation.id, &mutation.after)
                .expect("prevalidated owner bytes");
            if let Some(entry) = self.entries.get_mut(&mutation.id) {
                entry.item.item = item;
                entry.item.count = count;
                entry.created_unix_ms = born;
                let age = Duration::from_millis(unix_ms().saturating_sub(born));
                entry.age_at_load = age;
                entry.age_since = Instant::now();
                entry.pickup_delay = Duration::from_millis(u64::from(delay));
                self.expiry.insert(mutation.id, age, entry.age_since);
                // A count/ownership update must not wake a settled drop. It
                // can, however, make an entry ineligible for further physics
                // if its persisted age is already past the lifetime.
                if age >= LIFETIME {
                    self.active.remove(&mutation.id);
                }
            } else {
                // `validate_plan` proved this before the first mutation, so the
                // application half is deliberately infallible.
                let position = mutation
                    .initial_position
                    .expect("prevalidated spawn position");
                let age = Duration::from_millis(unix_ms().saturating_sub(born));
                self.entries.insert(
                    mutation.id,
                    Entry {
                        item: DroppedItem {
                            id: mutation.id,
                            item,
                            count,
                            position,
                            age_ms: 0,
                        },
                        vertical_speed: 0.0,
                        age_at_load: age,
                        age_since: Instant::now(),
                        created_unix_ms: born,
                        pickup_delay: Duration::from_millis(u64::from(delay)),
                    },
                );
                let entry = &self.entries[&mutation.id];
                self.spatial.insert(mutation.id, entry.item.position);
                self.expiry.insert(mutation.id, age, entry.age_since);
                if age < LIFETIME {
                    self.active.insert(mutation.id);
                }
            }
        }
        if let Some((before, after)) = plan.allocator {
            debug_assert!(self.next_id == before || self.next_id == after);
            self.next_id = self.next_id.max(after);
        }
        if !plan.changes.is_empty() || plan.allocator.is_some() {
            self.revision = self.revision.wrapping_add(1);
        }
        Ok(())
    }
}
