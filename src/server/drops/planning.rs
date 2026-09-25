//! Durable drop transaction planning and application.
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io;
use std::time::{Duration, Instant};

use super::shards::{MAX_SHARD_BYTES, SHARD_HEADER};
use super::{
    DropEntityPayload, Drops, Entry, LIFETIME, chunk_of, distance_sq, invalid, journal, spatial,
    unix_ms,
};
use crate::inventory::{STACK_LIMIT, Stack};
use crate::items::ItemId;
use crate::world::ChunkKey;

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
        item: ItemId,
        count: u16,
        pickup_delay: Duration,
    ) -> io::Result<DropPlan> {
        self.plan_spawns(&[(position, item, count, pickup_delay)])
    }

    pub(in crate::server) fn plan_spawn_stack(
        &self,
        position: [f32; 3],
        stack: Stack,
        pickup_delay: Duration,
    ) -> io::Result<DropPlan> {
        self.plan_stack_spawns(&[(position, stack, pickup_delay)])
    }

    /// Plans a group of loot outputs as one per-action delta. Same-item outputs
    /// share merge targets and newly allocated IDs deterministically.
    pub(in crate::server) fn plan_spawns(
        &self,
        spawns: &[([f32; 3], ItemId, u16, Duration)],
    ) -> io::Result<DropPlan> {
        let stacks: Vec<_> = spawns
            .iter()
            .map(|&(position, item, count, delay)| (position, Stack::new(item, count), delay))
            .collect();
        self.plan_stack_spawns(&stacks)
    }

    pub(in crate::server) fn plan_stack_spawns(
        &self,
        spawns: &[([f32; 3], Stack, Duration)],
    ) -> io::Result<DropPlan> {
        let mut changed = BTreeMap::<u64, Entry>::new();
        let mut spawned = spatial::DropSpatialIndex::with_bucket_size(self.spatial.bucket_size());
        let original_next = self.next_id;
        let mut next_id = original_next;
        let mut mutations = Vec::new();
        for (position, stack, pickup_delay) in spawns {
            let (position, item, mut count, pickup_delay) =
                (*position, stack.item, stack.count, *pickup_delay);
            let pickup_delay_ms = u16::try_from(pickup_delay.as_millis())
                .map_err(|_| invalid("drop pickup delay exceeds durable range"))?;
            if !stack.valid_in(&self.catalog)
                || position.iter().any(|coordinate| !coordinate.is_finite())
                || Duration::from_millis(u64::from(pickup_delay_ms)) != pickup_delay
            {
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
                        let entry_stack = &entry.drop_payload().stack;
                        entry_stack.item == item
                            && entry_stack.components == stack.components
                            && entry.age() < LIFETIME
                            && entry_stack.count < STACK_LIMIT
                            && distance_sq(entry.position, position) < 1.0
                    })
                });
                if let Some(id) = target {
                    let entry = changed
                        .entry(id)
                        .or_insert_with(|| self.entries[&id].clone());
                    let mut payload = entry.drop_payload().clone();
                    let added = count.min(STACK_LIMIT - payload.stack.count);
                    payload.stack.count += added;
                    payload.created_unix_ms = unix_ms();
                    payload.pickup_delay = pickup_delay;
                    entry.payload = payload.into_entity_payload();
                    entry.age_at_load = Duration::ZERO;
                    entry.age_since = Instant::now();
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
                    Entry::new(
                        id,
                        position,
                        DropEntityPayload::new(
                            Stack {
                                item,
                                count: added,
                                components: stack.components.clone(),
                            },
                            born,
                            pickup_delay,
                        ),
                        0.0,
                        Duration::ZERO,
                        Instant::now(),
                    ),
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
                    Some(after_entry.position)
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
            *amount = amount
                .saturating_add(count)
                .min(entry.drop_payload().stack.count);
        }
        let mut changes = Vec::new();
        for (id, amount) in amounts {
            let entry = &self.entries[&id];
            let mut after = entry.clone();
            let mut payload = after.drop_payload().clone();
            payload.stack.count -= amount;
            after.payload = payload.into_entity_payload();
            changes.push(DropMutation {
                id,
                before: journal::encode_owner(entry),
                after: if after.drop_payload().stack.count == 0 {
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
                journal::decode_owner_with_catalog(mutation.id, &mutation.after, &self.catalog)?;
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

    /// Projects per-chunk checkpoint sizes for the WAL staging gate so one
    /// action reserves exactly the shard keys it will dirty, plus whether
    /// the allocator checkpoint joins them. Sizes derive from live member
    /// records plus the plan delta, so the gate bounds real checkpoint
    /// memory instead of one aggregate guess.
    pub(in crate::server) fn projected_checkpoint_sizes(
        &self,
        plan: &DropPlan,
    ) -> io::Result<(Vec<(ChunkKey, usize)>, bool)> {
        self.validate_plan(plan)?;
        let mut affected = BTreeSet::new();
        for mutation in &plan.changes {
            if let Some(position) = mutation.initial_position {
                affected.insert(chunk_of(position));
            } else if let Some(entry) = self.entries.get(&mutation.id) {
                affected.insert(chunk_of(entry.position));
            }
        }
        let mut sizes = Vec::with_capacity(affected.len());
        for chunk in affected {
            let mut size = SHARD_HEADER + 4;
            if let Some(members) = self.chunk_members.get(&chunk) {
                for id in members {
                    let entry = &self.entries[id];
                    size += super::persistence::RECORD
                        + entry
                            .drop_payload()
                            .stack
                            .components
                            .as_ref()
                            .map_or(0, |component| component.bytes.len());
                }
            }
            for mutation in &plan.changes {
                let mutation_chunk = if let Some(position) = mutation.initial_position {
                    chunk_of(position)
                } else if let Some(entry) = self.entries.get(&mutation.id) {
                    chunk_of(entry.position)
                } else {
                    continue;
                };
                if mutation_chunk != chunk {
                    continue;
                }
                let old = mutation.before.is_empty().then_some(0).unwrap_or(
                    super::persistence::RECORD + mutation.before.len().saturating_sub(21),
                );
                let after = mutation.after.is_empty().then_some(0).unwrap_or(
                    super::persistence::RECORD + mutation.after.len().saturating_sub(21),
                );
                size = size.saturating_add(after).saturating_sub(old);
            }
            if size > MAX_SHARD_BYTES {
                return Err(invalid("drops shard snapshot too large"));
            }
            sizes.push((chunk, size));
        }
        Ok((sizes, plan.allocator.is_some()))
    }

    pub(in crate::server) fn apply_plan(&mut self, plan: &DropPlan) -> io::Result<()> {
        self.validate_plan(plan)?;
        for mutation in &plan.changes {
            if mutation.after.is_empty() {
                let position = self.entries.get(&mutation.id).map(|entry| entry.position);
                if self.entries.remove(&mutation.id).is_some() {
                    self.remove_entry_indexes(mutation.id);
                    self.index_remove(mutation.id, position.expect("removed drop has a position"));
                }
                self.active.remove(&mutation.id);
                continue;
            }
            let payload =
                journal::decode_owner_with_catalog(mutation.id, &mutation.after, &self.catalog)
                    .expect("prevalidated owner bytes");
            let born = payload.created_unix_ms;
            if let Some(entry) = self.entries.get_mut(&mutation.id) {
                entry.payload = payload.into_entity_payload();
                let age = Duration::from_millis(unix_ms().saturating_sub(born));
                entry.age_at_load = age;
                entry.age_since = Instant::now();
                self.expiry.insert(mutation.id, age, entry.age_since);
                let position = entry.position;
                // A count/ownership update must not wake a settled drop. It
                // can, however, make an entry ineligible for further physics
                // if its persisted age is already past the lifetime.
                if age >= LIFETIME {
                    self.active.remove(&mutation.id);
                }
                self.chunk_dirty.insert(chunk_of(position));
            } else {
                // `validate_plan` proved this before the first mutation, so the
                // application half is deliberately infallible.
                let position = mutation
                    .initial_position
                    .expect("prevalidated spawn position");
                let age = Duration::from_millis(unix_ms().saturating_sub(born));
                self.entries.insert(
                    mutation.id,
                    Entry::new(mutation.id, position, payload, 0.0, age, Instant::now()),
                );
                let entry = &self.entries[&mutation.id];
                self.spatial.insert(mutation.id, entry.position);
                self.expiry.insert(mutation.id, age, entry.age_since);
                if age < LIFETIME {
                    self.active.insert(mutation.id);
                }
                self.index_insert(mutation.id, position);
            }
        }
        if let Some((before, after)) = plan.allocator {
            debug_assert!(self.next_id == before || self.next_id == after);
            if self.next_id != after {
                self.allocator_dirty = true;
            }
            self.next_id = self.next_id.max(after);
        }
        if !plan.changes.is_empty() || plan.allocator.is_some() {
            self.revision = self.revision.wrapping_add(1);
        }
        Ok(())
    }
}
