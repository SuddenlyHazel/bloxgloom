//! Authoritative chunk loading and bounded per-tick interest publication.
//!
//! No path here reads or generates a chunk on the coordinator. Loader workers
//! supply resident chunks; publication workers prepare snapshots and the socket
//! reactor's codec pool serializes shared frames.

use super::chunk_loader::{RequestError, RequestStatus};
use super::metrics::LatencyEvent;
use super::outbound::{
    OUTBOUND_CLIENT_BYTE_CAPACITY, OUTBOUND_FRAME_CAPACITY, OutboundClientSnapshot,
};
use super::*;

pub(in crate::server) mod entities;
mod interest_projection;
pub(in crate::server) mod shared;
pub(in crate::server) mod snapshots;
pub(in crate::server) mod subscriptions;
pub(in crate::server) mod workers;

const MAX_LOAD_RESULTS_PER_TICK: usize = 32;
const MAX_PREFETCH_CANDIDATES: usize = 16;
const MAX_NEW_LOADS_PER_CLIENT: usize = 2;
const SNAPSHOT_FRAME_HEADROOM: usize = 32;
const SNAPSHOT_BYTE_HEADROOM: u64 = 512 * 1024;

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
    interest_projection::reduce_view_under_pressure(state)?;
    let mut ids: Vec<_> = state.clients.keys().copied().collect();
    if ids.is_empty() {
        return Ok(());
    }
    ids.sort_unstable();
    let start = state.stream_cursor as usize % ids.len();
    ids.rotate_left(start);
    state.stream_cursor = state.stream_cursor.wrapping_add(1);
    let mut snapshots = snapshots::Selection::default();
    interest_projection::prepare(state, &ids, &mut snapshots)?;
    snapshots::publish(state, snapshots)?;
    Ok(())
}

fn stream_candidates(
    state: &mut State,
    id: u64,
    candidates: Vec<ChunkKey>,
    snapshots: &mut snapshots::Selection,
) -> io::Result<()> {
    let Some(client) = state.clients.get_mut(&id) else {
        return Ok(());
    };
    let mut sent_chunk = false;
    let mut new_loads = 0;
    for key in candidates {
        if state.world.cached_version(key).is_some() {
            if !sent_chunk {
                snapshots.select(id, client, key);
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
    Ok(())
}

#[cfg(test)]
fn can_stream_snapshot(
    queued: OutboundClientSnapshot,
    messages: &[ServerMessage],
) -> io::Result<bool> {
    let bytes = messages.iter().try_fold(0_u64, |total, message| {
        total.checked_add(crate::protocol::server_wire_len(message) as u64)
    });
    let bytes = bytes.ok_or_else(|| io::Error::other("chunk snapshot byte count overflow"))?;
    can_stream_snapshot_size(queued, messages.len(), bytes)
}

fn can_stream_snapshot_size(
    queued: OutboundClientSnapshot,
    frames: usize,
    bytes: u64,
) -> io::Result<bool> {
    if frames > OUTBOUND_FRAME_CAPACITY || bytes > OUTBOUND_CLIENT_BYTE_CAPACITY {
        return Err(io::Error::other(
            "one chunk snapshot exceeds the outbound queue bound",
        ));
    }
    let frame_limit = (OUTBOUND_FRAME_CAPACITY - SNAPSHOT_FRAME_HEADROOM).max(frames);
    let byte_limit = (OUTBOUND_CLIENT_BYTE_CAPACITY - SNAPSHOT_BYTE_HEADROOM).max(bytes);
    Ok(queued.queued_frames.saturating_add(frames) <= frame_limit
        && queued.queued_bytes.saturating_add(bytes) <= byte_limit)
}

fn inside_view(key: ChunkKey, center: ChunkKey, radius: i64) -> bool {
    (i64::from(key.x) - i64::from(center.x)).abs() <= radius
        && (i64::from(key.y) - i64::from(center.y)).abs() <= 1
        && (i64::from(key.z) - i64::from(center.z)).abs() <= radius
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
