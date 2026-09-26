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
                        _ => {}
                    }
                }
            }
            Err(error) => eprintln!("checkpoint {} failed: {error}", receipt.key.domain),
        }
    }
}

pub(super) fn submit_dirty_checkpoints(state: &mut State) {
    let keys = checkpoint_keys_turn(&mut state.durability, 16);
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

/// Bound traversal and capture, including blocked/retrying keys. Advance past
/// those keys so a full queue or an unavailable file cannot monopolize scans.
/// One wrap per dispatch, no collecting the population before truncation.
fn checkpoint_keys_turn(durability: &mut Durability, quota: usize) -> Vec<StateKey> {
    use std::ops::Bound::{Excluded, Unbounded};
    let mut keys = Vec::with_capacity(quota);
    if let Some(cursor) = &durability.checkpoint_cursor {
        keys.extend(
            durability
                .dirty_checkpoints
                .range((Excluded(cursor), Unbounded))
                .take(quota)
                .map(|(key, _)| key.clone()),
        );
        if keys.len() < quota {
            keys.extend(
                durability
                    .dirty_checkpoints
                    .range(..=cursor.clone())
                    .take(quota - keys.len())
                    .map(|(key, _)| key.clone()),
            );
        }
    } else {
        keys.extend(durability.dirty_checkpoints.keys().take(quota).cloned());
    }
    durability.checkpoint_cursor = keys.last().cloned();
    keys
}

fn submit_fire_checkpoint_batch(state: &mut State) {
    if state.durability.fire_checkpoint_batch.is_some()
        || state.durability.checkpoint_writer.is_full()
    {
        return;
    }
    let now = Instant::now();
    let mut snapshots = Vec::new();
    let mut covered = Vec::new();
    // Fire retains its private complete-map checkpoint protocol. Do not scan
    // unrelated ordinary dirty keys to discover that no fire work exists.
    let start = StateKey::new("bloxgloom:fire_cursor", Vec::new());
    let end = StateKey::new("bloxgloom:fire_pending\0", Vec::new());
    for (key, dirty) in state.durability.dirty_checkpoints.range(start..end) {
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

#[cfg(test)]
#[path = "checkpoint/tests.rs"]
mod tests;
