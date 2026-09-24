//! Apply WAL-acknowledged actions and publish their committed state.

use super::*;
use crate::protocol::ServerMessage;
use crate::server::{State, durable};
use crate::world::ChunkKey;

#[cfg(test)]
#[path = "publication/tests.rs"]
mod tests;

pub(super) fn apply_committed_action(
    state: &mut State,
    mut action: CommitAction,
) -> io::Result<()> {
    state.drops.validate_plan(&action.drops)?;
    let world_edits = std::mem::take(&mut action.world_edits);
    let chunk_checkpoints: Vec<_> = world_edits
        .iter()
        .filter(|edit| edit.changed)
        .map(|edit| (edit.key, edit.after_snapshot.clone()))
        .collect();
    state.world.apply_prepared_edits(world_edits)?;
    state.drops.apply_plan(&action.drops)?;

    let mut cells_per_chunk = HashMap::<ChunkKey, usize>::new();
    for delta in &action.deltas {
        *cells_per_chunk.entry(delta.key).or_default() += 1;
    }
    let mut full_keys: Vec<_> = cells_per_chunk
        .into_iter()
        .filter_map(|(key, count)| (count > 1).then_some(key))
        .collect();
    full_keys.sort_unstable_by_key(|key| (key.x, key.y, key.z));
    let full_key_set: HashSet<_> = full_keys.iter().copied().collect();
    let mut full_chunks = Vec::with_capacity(full_keys.len());
    for key in &full_keys {
        let chunk = state
            .world
            .cached_chunk(*key)
            .ok_or_else(|| io::Error::other("committed edit chunk vanished before publication"))?;
        full_chunks.push(chunk);
    }
    action
        .deltas
        .retain(|delta| !full_key_set.contains(&delta.key));

    if let (Some(profile), Some(inventory)) = (action.profile, &action.inventory) {
        state
            .durability
            .inventory_revisions
            .insert(profile, inventory.revision);
        state
            .durability
            .inventory_overlay
            .insert(profile, inventory.clone());
        if let Some(client) = action
            .client_id
            .and_then(|id| state.clients.get_mut(&id))
            .filter(|client| client.profile == profile)
        {
            client.inventory = inventory.clone();
        }
        state.durability.remember_checkpoint(
            durable::inventory_state_key(profile),
            InventoryStore::encode_snapshot(inventory)?,
        );
    }
    if let (Some(profile), Some(action_id), Some(value)) = (
        action.profile,
        action.action_id,
        action.receipt_value.clone(),
    ) {
        state
            .durability
            .action_receipts
            .insert((profile, action_id), value);
    }
    for (key, snapshot) in chunk_checkpoints {
        state
            .durability
            .remember_checkpoint(durable::chunk_state_key(key), snapshot);
    }
    if !action.drops.changes.is_empty() || action.drops.allocator.is_some() {
        state.moving_drops_dirty = true;
    }
    state.pending_block_changes.extend(action.changed_cells);
    let completed_pickup = action.action_id.is_none() && action.profile.is_some();
    state.durability.publish_queue.push(PublishEffects {
        client_id: action.client_id,
        profile: action.profile,
        action_id: action.action_id,
        accepted: true,
        reason: String::new(),
        inventory: action.inventory,
        chunks: full_chunks,
        deltas: action.deltas,
        pickups: action.pickups,
    });
    if completed_pickup && let Some(id) = action.client_id {
        state.durability.retry_pickups.remove(&id);
    }
    if action.profile.is_none() {
        state.durability.expire_queued = false;
        state.durability.expire_again = action.drops.changes.len() == 256;
    }
    Ok(())
}

pub(in crate::server) fn publish_committed(state: &mut State) {
    for effect in state.durability.publish_queue.drain(..) {
        if let Some(id) = effect.client_id
            && let Some(client) = state.clients.get(&id)
            && effect
                .profile
                .is_none_or(|profile| client.profile == profile)
        {
            if let Some(action_id) = effect.action_id {
                client.enqueue(ServerMessage::ActionResult {
                    action_id,
                    accepted: effect.accepted,
                    reason: effect.reason,
                });
            }
            if let Some(inventory) = effect.inventory {
                client.enqueue(ServerMessage::Inventory {
                    revision: inventory.revision,
                    slots: inventory.slots,
                });
            }
            if !effect.pickups.is_empty() {
                client.enqueue(ServerMessage::Pickups {
                    items: effect.pickups,
                });
            }
        }
        for chunk in effect.chunks {
            let key = chunk.key;
            for client in state.clients.values() {
                if client.sent.contains(&key) {
                    client.enqueue(ServerMessage::Chunk(chunk.clone()));
                }
            }
        }
        for delta in effect.deltas {
            for client in state.clients.values() {
                if client.sent.contains(&delta.key) {
                    client.enqueue(ServerMessage::Delta {
                        key: delta.key,
                        version: delta.version,
                        x: delta.local[0],
                        y: delta.local[1],
                        z: delta.local[2],
                        block: delta.block,
                    });
                }
            }
        }
    }
}
