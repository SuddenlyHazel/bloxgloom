//! Durable drop transaction planning over the shared entity store.
//!
//! Every planner below stages exact before/after preimages through
//! [`PreparedEntityBatch`], so the coordinator can combine drop changes with
//! inventory and world edits in one WAL record. Spawns share the single
//! [`EntityStore`] allocator; merge targets resolve in stable ID order.

use std::collections::BTreeMap;
use std::io;
use std::time::Duration;

use super::entity::{DROP_ENTITY_TYPE, DropEntityPayload};
use super::{LIFETIME, invalid};
use crate::content::Catalog;
use crate::inventory::{MAX_COMPONENT_BYTES, STACK_LIMIT, Stack};
use crate::items::ItemId;
use crate::server::entities::{
    EntityError, EntityId, EntityLocation, EntityPatch, EntitySpawn, EntityStore,
    PreparedEntityBatch,
};
use bloxgloom_host_api::entity::{
    DropLifetime, DropMergeCandidate, DropMergeContext, DropStackFill,
};

/// Plans harvest/drop-stack creation without mutating authoritative state.
/// Existing merge targets resolve in stable ID order; newly allocated IDs
/// follow the store allocator deterministically. Returns `None` when every
/// requested count is zero.
#[cfg(test)]
pub(in crate::server) fn plan_spawn_stack(
    store: &EntityStore,
    catalog: &Catalog,
    position: [f32; 3],
    stack: Stack,
    pickup_delay: Duration,
    spawn_tick: u64,
    now_ms: u64,
) -> io::Result<Option<PreparedEntityBatch>> {
    plan_stack_spawns(
        store,
        catalog,
        &[(position, stack, pickup_delay)],
        spawn_tick,
        now_ms,
    )
}

/// Plans a group of loot outputs as one atomic delta. Same-item outputs
/// share merge targets and newly allocated IDs deterministically.
pub(in crate::server) fn plan_spawns(
    store: &EntityStore,
    catalog: &Catalog,
    spawns: &[([f32; 3], ItemId, u16, Duration)],
    spawn_tick: u64,
    now_ms: u64,
) -> io::Result<Option<PreparedEntityBatch>> {
    plan_spawns_with_extra(store, catalog, spawns, Vec::new(), spawn_tick, now_ms)
}

/// Loot planning that also stages caller-owned spawns (for example a kiln
/// placement) in the same atomic batch. Extra spawns allocate after every
/// drop so drop ID prediction stays exact; they never merge.
pub(in crate::server) fn plan_spawns_with_extra(
    store: &EntityStore,
    catalog: &Catalog,
    spawns: &[([f32; 3], ItemId, u16, Duration)],
    extra_spawns: Vec<EntitySpawn>,
    spawn_tick: u64,
    now_ms: u64,
) -> io::Result<Option<PreparedEntityBatch>> {
    let stacks: Vec<_> = spawns
        .iter()
        .map(|&(position, item, count, delay)| (position, Stack::new(item, count), delay))
        .collect();
    plan_stack_spawns_with_extra(store, catalog, &stacks, extra_spawns, spawn_tick, now_ms)
}

pub(in crate::server) fn plan_stack_spawns(
    store: &EntityStore,
    catalog: &Catalog,
    spawns: &[([f32; 3], Stack, Duration)],
    spawn_tick: u64,
    now_ms: u64,
) -> io::Result<Option<PreparedEntityBatch>> {
    plan_stack_spawns_with_extra(store, catalog, spawns, Vec::new(), spawn_tick, now_ms)
}

pub(in crate::server) fn plan_stack_spawns_with_extra(
    store: &EntityStore,
    catalog: &Catalog,
    spawns: &[([f32; 3], Stack, Duration)],
    extra_spawns: Vec<EntitySpawn>,
    spawn_tick: u64,
    now_ms: u64,
) -> io::Result<Option<PreparedEntityBatch>> {
    // Merges accumulate per live ID; remainder becomes new allocations with
    // predicted IDs. Prediction is exact: planning runs on the coordinator
    // against live state with no interleaving before preparation, and the
    // prepared batch is re-checked against the allocator below.
    let mut merged = BTreeMap::<EntityId, (Stack, Duration)>::new();
    let mut fresh: Vec<PlannedNew> = Vec::new();
    let first_id = store.next_id();
    let mut next_id = first_id;
    for (position, stack, pickup_delay) in spawns {
        if stack.count == 0 {
            continue;
        }
        // Millisecond-exact round trip through the durable u32 range.
        let pickup_delay_ms = u32::try_from(pickup_delay.as_millis())
            .map_err(|_| invalid("drop pickup delay exceeds durable range"))?;
        // Spawn rows carry pre-split totals, so the per-stack cap does not
        // apply here: every allocated stack below is capped individually and
        // re-validated by preparation. Item identity, nonzero count, and
        // component framing are still rejected fail-closed.
        if !spawn_stack_valid(stack, catalog)
            || position.iter().any(|coordinate| !coordinate.is_finite())
            || Duration::from_millis(u64::from(pickup_delay_ms)) != *pickup_delay
        {
            return Err(invalid("invalid durable drop spawn"));
        }
        let mut count = stack.count;
        while count > 0 {
            let target = merge_target(store, &merged, &fresh, *position, stack, now_ms)?;
            if let Some(id) = target {
                let (entry, from_fresh) = if let Some(entry) = fresh.iter_mut().find(|e| e.id == id)
                {
                    (&mut entry.stack, true)
                } else {
                    let slot = merged.entry(id).or_insert_with(|| {
                        let live =
                            super::queries::stack(store, id).expect("merge target is a live drop");
                        (live, *pickup_delay)
                    });
                    (&mut slot.0, false)
                };
                debug_assert_eq!(entry.item, stack.item);
                debug_assert_eq!(entry.components, stack.components);
                let fill = DropStackFill::plan(count, entry.count, STACK_LIMIT)
                    .ok_or_else(|| invalid("invalid durable drop merge count"))?;
                entry.count = fill.final_count;
                if !from_fresh {
                    merged.get_mut(&id).expect("merged slot exists").1 = *pickup_delay;
                }
                count = fill.remaining;
                continue;
            }
            let fill = DropStackFill::plan(count, 0, STACK_LIMIT)
                .ok_or_else(|| invalid("invalid durable drop split count"))?;
            let id = EntityId::new(next_id).ok_or_else(|| invalid("invalid durable drop spawn"))?;
            next_id = next_id
                .checked_add(1)
                .filter(|next| *next != 0)
                .ok_or_else(|| io::Error::other("drop ID space exhausted"))?;
            fresh.push(PlannedNew {
                id,
                position: *position,
                stack: Stack {
                    item: stack.item,
                    count: fill.final_count,
                    components: stack.components.clone(),
                },
                delay: *pickup_delay,
            });
            count = fill.remaining;
        }
    }
    if merged.is_empty() && fresh.is_empty() && extra_spawns.is_empty() {
        return Ok(None);
    }
    let mut transactions = Vec::new();
    let mut fresh_spawns = Vec::with_capacity(fresh.len());
    for (id, (stack, delay)) in merged {
        let snapshot = store
            .snapshot(id)
            .ok_or_else(|| invalid("drop changed before durable spawn"))?;
        let mut payload = snapshot
            .private_payload
            .downcast_ref::<DropEntityPayload>()
            .cloned()
            .ok_or_else(|| invalid("drop changed before durable spawn"))?;
        payload.stack = stack;
        payload.created_unix_ms = now_ms;
        payload.pickup_delay = delay;
        transactions.push(
            store
                .prepare_update(
                    id,
                    snapshot.revision,
                    EntityPatch {
                        payload: Some(payload.into_entity_payload()),
                        next_tick: None,
                        position: None,
                    },
                )
                .map_err(plan_error)?,
        );
    }
    for entry in &fresh {
        fresh_spawns.push(EntitySpawn::Mobile {
            entity_type: DROP_ENTITY_TYPE,
            position: entry.position,
            payload: DropEntityPayload::new(entry.stack.clone(), now_ms, entry.delay)
                .into_entity_payload(),
            spawn_tick,
        });
    }
    fresh_spawns.extend(extra_spawns);
    if !fresh_spawns.is_empty() {
        let batch = store
            .prepare_spawn_batch(fresh_spawns)
            .map_err(plan_error)?;
        // The allocator must hand out exactly the predicted sequence:
        // drops first, extras after. Anything else means live state moved
        // under the plan and the whole action must not stage.
        let expected: Option<Vec<EntityId>> = (0..batch.entity_ids().len())
            .map(|offset| first_id.checked_add(offset as u64).and_then(EntityId::new))
            .collect();
        if Some(batch.entity_ids()) != expected {
            return Err(invalid("drop allocator changed before durable apply"));
        }
        transactions.push(batch);
    }
    let mut transaction = store.combine_prepared(transactions).map_err(plan_error)?;
    // Keep merge ordering and negative spatial reads stable through pending
    // commits. Same-owner motion can enter the radius without changing page
    // membership, hence the per-member motion/record dependencies too.
    for (position, stack, _) in spawns {
        if stack.count != 0 {
            transaction
                .add_dependencies(
                    store
                        .capture_mobile_dependencies(*position, 1.0)
                        .map_err(plan_error)?,
                )
                .map_err(plan_error)?;
        }
    }
    Ok(Some(transaction))
}

struct PlannedNew {
    id: EntityId,
    position: [f32; 3],
    stack: Stack,
    delay: Duration,
}

/// Capture eligible spatial candidates from the authoritative entity store and
/// use the public merge policy for identity, age, radius, cap and ID selection.
/// Newly allocated IDs exceed live IDs, so a single minimum-ID selection keeps
/// the historic ordering without sorting the captured candidates.
fn merge_target(
    store: &EntityStore,
    merged: &BTreeMap<EntityId, (Stack, Duration)>,
    fresh: &[PlannedNew],
    position: [f32; 3],
    stack: &Stack,
    now_ms: u64,
) -> io::Result<Option<EntityId>> {
    let min = position.map(|coordinate| coordinate - 1.0);
    let max = position.map(|coordinate| coordinate + 1.0);
    let live = store.query_mobile_aabb(min, max).map_err(plan_error)?;
    let live = live.into_iter().filter_map(|id| {
        let snapshot = store.snapshot(id)?;
        if snapshot.entity_type != DROP_ENTITY_TYPE {
            return None;
        }
        let EntityLocation::Mobile { position: at } = snapshot.location else {
            return None;
        };
        let payload = snapshot
            .private_payload
            .downcast_ref::<DropEntityPayload>()?;
        // A drop already merged in this plan has a refreshed birth and count.
        let (current, age_ms) = merged.get(&id).map_or_else(
            || {
                (
                    &payload.stack,
                    now_ms.saturating_sub(payload.created_unix_ms),
                )
            },
            |(current, _)| (current, 0),
        );
        Some(DropMergeCandidate {
            id: id.get(),
            position: at,
            count: current.count,
            age_ms,
            same_stack: current.item == stack.item && current.components == stack.components,
        })
    });
    let planned = fresh.iter().map(|entry| DropMergeCandidate {
        id: entry.id.get(),
        position: entry.position,
        count: entry.stack.count,
        age_ms: 0,
        same_stack: entry.stack.item == stack.item && entry.stack.components == stack.components,
    });
    Ok(DropMergeContext {
        position,
        radius: 1.0,
        lifetime_ms: LIFETIME
            .as_millis()
            .try_into()
            .expect("drop lifetime fits u64"),
        stack_limit: STACK_LIMIT,
    }
    .select(live.chain(planned))
    .and_then(EntityId::new))
}

/// Plans count transfers out of selected drops. A full removal stages a
/// despawn, so replay cannot resurrect a picked or expired ID.
pub(in crate::server) fn plan_take(
    store: &EntityStore,
    takes: &[(EntityId, u16)],
) -> io::Result<Option<PreparedEntityBatch>> {
    let mut amounts = BTreeMap::<EntityId, u16>::new();
    for &(id, count) in takes {
        if count == 0 {
            continue;
        }
        let snapshot = store
            .snapshot(id)
            .ok_or_else(|| invalid("drop changed before durable take"))?;
        let live = snapshot
            .private_payload
            .downcast_ref::<DropEntityPayload>()
            .ok_or_else(|| invalid("drop changed before durable take"))?;
        if snapshot.entity_type != DROP_ENTITY_TYPE
            || !matches!(&snapshot.location, EntityLocation::Mobile { .. })
        {
            return Err(invalid("drop changed before durable take"));
        }
        let amount = amounts.entry(id).or_default();
        *amount = amount.saturating_add(count).min(live.stack.count);
    }
    if amounts.is_empty() {
        return Ok(None);
    }
    let mut transactions = Vec::with_capacity(amounts.len());
    for (id, amount) in amounts {
        let snapshot = store
            .snapshot(id)
            .ok_or_else(|| invalid("drop changed before durable take"))?;
        let live = snapshot
            .private_payload
            .downcast_ref::<DropEntityPayload>()
            .cloned()
            .ok_or_else(|| invalid("drop changed before durable take"))?;
        if amount >= live.stack.count {
            transactions.push(
                store
                    .prepare_despawn(id, snapshot.revision)
                    .map_err(plan_error)?,
            );
        } else {
            let mut after = live;
            after.stack.count -= amount;
            transactions.push(
                store
                    .prepare_update(
                        id,
                        snapshot.revision,
                        EntityPatch {
                            payload: Some(after.into_entity_payload()),
                            next_tick: None,
                            position: None,
                        },
                    )
                    .map_err(plan_error)?,
            );
        }
    }
    store
        .combine_prepared(transactions)
        .map_err(plan_error)
        .map(Some)
}

/// Plans removal of drops past their lifetime, oldest IDs first, bounded to
/// `limit` per call so one expiry round stays one bounded WAL record.
pub(in crate::server) fn plan_expired(
    store: &EntityStore,
    now_ms: u64,
    limit: usize,
) -> io::Result<Option<PreparedEntityBatch>> {
    let mut transactions = Vec::new();
    for record in store.record_values() {
        if transactions.len() >= limit {
            break;
        }
        if record.entity_type != DROP_ENTITY_TYPE {
            continue;
        }
        let expired = record
            .payload
            .downcast_ref::<DropEntityPayload>()
            .is_some_and(|payload| {
                DropLifetime {
                    created_ms: payload.created_unix_ms,
                    now_ms,
                    lifetime_ms: LIFETIME
                        .as_millis()
                        .try_into()
                        .expect("drop lifetime fits u64"),
                }
                .expired()
            });
        if !expired {
            continue;
        }
        transactions.push(
            store
                .prepare_despawn(record.id, record.revision)
                .map_err(plan_error)?,
        );
    }
    if transactions.is_empty() {
        return Ok(None);
    }
    store
        .combine_prepared(transactions)
        .map_err(plan_error)
        .map(Some)
}

/// Entity preparation failures are capacity or contention outcomes, never
/// corruption: surface them as `InvalidInput` so the coordinator defers or
/// rejects instead of stopping. Genuine corruption keeps `InvalidData`.
fn plan_error(error: EntityError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, error)
}

fn spawn_stack_valid(stack: &Stack, catalog: &Catalog) -> bool {
    stack.count >= 1
        && catalog.item(stack.item).is_some()
        && stack.components.as_ref().is_none_or(|components| {
            components.version != 0
                && !components.bytes.is_empty()
                && components.bytes.len() <= MAX_COMPONENT_BYTES
        })
}

#[cfg(test)]
#[path = "planning/tests.rs"]
mod tests;
