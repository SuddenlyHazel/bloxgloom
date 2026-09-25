//! Live drop reads over the shared entity store.
//!
//! Drops are ordinary mobile entities of type [`DROP_ENTITY_TYPE`]; these
//! helpers project their snapshots onto the historic [`DroppedItem`] view.
//! Spatial queries that exceed their bound degrade to an empty frame and are
//! retried next tick: a capacity condition must never stop the coordinator,
//! and merge/pickup planning treats an unreadable neighbourhood as having no
//! merge targets or candidates, which conserves items by construction.

use super::entity::{DROP_ENTITY_TYPE, DropEntityPayload};
use super::{LIFETIME, PICKUP_RANGE_SQ, VIEW_RANGE, VIEW_RANGE_SQ, age_ms_now, unix_ms};
use crate::inventory::Stack;
use crate::protocol::DroppedItem;
use crate::server::entities::{EntityId, EntityLocation, EntityStore};
use crate::world::ChunkKey;

struct LiveDrop {
    id: EntityId,
    position: [f32; 3],
    payload: DropEntityPayload,
}

fn live_drop(store: &EntityStore, id: EntityId) -> Option<LiveDrop> {
    let snapshot = store.snapshot(id)?;
    if snapshot.entity_type != DROP_ENTITY_TYPE {
        return None;
    }
    let EntityLocation::Mobile { position } = snapshot.location else {
        return None;
    };
    let payload = snapshot
        .private_payload
        .downcast_ref::<DropEntityPayload>()
        .cloned()?;
    Some(LiveDrop {
        id,
        position,
        payload,
    })
}

fn collect_in_aabb(
    store: &EntityStore,
    min: [f32; 3],
    max: [f32; 3],
) -> Vec<LiveDrop> {
    let ids = store.query_mobile_aabb(min, max).unwrap_or_default();
    ids.into_iter().filter_map(|id| live_drop(store, id)).collect()
}

fn snapshot_item(drop: &LiveDrop, now_ms: u64) -> DroppedItem {
    DroppedItem {
        id: drop.id.get(),
        item: drop.payload.stack.item,
        count: drop.payload.stack.count,
        position: drop.position,
        age_ms: age_ms_now(drop.payload.created_unix_ms, now_ms),
    }
}

fn distance_sq(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// Bounded client-visibility projection: every drop within view range,
/// nearest first, capped at 256. This is presentation data; ownership lives
/// in the entity store and never depends on who observes it.
pub(in crate::server) fn nearby(store: &EntityStore, position: [f32; 3]) -> Vec<DroppedItem> {
    let now_ms = unix_ms();
    let min = position.map(|coordinate| coordinate - VIEW_RANGE);
    let max = position.map(|coordinate| coordinate + VIEW_RANGE);
    let mut items: Vec<_> = collect_in_aabb(store, min, max)
        .iter()
        .filter(|drop| distance_sq(drop.position, position) <= VIEW_RANGE_SQ)
        .map(|drop| snapshot_item(drop, now_ms))
        .collect();
    items.sort_by(|a, b| {
        distance_sq(a.position, position)
            .total_cmp(&distance_sq(b.position, position))
            .then(a.id.cmp(&b.id))
    });
    items.truncate(256);
    items
}

/// Drops one client may pick up right now: in range, past any pickup delay,
/// and not yet expired. Server age alone gates eligibility; no client timing
/// signal participates.
pub(in crate::server) fn pickup_candidates(
    store: &EntityStore,
    position: [f32; 3],
) -> Vec<DroppedItem> {
    let now_ms = unix_ms();
    let radius = PICKUP_RANGE_SQ.sqrt();
    let min = position.map(|coordinate| coordinate - radius);
    let max = position.map(|coordinate| coordinate + radius);
    let mut items: Vec<_> = collect_in_aabb(store, min, max)
        .iter()
        .filter(|drop| {
            let age = u128::from(now_ms.saturating_sub(drop.payload.created_unix_ms));
            age >= drop.payload.pickup_delay.as_millis()
                && age < LIFETIME.as_millis()
                && distance_sq(drop.position, position) <= PICKUP_RANGE_SQ
        })
        .map(|drop| snapshot_item(drop, now_ms))
        .collect();
    items.sort_by_key(|item| item.id);
    items.truncate(256);
    items
}

pub(in crate::server) fn stack(store: &EntityStore, id: EntityId) -> Option<Stack> {
    live_drop(store, id).map(|drop| drop.payload.stack)
}

/// Airborne drops are exactly the scheduled ones: settled drops suspend off
/// the sparse due schedule, so this counts queue membership, not records.
pub(in crate::server) fn airborne_count(store: &EntityStore) -> usize {
    store
        .due_entities(u64::MAX, usize::MAX)
        .into_iter()
        .filter(|id| {
            store
                .snapshot(*id)
                .is_some_and(|snapshot| snapshot.entity_type == DROP_ENTITY_TYPE)
        })
        .count()
}

/// True when at least one drop is past its lifetime. The coordinator calls
/// this through a one-second throttle; the scan exits on the first hit.
pub(in crate::server) fn has_expired(store: &EntityStore, now_ms: u64) -> bool {
    store.record_values().any(|record| {
        record.entity_type == DROP_ENTITY_TYPE
            && record
                .payload
                .downcast_ref::<DropEntityPayload>()
                .is_some_and(|payload| {
                    u128::from(now_ms.saturating_sub(payload.created_unix_ms))
                        >= LIFETIME.as_millis()
                })
    })
}

/// Sleeping (suspended-schedule) drops owned by one chunk. Block edits wake
/// these through durable wakes; already-scheduled drops need nothing.
pub(in crate::server) fn sleeping_drop_ids_in_chunk(
    store: &EntityStore,
    chunk: ChunkKey,
) -> Vec<EntityId> {
    store
        .ids_for_chunk(chunk)
        .into_iter()
        .filter(|id| {
            store.snapshot(*id).is_some_and(|snapshot| {
                snapshot.entity_type == DROP_ENTITY_TYPE && snapshot.next_tick.is_none()
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "queries/tests.rs"]
mod tests;
