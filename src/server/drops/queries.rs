//! Live drop reads over the shared entity store.
//!
//! Drops are ordinary mobile entities of type [`DROP_ENTITY_TYPE`]; these
//! helpers project their snapshots onto the historic [`DroppedItem`] view.
//! Gameplay queries treat an unreadable neighbourhood as having no merge or
//! pickup candidates, conserving ownership. Replication uses immutable mobile
//! pages and reports capacity explicitly, so an impossible view closes only its
//! observer rather than stopping the coordinator or sending a false empty frame.

#[cfg(test)]
use super::LIFETIME;
use super::entity::{DROP_ENTITY_TYPE, DropEntityPayload};
use super::{VIEW_RANGE, VIEW_RANGE_SQ, age_ms_now, unix_ms};
use crate::content::Catalog;
use crate::inventory::Stack;
use crate::protocol::DroppedItem;
use crate::server::entities::{EntityId, EntityLocation, EntityStore, MobilePage};
use crate::world::ChunkKey;
use bloxgloom_host_api::entity::DropPickupContext;

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

fn collect_in_aabb(store: &EntityStore, min: [f32; 3], max: [f32; 3]) -> Vec<LiveDrop> {
    let ids = store.query_mobile_aabb(min, max).unwrap_or_default();
    ids.into_iter()
        .filter_map(|id| live_drop(store, id))
        .collect()
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

fn pickup_policy(payload: &DropEntityPayload, catalog: &Catalog, now_ms: u64) -> DropPickupContext {
    DropPickupContext {
        age_ms: now_ms.saturating_sub(payload.created_unix_ms),
        delay_ms: payload
            .pickup_delay
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX),
        lifetime_ms: catalog.drop_policy(payload.stack.item).lifetime_ms,
    }
}

/// Bounded client-visibility projection: every drop within view range,
/// nearest first, capped at 256. This is presentation data; ownership lives
/// in the entity store and never depends on who observes it.
#[cfg(test)]
pub(in crate::server) fn nearby(store: &EntityStore, position: [f32; 3]) -> Vec<DroppedItem> {
    capture_nearby(store, position)
        .map(|pages| project_nearby(pages, position, unix_ms()))
        .unwrap_or_default()
}

/// At most 9^3 chunk-page handles and 65,536 candidate records. Oversized
/// regions are explicit to live replication, not an empty successful frame.
pub(in crate::server) fn capture_nearby(
    store: &EntityStore,
    position: [f32; 3],
) -> Result<Vec<MobilePage>, ()> {
    let min = position.map(|v| ((v - VIEW_RANGE) / 16.0).floor() as i32);
    let max = position.map(|v| ((v + VIEW_RANGE) / 16.0).floor() as i32);
    if min
        .iter()
        .zip(max)
        .any(|(&a, b)| i64::from(b) - i64::from(a) > 8)
    {
        return Err(());
    }
    let mut pages = Vec::new();
    let mut count = 0usize;
    for x in min[0]..=max[0] {
        for y in min[1]..=max[1] {
            for z in min[2]..=max[2] {
                if let Some(page) = store.mobile_publication_page(ChunkKey { x, y, z }) {
                    count += page.len();
                    if count > 65_536 {
                        return Err(());
                    }
                    pages.push(page);
                }
            }
        }
    }
    Ok(pages)
}

pub(in crate::server) fn project_nearby(
    pages: Vec<MobilePage>,
    position: [f32; 3],
    now_ms: u64,
) -> Vec<DroppedItem> {
    struct Candidate(f32, DroppedItem);
    impl PartialEq for Candidate {
        fn eq(&self, other: &Self) -> bool {
            self.cmp(other).is_eq()
        }
    }
    impl Eq for Candidate {}
    impl PartialOrd for Candidate {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }
    impl Ord for Candidate {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            self.0.total_cmp(&other.0).then(self.1.id.cmp(&other.1.id))
        }
    }
    let mut nearest = std::collections::BinaryHeap::with_capacity(257);
    for page in pages {
        for snapshot in page.values() {
            if snapshot.entity_type != DROP_ENTITY_TYPE {
                continue;
            }
            let EntityLocation::Mobile {
                position: drop_position,
            } = snapshot.location
            else {
                continue;
            };
            let distance = distance_sq(drop_position, position);
            if distance > VIEW_RANGE_SQ {
                continue;
            }
            let Some(payload) = snapshot.private_payload.downcast_ref::<DropEntityPayload>() else {
                continue;
            };
            nearest.push(Candidate(
                distance,
                DroppedItem {
                    id: snapshot.id.get(),
                    item: payload.stack.item,
                    count: payload.stack.count,
                    position: drop_position,
                    age_ms: age_ms_now(payload.created_unix_ms, now_ms),
                },
            ));
            if nearest.len() > 256 {
                nearest.pop();
            }
        }
    }
    nearest
        .into_sorted_vec()
        .into_iter()
        .map(|candidate| candidate.1)
        .collect()
}

/// Drops one client may pick up right now: in range, past any pickup delay,
/// and not yet expired. Server age alone gates eligibility; no client timing
/// signal participates.
pub(in crate::server) fn pickup_candidates(
    store: &EntityStore,
    catalog: &Catalog,
    position: [f32; 3],
) -> Vec<DroppedItem> {
    let now_ms = unix_ms();
    let radius = catalog.max_drop_pickup_range();
    let min = position.map(|coordinate| coordinate - radius);
    let max = position.map(|coordinate| coordinate + radius);
    let mut items: Vec<_> = collect_in_aabb(store, min, max)
        .iter()
        .filter(|drop| {
            pickup_policy(&drop.payload, catalog, now_ms).in_range(
                position,
                drop.position,
                catalog
                    .drop_policy(drop.payload.stack.item)
                    .pickup_range
                    .powi(2),
            )
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

/// One candidate recheck for the public pickup convenience operation. The
/// server supplies the authenticated actor position and clock, not script data.
pub(in crate::server) fn pickup_eligible(
    store: &EntityStore,
    catalog: &Catalog,
    raw_id: u64,
    position: [f32; 3],
) -> bool {
    EntityId::new(raw_id)
        .and_then(|id| live_drop(store, id))
        .is_some_and(|drop| {
            pickup_policy(&drop.payload, catalog, unix_ms()).in_range(
                position,
                drop.position,
                catalog
                    .drop_policy(drop.payload.stack.item)
                    .pickup_range
                    .powi(2),
            )
        })
}

/// The same server-clock eligibility used by ordinary pickup. Inspecting a
/// drop is allowed earlier; extraction through the shared inventory service is
/// not, so scripts cannot silently bypass its pickup delay or expiration.
pub(in crate::server) fn extractable(store: &EntityStore, catalog: &Catalog, id: EntityId) -> bool {
    let Some(drop) = live_drop(store, id) else {
        return false;
    };
    pickup_policy(&drop.payload, catalog, unix_ms()).extractable()
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
/// this through a one-second throttle; the derived due index inspects one entry.
pub(in crate::server) fn has_expired(store: &EntityStore, now_ms: u64) -> bool {
    !store.expired_entities(now_ms, 1).is_empty()
}

#[cfg(test)]
#[path = "queries/tests.rs"]
mod tests;
