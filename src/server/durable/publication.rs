//! Apply WAL-acknowledged actions and publish their committed state.

use super::*;
use crate::server::fire::FireTransaction;
use crate::server::journal::Change;
use crate::server::runtime::owner_codec::{OWNER_CURSOR_DOMAIN, OWNER_STATE_DOMAIN};
use crate::server::runtime::owner_wake::OWNER_WAKE_DOMAIN;
use crate::server::{State, durable};

/// Owner-wave domains that converge through publication. State cells carry
/// the values, wake flags carry durable "due" markers, and cursors carry
/// the round-robin rotation: dropping any of them here would silently lose
/// receipted work that rode a `CommitAction`.
pub(super) fn is_owner_publication_key(key: &StateKey) -> bool {
    key.domain == OWNER_STATE_DOMAIN
        || key.domain == OWNER_WAKE_DOMAIN
        || key.domain == OWNER_CURSOR_DOMAIN
}

#[path = "publication/commit.rs"]
mod commit;
#[path = "publication/dispatch.rs"]
mod dispatch;

#[cfg(test)]
#[path = "publication/tests.rs"]
mod tests;

pub(super) fn apply_committed_action(
    state: &mut State,
    action: CommitAction,
    entity_permit: Option<super::MirrorPermit>,
) -> io::Result<()> {
    apply_committed_action_inner(state, action, entity_permit, true, Vec::new())
}

/// Keep the burn cue in the same publication as its combined block/entity
/// removal, after the WAL receipt and before optional client presentation.
pub(super) fn apply_committed_fire_action(
    state: &mut State,
    action: CommitAction,
    entity_permit: Option<super::MirrorPermit>,
    transaction: &FireTransaction,
) -> io::Result<()> {
    apply_committed_action_inner(
        state,
        action,
        entity_permit,
        true,
        transaction.burned_cells(),
    )
}

/// An owner-state and world edit already share one WAL record; this applies
/// its block/entity projection at the same receipt without changing the drop-expiry
/// coordinator's independent queued sweep.
pub(super) fn apply_committed_owner_world(
    state: &mut State,
    action: CommitAction,
    entity_permit: Option<super::MirrorPermit>,
) -> io::Result<()> {
    apply_committed_action_inner(state, action, entity_permit, false, Vec::new())
}

fn apply_committed_action_inner(
    state: &mut State,
    mut action: CommitAction,
    entity_permit: Option<super::MirrorPermit>,
    complete_expiry: bool,
    fire_bursts: Vec<[i32; 3]>,
) -> io::Result<()> {
    if !action.terrain_reads.is_current() || !action.terrain_reads.entities_current(&state.entities)
    {
        return Err(io::Error::other(
            "committed terrain dependency changed before apply",
        ));
    }
    if action.entities.is_some() != entity_permit.is_some() {
        return Err(io::Error::other(
            "WAL-committed entity action has no checkpoint mirror reservation",
        ));
    }
    if let Some(entities) = &action.entities {
        state
            .entities
            .validate_committed(entities)
            .map_err(io::Error::other)?;
    }
    if let Some(change) = &action.clock_change {
        state.world_time.validate(change)?;
    }
    // Owner cells, durable wake flags, and rotation cursors piggybacked
    // on this transaction through `add_related_change` ride the same WAL
    // record and the same receipt. Their before-values are rechecked at
    // apply; a mismatch is genuine corruption and stops the coordinator
    // through the caller's fatal path. Wake and cursor changes must reach
    // publication: filtering to the state domain only would silently drop
    // them here.
    let mut owner_changes: Vec<Change> = action
        .entities
        .as_ref()
        .map(|entities| {
            entities
                .changes()
                .iter()
                .filter(|change| is_owner_publication_key(&change.key))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    owner_changes.extend(
        action
            .owner_changes
            .iter()
            .filter(|change| is_owner_publication_key(&change.key))
            .cloned(),
    );
    let world_edits = std::mem::take(&mut action.world_edits);
    let chunk_checkpoints: Vec<_> = world_edits
        .iter()
        .filter(|edit| edit.changed)
        .map(|edit| (edit.key, edit.after_snapshot.clone()))
        .collect();
    state.world.apply_prepared_edits(world_edits)?;
    if let Some(change) = action.clock_change.take() {
        state.world_time.apply(change)?;
        crate::server::world_time::publish(state);
    }
    let mut entity_commit = None;
    if let (Some(entities), Some(permit)) = (action.entities.take(), entity_permit) {
        let mut commit = state
            .entities
            .apply_committed(entities.clone())
            .map_err(io::Error::other)?;
        commit.registry_revision = state.advance_entity_public_revision()?;
        if commit.deltas.iter().any(super::super::drops::is_drop_delta) {
            state.drop_revision = state.drop_revision.wrapping_add(1);
        }
        entity_commit = Some(commit);
        state
            .durability
            .entity_mirror
            .submit_durable(permit, entities)?;
    }
    if !owner_changes.is_empty() {
        state
            .system_runtime
            .apply_replayed_owner_changes(&owner_changes)?;
    }
    if let Some(published) = action.player_publication.take() {
        super::super::players::committed(state, published)?;
    }
    // Delivery at the commit barrier: the producer's transaction is now
    // durable, so its routed wakes become transient tick attempts. They wait
    // in `pending_wakes` for the interaction/commit barrier, which queues
    // them for planning no earlier than the next tick. This extends only
    // in-memory scheduling state; nothing here enters the WAL.
    if !action.entity_wakes.is_empty() {
        let wakes = std::mem::take(&mut action.entity_wakes);
        for id in wakes {
            state.durability.hint_entity_wake(id);
        }
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
    state.pending_block_changes.extend(action.changed_cells);
    let completed_pickup =
        action.action_id.is_none() && action.profile.is_some() && action.inventory.is_some();
    if complete_expiry && action.profile.is_none() {
        state.durability.expire_queued = false;
        // A full expiry batch may leave more expired drops behind; the next
        // throttled scan re-queues the sweep.
        state.durability.expire_again = entity_commit.as_ref().is_some_and(|commit| {
            commit
                .deltas
                .iter()
                .filter(|delta| {
                    matches!(
                        delta,
                        crate::server::entities::EntityDelta::Despawned { .. }
                    ) && super::super::drops::is_drop_delta(delta)
                })
                .count()
                == 256
        });
    }
    if let Some(id) = action.action_id {
        crate::response_trace::event(format_args!("server committed {id}"));
        for delta in &action.deltas {
            crate::response_trace::event(format_args!(
                "server edit {id} chunk={:?} version={}",
                delta.key, delta.version
            ));
        }
    }
    state.notifications.enqueue(
        state.world.catalog(),
        &action.deltas,
        entity_commit.as_ref(),
        action.profile,
        action.inventory.as_ref(),
    );
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
        fire_bursts,
    });
    if completed_pickup && let Some(id) = action.client_id {
        state.durability.retry_pickups.remove(&id);
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
        state
            .pending_block_changes
            .extend(changed_cells.iter().copied());
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
            fire_bursts: changed_cells
                .iter()
                .map(|cell| [cell.x, cell.y, cell.z])
                .collect(),
        });
    }
    Ok(())
}

pub(in crate::server) fn publish_committed(state: &mut State) -> io::Result<()> {
    dispatch::publish(state)
}
