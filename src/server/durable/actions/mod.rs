//! Planning for durable gameplay commands and their exact WAL participants.

use super::*;
use crate::items::ItemId;
use crate::server::streaming::request_chunk;
use crate::server::{
    AIR, BEDROCK_Y, EDIT_REACH, State, block_intersects_player, loot, world_to_chunk,
};
use crate::world::BlockId;

pub(in crate::server) mod kiln;
#[cfg(test)]
#[path = "tests.rs"]
mod tests;

pub(in crate::server) fn plan_durable_request(
    state: &mut State,
    request: &DurableRequest,
    tick: TickId,
) -> io::Result<Option<CommitAction>> {
    match request {
        DurableRequest::Command { id, message, .. } => {
            let Some(client) = state.clients.get(id) else {
                return Ok(None);
            };
            let profile = client.profile;
            let action_id = match message {
                ClientMessage::Edit { action_id, .. }
                | ClientMessage::InventoryMove { action_id, .. }
                | ClientMessage::DropStack { action_id, .. }
                | ClientMessage::EntityInteract { action_id, .. } => *action_id,
                _ => {
                    return Err(io::Error::new(
                        ErrorKind::InvalidInput,
                        "not a durable action",
                    ));
                }
            };
            if action_id == 0 {
                return Err(io::Error::new(
                    ErrorKind::InvalidInput,
                    "action ID must be nonzero",
                ));
            }
            let receipt_value =
                super::state::encode_action_receipt_with_catalog(message, state.world.catalog())?;
            if state.durability.profile_reserved(profile) {
                return Err(io::Error::new(
                    ErrorKind::WouldBlock,
                    "profile has a pending durable action",
                ));
            }
            let (inventory, position) = {
                let client = state.clients.get(id).expect("client was checked above");
                (client.inventory.clone(), client.position())
            };
            let mut action = CommitAction {
                client_id: Some(*id),
                profile: Some(profile),
                action_id: Some(action_id),
                receipt_value: Some(receipt_value.clone()),
                receipt_transition: None,
                inventory_before: None,
                inventory: None,
                world_edits: Vec::new(),
                drops: Default::default(),
                deltas: Vec::new(),
                changed_cells: Vec::new(),
                pickups: Vec::new(),
                fire_seed: None,
                entities: None,
            };
            match message {
                ClientMessage::InventoryMove {
                    from, to, count, ..
                } => {
                    action.inventory_before = Some(InventoryStore::encode_snapshot_with_catalog(
                        &inventory,
                        state.world.catalog(),
                    )?);
                    let mut next = inventory;
                    if !next.transfer(*from, *to, *count) {
                        return Ok(None);
                    }
                    action.inventory = Some(next);
                }
                ClientMessage::DropStack { slot, count, .. } => {
                    let Some(stack) = inventory.slots.get(*slot as usize).cloned().flatten() else {
                        return Ok(None);
                    };
                    if *count == 0 || *count > stack.count {
                        return Ok(None);
                    }
                    action.inventory_before = Some(InventoryStore::encode_snapshot_with_catalog(
                        &inventory,
                        state.world.catalog(),
                    )?);
                    let mut next = inventory;
                    next.slots[*slot as usize] = (*count < stack.count).then(|| {
                        let mut remainder = stack.clone();
                        remainder.count -= *count;
                        remainder
                    });
                    next.revision = next.revision.wrapping_add(1);
                    action.inventory = Some(next);
                    let mut dropped = stack;
                    dropped.count = *count;
                    action.drops = state.drops.plan_spawn_stack(
                        [position[0], position[1] + 0.8, position[2]],
                        dropped,
                        Duration::from_millis(1_500),
                    )?;
                }
                ClientMessage::Edit {
                    x,
                    y,
                    z,
                    block,
                    slot,
                    ..
                } => {
                    return plan_block_edit(
                        state,
                        tick,
                        BlockEditCommand {
                            id: *id,
                            profile,
                            action_id,
                            receipt_value,
                            x: *x,
                            y: *y,
                            z: *z,
                            block: *block,
                            slot: *slot,
                        },
                    )
                    .map(Some);
                }
                ClientMessage::EntityInteract {
                    action_id,
                    target,
                    payload,
                } => {
                    return kiln::plan_interact(
                        state,
                        *id,
                        profile,
                        *action_id,
                        *target,
                        payload,
                        receipt_value,
                    )
                    .map(Some);
                }
                _ => unreachable!(),
            }
            Ok(Some(action))
        }
        DurableRequest::Pickup { id } => {
            let Some(client) = state.clients.get(id) else {
                return Ok(None);
            };
            if state.durability.profile_reserved(client.profile) {
                return Err(io::Error::new(
                    ErrorKind::WouldBlock,
                    "profile has a pending durable action",
                ));
            }
            let profile = client.profile;
            let original = client.inventory.clone();
            let position = client.position();
            let mut updated = original.clone();
            let catalog = state.world.catalog_arc();
            let mut taken = Vec::new();
            let mut takes = Vec::new();
            for item in state.drops.pickup_candidates(position) {
                let mut stack = state.drops.stack(item.id).ok_or_else(|| {
                    io::Error::new(ErrorKind::InvalidData, "pickup candidate disappeared")
                })?;
                stack.count = item.count;
                let remaining = updated.insert_stack(&stack, &catalog);
                if remaining != item.count {
                    let amount = item.count - remaining;
                    takes.push((item.id, amount));
                    taken.push(DroppedItem {
                        count: amount,
                        ..item
                    });
                }
            }
            if taken.is_empty() {
                return Ok(None);
            }
            Ok(Some(CommitAction {
                client_id: Some(*id),
                profile: Some(profile),
                action_id: None,
                receipt_value: None,
                receipt_transition: None,
                inventory_before: Some(InventoryStore::encode_snapshot_with_catalog(
                    &original,
                    state.world.catalog(),
                )?),
                inventory: Some(updated),
                world_edits: Vec::new(),
                drops: state.drops.plan_take(&takes)?,
                deltas: Vec::new(),
                changed_cells: Vec::new(),
                pickups: taken,
                fire_seed: None,
                entities: None,
            }))
        }
        DurableRequest::Expire => {
            let plan = state.drops.plan_expired(256);
            if plan.changes.is_empty() {
                state.durability.expire_queued = false;
                state.durability.expire_again = false;
                return Ok(None);
            }
            Ok(Some(CommitAction {
                client_id: None,
                profile: None,
                action_id: None,
                receipt_value: None,
                receipt_transition: None,
                inventory_before: None,
                inventory: None,
                world_edits: Vec::new(),
                drops: plan,
                deltas: Vec::new(),
                changed_cells: Vec::new(),
                pickups: Vec::new(),
                fire_seed: None,
                entities: None,
            }))
        }
        DurableRequest::EntityTick { id } => kiln::plan_entity_tick(state, *id, tick.get()),
    }
}

pub(in crate::server) struct BlockEditCommand {
    pub(in crate::server) id: u64,
    pub(in crate::server) profile: u128,
    pub(in crate::server) action_id: u128,
    pub(in crate::server) receipt_value: Vec<u8>,
    pub(in crate::server) x: i32,
    pub(in crate::server) y: i32,
    pub(in crate::server) z: i32,
    pub(in crate::server) block: BlockId,
    pub(in crate::server) slot: u8,
}

fn plan_block_edit(
    state: &mut State,
    tick: TickId,
    command: BlockEditCommand,
) -> io::Result<CommitAction> {
    let (id, x, y, z, block) = (command.id, command.x, command.y, command.z, command.block);
    let client = state.clients.get(&id).expect("command client exists");
    let position = client.position();
    let inventory_before = client.inventory.clone();
    let catalog = state.world.catalog_arc();
    let has = |state_id: BlockId, flag: u8| catalog.block_flags(state_id) & flag != 0;
    if catalog.state(block).is_none() || y <= BEDROCK_Y {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "invalid block edit",
        ));
    }
    let distance_sq = (x as f32 + 0.5 - position[0]).powi(2)
        + (y as f32 + 0.5 - (position[1] + 1.6)).powi(2)
        + (z as f32 + 0.5 - position[2]).powi(2);
    if distance_sq > EDIT_REACH * EDIT_REACH || !client.interested(world_to_chunk(x, y, z).0) {
        return Err(io::Error::new(
            ErrorKind::PermissionDenied,
            "block out of reach",
        ));
    }
    let previous = cached_block_or_request(state, x, y, z, "edit target chunk is not resident")?;
    if previous == block {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "block is unchanged",
        ));
    }
    let hooks = state
        .block_actions
        .for_state(&catalog, if block == AIR { previous } else { block });
    if let Some(hooks) = hooks {
        return if block == AIR {
            (hooks.break_block)(state, tick, command, previous)
        } else {
            (hooks.place)(state, tick, command, previous)
        };
    }
    let BlockEditCommand {
        profile,
        action_id,
        receipt_value,
        slot,
        ..
    } = command;
    let mut coords = vec![(x, y, z, block)];
    let mut removed_plants = Vec::new();
    if block != AIR {
        if !has(previous, crate::content::REPLACEABLE) {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "target cannot be replaced",
            ));
        }
        if has(block, crate::content::PLANT) {
            if y == i32::MIN {
                return Err(io::Error::new(
                    ErrorKind::PermissionDenied,
                    "plant needs soil below",
                ));
            }
            if !has(
                cached_block_or_request(state, x, y - 1, z, "plant support chunk is not resident")?,
                crate::content::SUPPORTS_PLANT,
            ) {
                return Err(io::Error::new(
                    ErrorKind::PermissionDenied,
                    "plant needs soil below",
                ));
            }
        }
        if has(block, crate::content::SOLID)
            && state
                .clients
                .values()
                .any(|other| block_intersects_player([x, y, z], other.position()))
        {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "block overlaps a player",
            ));
        }
        let selected = inventory_before
            .slots
            .get(slot as usize)
            .cloned()
            .flatten()
            .filter(|stack| {
                (slot as usize) < crate::inventory::HOTBAR_SLOTS
                    && crate::items::placeable_block_in(stack.item, &catalog) == Some(block)
            })
            .ok_or_else(|| {
                io::Error::new(ErrorKind::PermissionDenied, "selected stack mismatch")
            })?;
        let mut updated = inventory_before.clone();
        if !updated.consume(slot, selected.item) {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "selected stack empty",
            ));
        }
        if has(previous, crate::content::PLANT) {
            removed_plants.push((previous, [x, y, z]));
        }
        ensure_no_unhandled_anchor(state, &coords)?;
        let prepared = state.world.prepare_edits(&coords)?;
        let deltas = prepared_deltas(&coords, &prepared);
        let mut drop_spawns = Vec::new();
        for (plant, at) in removed_plants {
            let version = prepared
                .iter()
                .find(|edit| edit.key == world_to_chunk(at[0], at[1], at[2]).0)
                .map(|edit| edit.new_version)
                .unwrap_or(0);
            push_harvest_spawns(&mut drop_spawns, &catalog, plant, at, version, state.seed);
        }
        let drops = state.drops.plan_spawns(&drop_spawns)?;
        let (source, local) = world_to_chunk(x, y, z);
        let cell = crate::world::Chunk::index(local)
            .and_then(|index| u16::try_from(index).ok())
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "fire seed cell invalid"))?;
        let fire_seed = state
            .fire
            .prepare_seed_from_edit(tick, source, cell, block)?;
        return Ok(CommitAction {
            client_id: Some(id),
            profile: Some(profile),
            action_id: Some(action_id),
            receipt_value: Some(receipt_value),
            receipt_transition: None,
            inventory_before: Some(InventoryStore::encode_snapshot_with_catalog(
                &inventory_before,
                &catalog,
            )?),
            inventory: Some(updated),
            world_edits: prepared,
            drops,
            deltas,
            changed_cells: vec![CellCoord::new(x, y, z)],
            pickups: Vec::new(),
            fire_seed,
            entities: None,
        });
    }

    if previous == AIR {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "target is already air",
        ));
    }
    if has(previous, crate::content::SUPPORTS_PLANT)
        && let Some(above_y) = y.checked_add(1)
    {
        let above =
            cached_block_or_request(state, x, above_y, z, "plant-check chunk is not resident")?;
        if has(above, crate::content::PLANT) {
            coords.push((x, above_y, z, AIR));
            removed_plants.push((above, [x, above_y, z]));
        }
    }
    ensure_no_unhandled_anchor(state, &coords)?;
    let prepared = state.world.prepare_edits(&coords)?;
    let deltas = prepared_deltas(&coords, &prepared);
    let mut drop_spawns = Vec::new();
    let base_version = prepared
        .iter()
        .find(|edit| edit.key == world_to_chunk(x, y, z).0)
        .map(|edit| edit.new_version)
        .unwrap_or(0);
    push_harvest_spawns(
        &mut drop_spawns,
        &catalog,
        previous,
        [x, y, z],
        base_version,
        state.seed,
    );
    for (plant, at) in removed_plants {
        let version = prepared
            .iter()
            .find(|edit| edit.key == world_to_chunk(at[0], at[1], at[2]).0)
            .map(|edit| edit.new_version)
            .unwrap_or(0);
        push_harvest_spawns(&mut drop_spawns, &catalog, plant, at, version, state.seed);
    }
    let drops = state.drops.plan_spawns(&drop_spawns)?;
    Ok(CommitAction {
        client_id: Some(id),
        profile: Some(profile),
        action_id: Some(action_id),
        receipt_value: Some(receipt_value),
        receipt_transition: None,
        inventory_before: None,
        inventory: None,
        world_edits: prepared,
        drops,
        deltas,
        changed_cells: coords
            .into_iter()
            .map(|(x, y, z, _)| CellCoord::new(x, y, z))
            .collect(),
        pickups: Vec::new(),
        fire_seed: None,
        entities: None,
    })
}

/// A failed resident read is not authoritative air. Ask the bounded chunk
/// loader for that exact key before deferring the original command for retry.
/// This matters at vertical interest boundaries, where an edit can depend on
/// a support/plant-check chunk that normal view streaming never requests.
fn cached_block_or_request(
    state: &mut State,
    x: i32,
    y: i32,
    z: i32,
    reason: &'static str,
) -> io::Result<BlockId> {
    if let Some(block) = state.world.cached_block(x, y, z) {
        return Ok(block);
    }
    let key = world_to_chunk(x, y, z).0;
    let _ = request_chunk(state, key)?;
    Err(io::Error::new(ErrorKind::WouldBlock, reason))
}

fn ensure_no_unhandled_anchor(
    state: &State,
    coords: &[(i32, i32, i32, BlockId)],
) -> io::Result<()> {
    if coords.iter().any(|&(x, y, z, _)| {
        state
            .entities
            .anchored_at(crate::server::entities::CellCoord::new(x, y, z))
            .is_some()
    }) {
        Err(io::Error::new(
            ErrorKind::InvalidData,
            "generic block edit would orphan an anchored entity",
        ))
    } else {
        Ok(())
    }
}

fn prepared_deltas(
    coords: &[(i32, i32, i32, BlockId)],
    prepared: &[PreparedEdit],
) -> Vec<BlockDelta> {
    coords
        .iter()
        .filter_map(|&(x, y, z, block)| {
            let (key, local) = world_to_chunk(x, y, z);
            let version = prepared.iter().find(|edit| edit.key == key)?.new_version;
            Some(BlockDelta {
                key,
                version,
                local: local.map(|value| value as u8),
                block,
            })
        })
        .collect()
}

fn push_harvest_spawns(
    output: &mut Vec<([f32; 3], ItemId, u16, Duration)>,
    catalog: &crate::content::Catalog,
    block: BlockId,
    position: [i32; 3],
    version: u64,
    seed: u64,
) {
    let position = position.map(|coordinate| coordinate as f32 + 0.5);
    output.extend(
        loot::harvest_with_catalog(
            catalog,
            block,
            position.map(|n| n.floor() as i32),
            version,
            seed,
        )
        .into_iter()
        .flatten()
        .map(|(item, count)| (position, item, count, Duration::from_millis(250))),
    );
}
