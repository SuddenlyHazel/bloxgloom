//! Receipt-ordered immutable effect -> worker fanout -> validated queue admission.
//! Drain one effect completely before the next. No worker completion order can
//! reorder a transaction, its inventory result, or its pickup presentation event.
use super::{PublishEffects, commit};
use crate::protocol::ServerMessage;
use crate::server::State;
use crate::server::outbound::{
    OUTBOUND_CLIENT_BYTE_CAPACITY, OUTBOUND_FRAME_CAPACITY, SharedMessage,
};
use crate::server::streaming::subscriptions::Capture;
use crate::server::streaming::workers::{Job, WIDTH};
use crate::world::ChunkKey;
use std::io;
use std::net::Shutdown;
use std::sync::Arc;

#[path = "dispatch/shared_parts.rs"]
mod shared_parts;
use shared_parts::SharedParts;

struct Prepared {
    capture: Capture,
    frames: Vec<Arc<SharedMessage>>,
    fire: Option<Arc<SharedMessage>>,
    revisions: Vec<(ChunkKey, u64, u64)>,
    resync: Vec<ChunkKey>,
}

pub(super) fn publish(state: &mut State) -> io::Result<()> {
    let effects = std::mem::take(&mut state.durability.publish_queue);
    for effect in effects {
        let effect = Arc::new(effect);
        let input = Arc::clone(&effect);
        let catalog = state.world.catalog_arc();
        let mut collected = state
            .publication_workers
            .run(vec![Box::new(move || commit::collect(&input, &catalog))])?;
        let changes = match collected.pop().expect("one collection job") {
            Ok(changes) => changes?,
            Err(()) => None, // failed projection: resnapshot, never lose the effect's reliable reply
        };
        let commit_id = if changes
            .as_ref()
            .is_some_and(commit::CommitChanges::is_empty)
        {
            None
        } else {
            let id = state.durability.next_publish_commit_id;
            state.durability.next_publish_commit_id = id
                .checked_add(1)
                .ok_or_else(|| io::Error::other("world publication commit ID exhausted"))?;
            Some(id)
        };
        let changes = Arc::new(changes);
        let mut ids: Vec<_> = state.clients.keys().copied().collect();
        ids.sort_unstable();
        for batch in ids.chunks(WIDTH) {
            let shared_parts = Arc::new(SharedParts::default());
            let mut targets = Vec::new();
            let mut jobs: Vec<Job<io::Result<Prepared>>> = Vec::new();
            for &id in batch {
                let Some(client) = state.clients.get(&id) else {
                    continue;
                };
                let capture = Capture::new(id, client);
                let effect = Arc::clone(&effect);
                let changes = Arc::clone(&changes);
                let shared_parts = Arc::clone(&shared_parts);
                targets.push(id);
                jobs.push(Box::new(move || {
                    prepare(
                        capture,
                        &effect,
                        changes.as_ref().as_ref(),
                        commit_id,
                        &shared_parts,
                    )
                }));
            }
            let results = state.publication_workers.run(jobs)?;
            for (id, result) in targets.into_iter().zip(results) {
                match result {
                    Ok(Ok(prepared)) => apply(state, prepared),
                    Ok(Err(error)) => return Err(error), // corrupt revision/catalog invariant
                    Err(()) => disconnect(state, id),
                }
            }
        }
    }
    Ok(())
}

fn prepare(
    capture: Capture,
    effect: &PublishEffects,
    changes: Option<&commit::CommitChanges>,
    commit_id: Option<u64>,
    shared_parts: &SharedParts,
) -> io::Result<Prepared> {
    let mut frames = Vec::new();
    let mut revisions = Vec::new();
    let mut resync = Vec::new();
    if let Some(commit_id) = commit_id {
        if let Some(changes) = changes {
            if let Some(plan) = commit::for_client(changes, &capture, commit_id)? {
                frames.extend(plan.parts.into_iter().map(|part| shared_parts.frame(part)));
                revisions = plan.revisions;
            } else {
                resync = changes.subscribed_keys(&capture);
            }
        } else {
            resync.extend(capture.sent.iter().copied());
        }
    }
    let world_frames = frames.len();
    if effect.client_id == Some(capture.id)
        && effect
            .profile
            .is_none_or(|profile| profile == capture.profile)
    {
        if let Some(action_id) = effect.action_id {
            frames.push(SharedMessage::new(ServerMessage::ActionResult {
                action_id,
                accepted: effect.accepted,
                reason: effect.reason.clone(),
            }));
        }
        if let Some(inventory) = &effect.inventory {
            frames.push(SharedMessage::new(ServerMessage::Inventory {
                revision: inventory.revision,
                slots: inventory.slots.clone(),
            }));
        }
        if !effect.pickups.is_empty() {
            frames.push(SharedMessage::new(ServerMessage::Pickups {
                items: effect.pickups.clone(),
            }));
        }
    }
    // Best-effort presentation only. Send after the committed world changes,
    // only to clients who already have the burned owner subscribed.
    let fire: Vec<_> = effect
        .fire_bursts
        .iter()
        .copied()
        .filter(|&[x, y, z]| {
            capture
                .sent
                .contains(&crate::world::world_to_chunk(x, y, z).0)
        })
        .take(crate::protocol::MAX_FIRE_BURSTS)
        .collect();
    let mut fire =
        (!fire.is_empty()).then(|| SharedMessage::new(ServerMessage::FireBursts { cells: fire }));
    // A complete group plus its actual reliable replies must fit an empty
    // queue. Otherwise replace all its interested chunks with fresh epochs,
    // retaining action/inventory/pickup replies in their original order.
    if frames.len() > OUTBOUND_FRAME_CAPACITY
        || frames.iter().map(|f| f.wire_len() as u64).sum::<u64>() > OUTBOUND_CLIENT_BYTE_CAPACITY
    {
        frames.drain(..world_frames);
        fire = None;
        revisions.clear();
        resync = changes.map_or_else(
            || capture.sent.iter().copied().collect(),
            |changes| changes.subscribed_keys(&capture),
        );
    }
    Ok(Prepared {
        capture,
        frames,
        fire,
        revisions,
        resync,
    })
}

fn apply(state: &mut State, prepared: Prepared) {
    let Prepared {
        capture,
        frames,
        fire,
        revisions,
        resync,
    } = prepared;
    let id = capture.id;
    let Some(client) = state.clients.get_mut(&id) else {
        return;
    };
    if !capture.current(client) {
        // Reliable effect output cannot simply be skipped on invalidation.
        // The live barrier prevents this; a future interleaving must reconnect.
        disconnect(state, id);
        return;
    }
    drop(capture);
    for key in resync {
        if client.sent.remove(&key) {
            let released = state.world.unpin_resident_chunk(key);
            debug_assert!(released);
        }
        client.sent_epochs.remove(&key);
        client.sent_block_versions.remove(&key);
        client.sent_entity_revisions.remove(&key);
    }
    for frame in frames {
        if client.sender.try_send_shared(frame).is_err() {
            disconnect(state, id);
            return;
        }
    }
    for (key, block, entity) in revisions {
        client.sent_block_versions.insert(key, block);
        client.sent_entity_revisions.insert(key, entity);
    }
    // Never disconnect a client or delay reliable replication for optional fire art.
    if let Some(fire) = fire {
        let occupancy = client.sender.snapshot();
        if occupancy.queued_frames < OUTBOUND_FRAME_CAPACITY / 2
            && occupancy.queued_bytes + (fire.wire_len() as u64) < OUTBOUND_CLIENT_BYTE_CAPACITY / 2
        {
            let _ = client.sender.try_send_shared(fire);
        }
    }
}

fn disconnect(state: &mut State, id: u64) {
    if let Some(client) = state.clients.get(&id) {
        let _ = client.socket.shutdown(Shutdown::Both);
    }
    state.remove_client(id);
}

#[cfg(test)]
#[path = "dispatch/tests.rs"]
mod tests;
