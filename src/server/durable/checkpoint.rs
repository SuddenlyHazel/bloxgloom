//! Off-tick snapshot checkpoint submission and receipt handling.

use super::*;
use crate::server::checkpoint::CheckpointSubmitError;
use crate::server::journal::StateKey;
use crate::server::{State, durable};
use crate::world::ChunkKey;
use std::time::Duration;

fn fire_batch_key() -> StateKey {
    StateKey::new("bloxgloom:fire_checkpoint_batch", Vec::new())
}

/// Submits dirty drop shards and the allocator checkpoint through the
/// shared checkpoint workers. Only chunks whose drops changed since the
/// last submission are serialized, so one moving drop costs its chunk
/// owners instead of every drop. Returns true once every dirty shard is
/// submitted (or already tracked with identical bytes); a false return
/// re-queues the refused snapshots for the next attempt instead of
/// dropping work.
pub(in crate::server) fn remember_drops_checkpoint(state: &mut State) -> io::Result<bool> {
    let mut complete = true;
    let mut deferred = Vec::new();
    for (chunk, bytes) in state.drops.take_dirty_shard_snapshots() {
        let key = durable::drops_chunk_key(chunk);
        if !remember_drop_snapshot(&mut state.durability, key, bytes) {
            deferred.push(chunk);
            complete = false;
        }
    }
    let mut allocator_deferred = false;
    if let Some(bytes) = state.drops.take_allocator_snapshot()
        && !remember_drop_snapshot(&mut state.durability, durable::drops_allocator_key(), bytes)
    {
        allocator_deferred = true;
        complete = false;
    }
    if !deferred.is_empty() || allocator_deferred {
        state
            .drops
            .restore_shard_dirtiness(deferred, allocator_deferred);
    }
    Ok(complete)
}

/// Tracks one shard snapshot without resubmitting an identical in-flight
/// copy: replacing it with a new revision would prevent its receipt from
/// ever clearing dirty state. Refuses (fail closed) past the shared
/// dirty-checkpoint bounds instead of growing without limit.
fn remember_drop_snapshot(
    durability: &mut Durability,
    key: StateKey,
    bytes: Vec<u8>,
) -> bool {
    if let Some(dirty) = durability.dirty_checkpoints.get(&key) {
        if dirty.snapshot == bytes {
            return true;
        }
    }
    // Overwriting this key frees its current bytes before the bound check.
    let held: usize = durability
        .dirty_checkpoints
        .iter()
        .filter(|(held_key, _)| *held_key != &key)
        .map(|(_, dirty)| dirty.snapshot.len())
        .sum::<usize>()
        .saturating_add(bytes.len());
    if (!durability.dirty_checkpoints.contains_key(&key)
        && durability.dirty_checkpoints.len() >= MAX_DIRTY_CHECKPOINT_KEYS)
        || held > MAX_DIRTY_CHECKPOINT_BYTES
    {
        return false;
    }
    durability.remember_checkpoint(key, bytes);
    true
}

fn drops_checkpoints_pending(state: &State) -> bool {
    let pending = |key: &StateKey| {
        matches!(
            key.domain.as_str(),
            "bloxgloom:drops_chunk" | "bloxgloom:drops_allocator"
        )
    };
    state.durability.dirty_checkpoints.keys().any(pending)
        || state.durability.checkpoint_inflight.keys().any(pending)
}

pub(super) fn process_checkpoint_receipts(state: &mut State, now: Instant) {
    for receipt in state.durability.poll_checkpoints() {
        if receipt.key == fire_batch_key() {
            let Some(batch) = state.durability.fire_checkpoint_batch.take() else {
                state.durability.failed = true;
                continue;
            };
            if batch.revision != receipt.revision {
                state.durability.failed = true;
                continue;
            }
            match receipt.result {
                Ok(()) => {
                    for (key, revision) in batch.covered {
                        state.durability.checkpoint_committed(&key, revision);
                    }
                }
                Err(error) => {
                    for (key, revision) in batch.covered {
                        if let Some(dirty) = state.durability.dirty_checkpoints.get_mut(&key)
                            && dirty.revision == revision
                        {
                            dirty.retry_after = now + Duration::from_secs(1);
                        }
                    }
                    eprintln!("checkpoint {} failed: {error}", receipt.key.domain);
                }
            }
            continue;
        }
        match receipt.result {
            Ok(()) => {
                if let Some(snapshot) = state
                    .durability
                    .checkpoint_committed(&receipt.key, receipt.revision)
                {
                    match receipt.key.domain.as_str() {
                        "bloxgloom:chunk_snapshot" => {
                            if let Some(key) = decode_chunk_checkpoint_key(&receipt.key.bytes) {
                                state.world.clear_checkpointed_snapshot(key, &snapshot);
                            }
                        }
                        "bloxgloom:inventory" => {
                            if let Some(profile) =
                                decode_inventory_checkpoint_key(&receipt.key.bytes)
                                && state
                                    .durability
                                    .inventory_overlay
                                    .get(&profile)
                                    .and_then(|inventory| {
                                        InventoryStore::encode_snapshot_with_catalog(
                                            inventory,
                                            state.world.catalog(),
                                        )
                                        .ok()
                                    })
                                    .as_deref()
                                    == Some(snapshot.as_slice())
                            {
                                state.durability.inventory_overlay.remove(&profile);
                            }
                        }
                        "bloxgloom:drops_chunk" | "bloxgloom:drops_allocator" => {
                            // The committed key already left dirty state
                            // above when its revision matched; the flags
                            // clear below once no drops key remains.
                        }
                        _ => {}
                    }
                }
            }
            Err(error) => eprintln!("checkpoint {} failed: {error}", receipt.key.domain),
        }
    }
    // Coordinator drop flags clear only when every submitted drops key has
    // drained AND the live store holds no unsubmitted dirtiness. Either
    // side alone would drop work: unsubmitted motion must still checkpoint,
    // and in-flight shards must still be receipted.
    if !state.drops.has_uncheckpointed_shards() && !drops_checkpoints_pending(state) {
        state.moving_drops_dirty = false;
        state.drops_landed_dirty = false;
        state.last_drop_save = now;
    }
}

pub(super) fn submit_dirty_checkpoints(state: &mut State) {
    let keys: Vec<_> = state.durability.dirty_checkpoints.keys().cloned().collect();
    submit_fire_checkpoint_batch(state);
    for key in keys {
        match key.domain.as_str() {
            "bloxgloom:chunk_snapshot" => {
                let Some(chunk) = decode_chunk_checkpoint_key(&key.bytes) else {
                    continue;
                };
                let storage = state.world.storage_handle();
                match state.durability.submit_checkpoint(key, move |bytes| {
                    storage.replace_snapshot(chunk, (!bytes.is_empty()).then_some(bytes))
                }) {
                    Ok(_) | Err(CheckpointSubmitError::Full) => {}
                    Err(CheckpointSubmitError::Closed) => state.durability.failed = true,
                }
            }
            "bloxgloom:inventory" => {
                let Some(profile) = decode_inventory_checkpoint_key(&key.bytes) else {
                    continue;
                };
                let store = state.inventory_store.clone();
                match state
                    .durability
                    .submit_checkpoint(key, move |bytes| store.checkpoint_snapshot(profile, bytes))
                {
                    Ok(_) | Err(CheckpointSubmitError::Full) => {}
                    Err(CheckpointSubmitError::Closed) => state.durability.failed = true,
                }
            }
            "bloxgloom:drops_chunk" => {
                let Some(chunk) = decode_drops_chunk_key(&key.bytes) else {
                    continue;
                };
                let Some(path) = state.drops.shard_snapshot_path(chunk) else {
                    continue;
                };
                match state.durability.submit_checkpoint(key, move |bytes| {
                    crate::server::drops::write_shard_snapshot(&path, bytes)
                }) {
                    Ok(_) | Err(CheckpointSubmitError::Full) => {}
                    Err(CheckpointSubmitError::Closed) => state.durability.failed = true,
                }
            }
            "bloxgloom:drops_allocator" => {
                let Some(path) = state.drops.allocator_snapshot_path() else {
                    continue;
                };
                match state.durability.submit_checkpoint(key, move |bytes| {
                    crate::server::drops::write_allocator_snapshot(&path, bytes)
                }) {
                    Ok(_) | Err(CheckpointSubmitError::Full) => {}
                    Err(CheckpointSubmitError::Closed) => state.durability.failed = true,
                }
            }
            "bloxgloom:action_ledger" => {
                let Some(profile) = decode_inventory_checkpoint_key(&key.bytes) else {
                    continue;
                };
                let store = state.durability.receipt_store.clone();
                match state
                    .durability
                    .submit_checkpoint(key, move |bytes| store.write(profile, bytes))
                {
                    Ok(_) | Err(CheckpointSubmitError::Full) => {}
                    Err(CheckpointSubmitError::Closed) => state.durability.failed = true,
                }
            }
            "bloxgloom:fire_frontier" | "bloxgloom:fire_pending" | "bloxgloom:fire_cursor" => {
                // All fire keys share one complete-map aggregate and one
                // worker write. The grouped receipt clears exact revisions.
            }
            _ => {}
        }
    }
}

fn submit_fire_checkpoint_batch(state: &mut State) {
    if state.durability.fire_checkpoint_batch.is_some() {
        return;
    }
    let now = Instant::now();
    let mut snapshots = Vec::new();
    let mut covered = Vec::new();
    for (key, dirty) in &state.durability.dirty_checkpoints {
        if !is_fire_checkpoint_key(key) || dirty.retry_after > now {
            continue;
        }
        snapshots.push((key.clone(), dirty.snapshot.clone()));
        covered.push((key.clone(), dirty.revision));
    }
    if snapshots.is_empty() {
        return;
    }

    let revision = state.durability.next_checkpoint_revision;
    let Some(next_revision) = revision.checked_add(1) else {
        state.durability.failed = true;
        return;
    };
    let store = state.durability.fire_store.clone();
    let result = state.durability.checkpoint_writer.try_submit(
        fire_batch_key(),
        revision,
        Vec::new(),
        move |_| store.write_batch(&snapshots),
    );
    match result {
        Ok(()) => {
            state.durability.next_checkpoint_revision = next_revision;
            state
                .durability
                .checkpoint_inflight
                .insert(fire_batch_key(), revision);
            state.durability.fire_checkpoint_batch =
                Some(durable::FireCheckpointBatch { revision, covered });
        }
        Err(CheckpointSubmitError::Full) => {}
        Err(CheckpointSubmitError::Closed) => state.durability.failed = true,
    }
}

fn is_fire_checkpoint_key(key: &StateKey) -> bool {
    matches!(
        key.domain.as_str(),
        "bloxgloom:fire_frontier" | "bloxgloom:fire_pending" | "bloxgloom:fire_cursor"
    )
}

fn decode_drops_chunk_key(bytes: &[u8]) -> Option<ChunkKey> {
    (bytes.len() == 12).then(|| ChunkKey {
        x: i32::from_le_bytes(bytes[0..4].try_into().unwrap()),
        y: i32::from_le_bytes(bytes[4..8].try_into().unwrap()),
        z: i32::from_le_bytes(bytes[8..12].try_into().unwrap()),
    })
}

fn decode_chunk_checkpoint_key(bytes: &[u8]) -> Option<ChunkKey> {
    (bytes.len() == 12).then(|| ChunkKey {
        x: i32::from_le_bytes(bytes[0..4].try_into().unwrap()),
        y: i32::from_le_bytes(bytes[4..8].try_into().unwrap()),
        z: i32::from_le_bytes(bytes[8..12].try_into().unwrap()),
    })
}

fn decode_inventory_checkpoint_key(bytes: &[u8]) -> Option<u128> {
    (bytes.len() == 16).then(|| u128::from_le_bytes(bytes.try_into().unwrap()))
}
