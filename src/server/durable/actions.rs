//! Planning for durable gameplay commands and their exact WAL participants.

use super::*;
use crate::server::streaming::request_chunk;
use crate::server::{
    AIR, BEDROCK_Y, EDIT_REACH, State, block_intersects_player, loot, world_to_chunk,
};
use crate::world::{is_plant, is_replaceable, is_solid, supports_plant};

#[cfg(test)]
#[path = "actions/tests.rs"]
mod tests;

pub(in crate::server) fn plan_durable_request(
    state: &mut State,
    request: &DurableRequest,
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
                | ClientMessage::DropStack { action_id, .. } => *action_id,
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
            let receipt_value = encode_action_receipt(message)?;
            if state
                .durability
                .action_receipt(profile, action_id)
                .is_some()
            {
                return Ok(None);
            }
            if state.durability.action_receipt_reserved(profile, action_id)
                || state.durability.profile_reserved(profile)
            {
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
                inventory_before: None,
                inventory: None,
                world_edits: Vec::new(),
                drops: Default::default(),
                deltas: Vec::new(),
                changed_cells: Vec::new(),
                pickups: Vec::new(),
            };
            match message {
                ClientMessage::InventoryMove {
                    from, to, count, ..
                } => {
                    action.inventory_before = Some(InventoryStore::encode_snapshot(&inventory)?);
                    let mut next = inventory;
                    if !next.transfer(*from, *to, *count) {
                        return Ok(None);
                    }
                    action.inventory = Some(next);
                }
                ClientMessage::DropStack { slot, count, .. } => {
                    let Some(stack) = inventory.slots.get(*slot as usize).copied().flatten() else {
                        return Ok(None);
                    };
                    if *count == 0 || *count > stack.count {
                        return Ok(None);
                    }
                    action.inventory_before = Some(InventoryStore::encode_snapshot(&inventory)?);
                    let mut next = inventory;
                    next.slots[*slot as usize] =
                        (*count < stack.count).then_some(crate::inventory::Stack {
                            item: stack.item,
                            count: stack.count - *count,
                        });
                    next.revision = next.revision.wrapping_add(1);
                    action.inventory = Some(next);
                    action.drops = state.drops.plan_spawn(
                        [position[0], position[1] + 0.8, position[2]],
                        stack.item,
                        *count,
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
            let mut taken = Vec::new();
            let mut takes = Vec::new();
            for item in state.drops.pickup_candidates(position) {
                let remaining = updated.insert(item.item, item.count);
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
                inventory_before: Some(InventoryStore::encode_snapshot(&original)?),
                inventory: Some(updated),
                world_edits: Vec::new(),
                drops: state.drops.plan_take(&takes)?,
                deltas: Vec::new(),
                changed_cells: Vec::new(),
                pickups: taken,
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
                inventory_before: None,
                inventory: None,
                world_edits: Vec::new(),
                drops: plan,
                deltas: Vec::new(),
                changed_cells: Vec::new(),
                pickups: Vec::new(),
            }))
        }
    }
}

struct BlockEditCommand {
    id: u64,
    profile: u128,
    action_id: u128,
    receipt_value: Vec<u8>,
    x: i32,
    y: i32,
    z: i32,
    block: u8,
    slot: u8,
}

fn plan_block_edit(state: &mut State, command: BlockEditCommand) -> io::Result<CommitAction> {
    let BlockEditCommand {
        id,
        profile,
        action_id,
        receipt_value,
        x,
        y,
        z,
        block,
        slot,
    } = command;
    let client = state.clients.get(&id).expect("command client exists");
    let position = client.position();
    let inventory_before = client.inventory.clone();
    if !crate::world::valid_block(block) || y <= BEDROCK_Y {
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
    let mut coords = vec![(x, y, z, block)];
    let mut removed_plants = Vec::new();
    if block != AIR {
        if !is_replaceable(previous) {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "target cannot be replaced",
            ));
        }
        if is_plant(block) {
            if y == i32::MIN {
                return Err(io::Error::new(
                    ErrorKind::PermissionDenied,
                    "plant needs soil below",
                ));
            }
            if !supports_plant(cached_block_or_request(
                state,
                x,
                y - 1,
                z,
                "plant support chunk is not resident",
            )?) {
                return Err(io::Error::new(
                    ErrorKind::PermissionDenied,
                    "plant needs soil below",
                ));
            }
        }
        if is_solid(block)
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
            .copied()
            .flatten()
            .filter(|stack| {
                (slot as usize) < crate::inventory::HOTBAR_SLOTS
                    && crate::items::placeable_block(stack.item) == Some(block)
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
        if is_plant(previous) {
            removed_plants.push((previous, [x, y, z]));
        }
        let prepared = state.world.prepare_edits(&coords)?;
        let deltas = prepared_deltas(&coords, &prepared);
        let mut drop_spawns = Vec::new();
        for (plant, at) in removed_plants {
            let version = prepared
                .iter()
                .find(|edit| edit.key == world_to_chunk(at[0], at[1], at[2]).0)
                .map(|edit| edit.new_version)
                .unwrap_or(0);
            push_harvest_spawns(&mut drop_spawns, plant, at, version, state.seed);
        }
        let drops = state.drops.plan_spawns(&drop_spawns)?;
        return Ok(CommitAction {
            client_id: Some(id),
            profile: Some(profile),
            action_id: Some(action_id),
            receipt_value: Some(receipt_value),
            inventory_before: Some(InventoryStore::encode_snapshot(&inventory_before)?),
            inventory: Some(updated),
            world_edits: prepared,
            drops,
            deltas,
            changed_cells: vec![CellCoord::new(x, y, z)],
            pickups: Vec::new(),
        });
    }

    if previous == AIR {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "target is already air",
        ));
    }
    if supports_plant(previous)
        && let Some(above_y) = y.checked_add(1)
    {
        let above =
            cached_block_or_request(state, x, above_y, z, "plant-check chunk is not resident")?;
        if is_plant(above) {
            coords.push((x, above_y, z, AIR));
            removed_plants.push((above, [x, above_y, z]));
        }
    }
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
        push_harvest_spawns(&mut drop_spawns, plant, at, version, state.seed);
    }
    let drops = state.drops.plan_spawns(&drop_spawns)?;
    Ok(CommitAction {
        client_id: Some(id),
        profile: Some(profile),
        action_id: Some(action_id),
        receipt_value: Some(receipt_value),
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
) -> io::Result<u8> {
    if let Some(block) = state.world.cached_block(x, y, z) {
        return Ok(block);
    }
    let key = world_to_chunk(x, y, z).0;
    let _ = request_chunk(state, key)?;
    Err(io::Error::new(ErrorKind::WouldBlock, reason))
}

fn prepared_deltas(coords: &[(i32, i32, i32, u8)], prepared: &[PreparedEdit]) -> Vec<BlockDelta> {
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
    output: &mut Vec<([f32; 3], u8, u16, Duration)>,
    block: u8,
    position: [i32; 3],
    version: u64,
    seed: u64,
) {
    let position = position.map(|coordinate| coordinate as f32 + 0.5);
    output.extend(
        loot::harvest(block, position.map(|n| n.floor() as i32), version, seed)
            .into_iter()
            .flatten()
            .map(|(item, count)| (position, item, count, Duration::from_millis(250))),
    );
}
