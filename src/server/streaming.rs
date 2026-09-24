//! Authoritative chunk loading and bounded per-tick interest publication.
//!
//! No path here reads or generates a chunk on the coordinator. Loader workers
//! supply resident chunks; the socket reactor serializes them.

use super::chunk_loader::{RequestError, RequestStatus};
use super::metrics::LatencyEvent;
use super::*;

pub(in crate::server) mod entities;

const MAX_LOAD_RESULTS_PER_TICK: usize = 32;
const MAX_PREFETCH_CANDIDATES: usize = 16;
const MAX_NEW_LOADS_PER_CLIENT: usize = 2;

pub(super) fn poll_chunk_loads(state: &mut State) -> io::Result<()> {
    for _ in 0..MAX_LOAD_RESULTS_PER_TICK {
        let completion = match state.loader.try_recv() {
            Ok(completion) => completion,
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => {
                return Err(io::Error::other("authoritative chunk loader stopped"));
            }
        };
        state.metrics.record_latency(
            LatencyEvent::ChunkLoad,
            completion.ticket.requested_at.elapsed(),
        );
        match completion.result {
            Ok(loaded) => {
                state
                    .world
                    .install_loaded_if_absent(loaded, completion.ticket.edit_epoch)?;
            }
            Err(error) => {
                state
                    .world
                    .finish_chunk_load(completion.ticket.key, completion.ticket.edit_epoch);
                return Err(io::Error::new(
                    error.kind(),
                    format!(
                        "load authoritative chunk {:?}: {error}",
                        completion.ticket.key
                    ),
                ));
            }
        }
    }
    Ok(())
}

/// Returns false only when the fixed loader budget is currently full.
pub(super) fn request_chunk(state: &mut State, key: ChunkKey) -> io::Result<bool> {
    if state.world.cached_version(key).is_some() {
        return Ok(true);
    }
    if !state.world.can_request_chunk(key) {
        return Ok(false);
    }
    match state.loader.request(&mut state.world, key) {
        Ok(RequestStatus::Enqueued(_) | RequestStatus::AlreadyPending(_)) => Ok(true),
        Err(RequestError::QueueFull) => Ok(false),
        Err(error) => Err(io::Error::other(format!(
            "request authoritative chunk {key:?}: {error}"
        ))),
    }
}

pub(super) fn publish_streams(state: &mut State) -> io::Result<()> {
    reduce_view_under_pressure(state);
    let mut ids: Vec<_> = state.clients.keys().copied().collect();
    if ids.is_empty() {
        return Ok(());
    }
    ids.sort_unstable();
    let start = state.stream_cursor as usize % ids.len();
    ids.rotate_left(start);
    state.stream_cursor = state.stream_cursor.wrapping_add(1);
    for id in ids {
        if !stream_one(state, id)? {
            state.remove_client(id);
        }
    }
    Ok(())
}

fn stream_one(state: &mut State, id: u64) -> io::Result<bool> {
    let Some(client) = state.clients.get_mut(&id) else {
        return Ok(false);
    };
    let position = client.position();
    let anchor = position.map(|coordinate| (coordinate / 4.0).floor() as i32);
    let drop_revision = state.drops.revision();
    if client.last_drops_revision != drop_revision || client.last_drop_anchor != anchor {
        let items = state.drops.nearby(position);
        if !same_drop_positions(&items, &client.last_sent_drops) {
            if !client.enqueue(ServerMessage::Drops {
                revision: drop_revision,
                items: items.clone(),
            }) {
                return Ok(false);
            }
            client.last_sent_drops = items;
        }
        client.last_drops_revision = drop_revision;
        client.last_drop_anchor = anchor;
    }

    let center = client.center;
    let radius = i64::from(client.radius);
    let mut released = Vec::new();
    client.sent.retain(|key| {
        let keep = inside_view(*key, center, radius);
        if !keep {
            released.push(*key);
        }
        keep
    });
    for key in released {
        let unpinned = state.world.unpin_resident_chunk(key);
        debug_assert!(unpinned, "out-of-view subscription lost its resident chunk");
    }
    client
        .sent_epochs
        .retain(|key, _| client.sent.contains(key));
    client
        .sent_block_versions
        .retain(|key, _| client.sent.contains(key));
    client
        .sent_entity_revisions
        .retain(|key, _| client.sent.contains(key));
    let candidates =
        interest::nearest_unsent(center, client.radius, &client.sent, MAX_PREFETCH_CANDIDATES);
    let mut sent_chunk = false;
    let mut new_loads = 0;
    for key in candidates {
        if state.world.cached_version(key).is_some() {
            if !sent_chunk {
                let chunk = state
                    .world
                    .cached_chunk(key)
                    .expect("resident version has a resident chunk");
                let epoch = client.next_snapshot_epoch;
                let next_epoch = epoch
                    .checked_add(1)
                    .ok_or_else(|| io::Error::other("client snapshot epoch exhausted"))?;
                let mut views = state
                    .entities
                    .public_views_for_chunk_bounded(key, entities::MAX_PUBLIC_ENTITIES_PER_CHUNK)
                    .map_err(io::Error::other)?;
                let remaining = entities::MAX_PUBLIC_ENTITIES_PER_CHUNK.saturating_sub(views.len());
                views.extend(
                    state
                        .player_entities
                        .public_views_for_chunk_bounded(key, remaining)
                        .map_err(io::Error::other)?,
                );
                let messages = entities::snapshot_messages(
                    chunk,
                    epoch,
                    state.entity_public_revision,
                    views,
                    state.world.catalog(),
                )?;
                let block_version = match &messages[0] {
                    ServerMessage::WorldSnapshotStart(start) => start.chunk.version,
                    _ => unreachable!("snapshot builder starts with chunk"),
                };
                let entity_revision = state.entity_public_revision;
                for message in messages {
                    if !client.enqueue(message) {
                        return Ok(false);
                    }
                }
                if !state.world.pin_resident_chunk(key) {
                    return Err(io::Error::other(
                        "snapshot chunk was evicted before subscription pin",
                    ));
                }
                client.next_snapshot_epoch = next_epoch;
                client.sent.insert(key);
                client.sent_epochs.insert(key, epoch);
                client.sent_block_versions.insert(key, block_version);
                client.sent_entity_revisions.insert(key, entity_revision);
                sent_chunk = true;
            }
            continue;
        }
        if new_loads == MAX_NEW_LOADS_PER_CLIENT {
            continue;
        }
        if !state.world.can_request_chunk(key) {
            break;
        }
        match state.loader.request(&mut state.world, key) {
            Ok(RequestStatus::Enqueued(_)) => new_loads += 1,
            Ok(RequestStatus::AlreadyPending(_)) => {}
            Err(RequestError::QueueFull) => break,
            Err(error) => {
                return Err(io::Error::other(format!("stream chunk {key:?}: {error}")));
            }
        }
    }
    Ok(true)
}

fn inside_view(key: ChunkKey, center: ChunkKey, radius: i64) -> bool {
    (i64::from(key.x) - i64::from(center.x)).abs() <= radius
        && (i64::from(key.y) - i64::from(center.y)).abs() <= 1
        && (i64::from(key.z) - i64::from(center.z)).abs() <= radius
}

/// A full pinned cache cannot accept a new authoritative owner. Reduce one
/// interested client's effective view and tell it before releasing its pins.
/// This is a bounded pressure response, not silent eviction of subscriptions.
fn reduce_view_under_pressure(state: &mut State) {
    if state.world.can_admit_chunk() {
        return;
    }
    let candidate = state
        .clients
        .iter()
        .filter(|(_, client)| client.radius > MIN_VIEW_DISTANCE)
        .map(|(&id, client)| {
            let next = i64::from(client.radius - 1);
            let reclaim = client
                .sent
                .iter()
                .filter(|&&key| !inside_view(key, client.center, next))
                .count();
            (id, reclaim, client.sent.len(), client.radius)
        })
        .max_by_key(|&(id, reclaim, sent, radius)| (reclaim > 0, radius, reclaim, sent, id));
    let Some((id, _, _, _)) = candidate else {
        return;
    };
    let client = state.clients.get_mut(&id).expect("selected live client");
    client.radius -= 1;
    if !client.enqueue(ServerMessage::ViewDistance {
        radius: client.radius,
    }) {
        state.remove_client(id);
        return;
    }
    let center = client.center;
    let radius = i64::from(client.radius);
    let mut released = Vec::new();
    client.sent.retain(|key| {
        let keep = inside_view(*key, center, radius);
        if !keep {
            released.push(*key);
        }
        keep
    });
    client
        .sent_epochs
        .retain(|key, _| client.sent.contains(key));
    client
        .sent_block_versions
        .retain(|key, _| client.sent.contains(key));
    client
        .sent_entity_revisions
        .retain(|key, _| client.sent.contains(key));
    for key in released {
        let unpinned = state.world.unpin_resident_chunk(key);
        debug_assert!(unpinned, "view reduction lost its resident chunk");
    }
}

pub(super) fn same_drop_positions(a: &[DroppedItem], b: &[DroppedItem]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| {
            a.id == b.id && a.item == b.item && a.count == b.count && a.position == b.position
        })
}

#[cfg(test)]
#[path = "streaming/tests.rs"]
mod tests;
