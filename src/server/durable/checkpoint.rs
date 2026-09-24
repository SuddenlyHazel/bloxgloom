//! Off-tick snapshot checkpoint submission and receipt handling.

use super::*;
use crate::server::checkpoint::CheckpointSubmitError;
use crate::server::drops::Drops;
use crate::server::{State, durable};
use crate::world::ChunkKey;

pub(in crate::server) fn remember_drops_checkpoint(state: &mut State) -> io::Result<()> {
    let key = durable::drops_checkpoint_key();
    if state
        .durability
        .dirty_checkpoints
        .get(&key)
        .is_some_and(|dirty| state.drops.matches_checkpoint_generation(&dirty.snapshot))
    {
        // Replacing an identical in-flight snapshot with a new checkpoint
        // revision would prevent its receipt from ever clearing dirty state.
        return Ok(());
    }
    let bytes = state.drops.snapshot_bytes()?;
    state.durability.remember_checkpoint(key, bytes);
    Ok(())
}

pub(super) fn process_checkpoint_receipts(state: &mut State, now: Instant) {
    for receipt in state.durability.poll_checkpoints() {
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
                        "bloxgloom:drops_snapshot" => {
                            if state.drops.matches_checkpoint_generation(&snapshot) {
                                state.moving_drops_dirty = false;
                                state.drops_landed_dirty = false;
                                state.last_drop_save = now;
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
    let keys: Vec<_> = state.durability.dirty_checkpoints.keys().cloned().collect();
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
            "bloxgloom:drops_snapshot" => {
                let Some(path) = state.drops.checkpoint_path() else {
                    continue;
                };
                match state
                    .durability
                    .submit_checkpoint(key, move |bytes| Drops::write_snapshot(&path, bytes))
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
            _ => {}
        }
    }
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
