//! Apply WAL-acknowledged actions and publish their committed state.

use super::*;
use crate::protocol::ServerMessage;
use crate::server::fire::FireTransaction;
use crate::server::{State, durable};

#[path = "publication/commit.rs"]
mod commit;

#[cfg(test)]
#[path = "publication/tests.rs"]
mod tests;

pub(super) fn apply_committed_action(
    state: &mut State,
    mut action: CommitAction,
    entity_permit: Option<super::MirrorPermit>,
) -> io::Result<()> {
    if action.entities.is_some() != entity_permit.is_some() {
        return Err(io::Error::other(
            "WAL-committed entity action has no checkpoint mirror reservation",
        ));
    }
    state.drops.validate_plan(&action.drops)?;
    if let Some(entities) = &action.entities {
        state
            .entities
            .validate_prepared(entities)
            .map_err(io::Error::other)?;
    }
    let world_edits = std::mem::take(&mut action.world_edits);
    let chunk_checkpoints: Vec<_> = world_edits
        .iter()
        .filter(|edit| edit.changed)
        .map(|edit| (edit.key, edit.after_snapshot.clone()))
        .collect();
    state.world.apply_prepared_edits(world_edits)?;
    state.drops.apply_plan(&action.drops)?;
    let mut entity_commit = None;
    if let (Some(entities), Some(permit)) = (action.entities.take(), entity_permit) {
        let mut commit = state
            .entities
            .apply_committed(entities.clone())
            .map_err(io::Error::other)?;
        commit.registry_revision = state.advance_entity_public_revision()?;
        entity_commit = Some(commit);
        state
            .durability
            .entity_mirror
            .submit_durable(permit, entities)?;
    }
    // Delivery at the commit barrier: the producer's transaction is now
    // durable, so its routed wakes become transient tick attempts. They wait
    // in `pending_wakes` for the interaction/commit barrier, which queues
    // them for planning no earlier than the next tick. This extends only
    // in-memory scheduling state; nothing here enters the WAL.
    if !action.entity_wakes.is_empty() {
        let wakes = std::mem::take(&mut action.entity_wakes);
        state.durability.pending_wakes.extend(wakes);
    }
    if let Some(seed) = action.fire_seed.take() {
        for change in seed.changes() {
            state
                .durability
                .remember_checkpoint(change.key.clone(), change.after.clone());
        }
        state.fire.install_seed_synced(seed)?;
    }

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
            InventoryStore::encode_snapshot_with_catalog(inventory, state.world.catalog())?,
        );
    }
    let mut result = None;
    if let Some(transition) = action.receipt_transition.take() {
        let profile = transition.profile;
        state
            .durability
            .receipt_ledgers
            .insert(profile, transition.ledger.clone());
        state
            .durability
            .remember_checkpoint(super::receipts::state_key(profile), transition.after);
        match transition.event {
            super::receipts::ReceiptEvent::Result(record) => result = Some(record),
            super::receipts::ReceiptEvent::EpochGrant => {
                state.durability.pending_grants.remove(&profile);
                state
                    .durability
                    .ready_grants
                    .insert(profile, transition.ledger.epoch);
            }
            super::receipts::ReceiptEvent::Ack => {}
        }
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
    let completed_pickup =
        action.action_id.is_none() && action.profile.is_some() && action.inventory.is_some();
    state.durability.publish_queue.push(PublishEffects {
        client_id: action.client_id,
        profile: action.profile,
        action_id: action.action_id,
        accepted: result.as_ref().is_none_or(|record| record.accepted),
        reason: result.map_or_else(String::new, |record| record.reason),
        inventory: action.inventory,
        deltas: action.deltas,
        entity_commit,
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

/// The caller has already installed every disjoint world owner in this
/// contiguous, WAL-synced receipt group. Keep gameplay and client effects in
/// the original receipt order, after the complete owner-worker barrier.
pub(super) fn publish_committed_fire_after_world(
    state: &mut State,
    transaction: FireTransaction,
) -> io::Result<()> {
    let owner = transaction.owner();
    let changed_cells = transaction.changed_cells.clone();
    let checkpoints: Vec<_> = transaction
        .changes()
        .iter()
        .filter(|change| durable::is_checkpoint_key(&change.key))
        .map(|change| (change.key.clone(), change.after.clone()))
        .collect();
    state.fire.install_synced(transaction)?;
    for (key, value) in checkpoints {
        state.durability.remember_checkpoint(key, value);
    }
    if !changed_cells.is_empty() {
        let chunk = state
            .world
            .cached_chunk(owner)
            .ok_or_else(|| io::Error::other("committed fire chunk vanished before publication"))?;
        let mut deltas = Vec::with_capacity(changed_cells.len());
        for cell in &changed_cells {
            let (key, local) = crate::world::world_to_chunk(cell.x, cell.y, cell.z);
            if key != owner {
                return Err(io::Error::other("fire publish cell crossed owner boundary"));
            }
            let block = chunk
                .block(local)
                .ok_or_else(|| io::Error::other("fire publish cell outside owner chunk"))?;
            deltas.push(BlockDelta {
                key,
                version: chunk.version,
                local: local.map(|coordinate| coordinate as u8),
                block,
            });
        }
        state.pending_block_changes.extend(changed_cells);
        state.durability.publish_queue.push(PublishEffects {
            client_id: None,
            profile: None,
            action_id: None,
            accepted: true,
            reason: String::new(),
            inventory: None,
            deltas,
            entity_commit: None,
            pickups: Vec::new(),
        });
    }
    Ok(())
}

pub(in crate::server) fn publish_committed(state: &mut State) -> io::Result<()> {
    let effects = std::mem::take(&mut state.durability.publish_queue);
    for effect in effects {
        let changes = commit::collect(&effect, state.world.catalog())?;
        let commit_id = if changes.is_empty() {
            None
        } else {
            let id = state.durability.next_publish_commit_id;
            state.durability.next_publish_commit_id = id
                .checked_add(1)
                .ok_or_else(|| io::Error::other("world publication commit ID exhausted"))?;
            Some(id)
        };
        let mut disconnected = Vec::new();
        let mut released_subscriptions = Vec::new();
        for (&id, client) in &mut state.clients {
            let mut healthy = true;
            if let Some(commit_id) = commit_id {
                match commit::for_client(&changes, client, commit_id)? {
                    Some(plan) => {
                        for part in plan.parts {
                            if !client.enqueue(ServerMessage::WorldCommitPart(part)) {
                                healthy = false;
                                break;
                            }
                        }
                        if healthy {
                            for (key, block_revision, entity_revision) in plan.revisions {
                                client.sent_block_versions.insert(key, block_revision);
                                client.sent_entity_revisions.insert(key, entity_revision);
                            }
                        }
                    }
                    None => {
                        // One oversized interested group is not partially visible.
                        // A fresh epoch for each affected chunk is streamed next.
                        for key in changes.subscribed_keys(client) {
                            if client.sent.remove(&key) {
                                released_subscriptions.push(key);
                            }
                            client.sent_epochs.remove(&key);
                            client.sent_block_versions.remove(&key);
                            client.sent_entity_revisions.remove(&key);
                        }
                    }
                }
            }
            if healthy
                && effect.client_id == Some(id)
                && effect
                    .profile
                    .is_none_or(|profile| client.profile == profile)
            {
                if let Some(action_id) = effect.action_id {
                    healthy = client.enqueue(ServerMessage::ActionResult {
                        action_id,
                        accepted: effect.accepted,
                        reason: effect.reason.clone(),
                    });
                }
                if healthy && let Some(inventory) = &effect.inventory {
                    healthy = client.enqueue(ServerMessage::Inventory {
                        revision: inventory.revision,
                        slots: inventory.slots.clone(),
                    });
                }
                if healthy && !effect.pickups.is_empty() {
                    healthy = client.enqueue(ServerMessage::Pickups {
                        items: effect.pickups.clone(),
                    });
                }
            }
            if !healthy {
                disconnected.push(id);
            }
        }
        for key in released_subscriptions {
            let released = state.world.unpin_resident_chunk(key);
            debug_assert!(released, "resnapshot subscription lost its resident chunk");
        }
        for id in disconnected {
            state.remove_client(id);
        }
    }
    Ok(())
}
