//! Trusted two-cell kiln edit planning. No world, item, or entity mutation is
//! visible here; all participants enter one WAL record through `CommitAction`.

use super::{BlockEditCommand, prepared_deltas, push_harvest_spawns};
use crate::content::{KILN_ITEM, PLANT, REPLACEABLE, SOLID};
use crate::inventory::{HOTBAR_SLOTS, InventoryStore, Stack};
use crate::server::durable::{BlockDelta, CommitAction};
use crate::server::effects::CellCoord as EffectCell;
use crate::server::entities::{
    CellCoord, KilnFacing, KilnPayload, kiln_block_states, kiln_footprint, kiln_payload,
    plan_break as plan_kiln_break,
};
use crate::server::simulation::TickId;
use crate::server::{AIR, State, block_intersects_player};
use crate::world::BlockId;
use std::io::{self, ErrorKind};
use std::time::Duration;

pub(in crate::server) fn plan_place(
    state: &mut State,
    tick: TickId,
    command: BlockEditCommand,
    previous: BlockId,
) -> io::Result<CommitAction> {
    let catalog = state.world.catalog_arc();
    let facing = KilnFacing::from_place_state(&catalog, command.block)
        .map_err(|error| io::Error::new(ErrorKind::InvalidInput, error))?;
    let inventory_before = {
        let client = state.clients.get(&command.id).ok_or_else(|| {
            io::Error::new(
                ErrorKind::NotConnected,
                "kiln placement client disconnected",
            )
        })?;
        client.inventory.clone()
    };
    let selected = inventory_before
        .slots
        .get(command.slot as usize)
        .and_then(Option::as_ref)
        .filter(|stack| {
            usize::from(command.slot) < HOTBAR_SLOTS
                && stack.item == KILN_ITEM
                && stack.components.is_none()
        })
        .ok_or_else(|| {
            io::Error::new(ErrorKind::PermissionDenied, "selected kiln item mismatch")
        })?;
    if selected.count == 0 {
        return Err(io::Error::new(
            ErrorKind::PermissionDenied,
            "selected kiln stack empty",
        ));
    }
    let anchor = CellCoord::new(command.x, command.y, command.z);
    let footprint =
        kiln_footprint(anchor).map_err(|error| io::Error::new(ErrorKind::InvalidInput, error))?;
    let payload = KilnPayload::new(facing);
    let states = kiln_block_states(&catalog, &payload).map_err(io::Error::other)?;
    let mut coords = Vec::with_capacity(2);
    let mut displaced_plants = Vec::new();
    for cell in &footprint {
        let before = if *cell == anchor {
            previous
        } else {
            super::cached_block_or_request(
                state,
                cell.x,
                cell.y,
                cell.z,
                "kiln upper chunk is not resident",
            )?
        };
        if catalog.block_flags(before) & REPLACEABLE == 0
            || state.entities.anchored_at(*cell).is_some()
        {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "kiln footprint cannot be replaced",
            ));
        }
        if catalog.block_flags(states[0]) & SOLID != 0
            && state
                .clients
                .values()
                .any(|client| block_intersects_player([cell.x, cell.y, cell.z], client.position()))
        {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "kiln footprint overlaps a player",
            ));
        }
        let block = if *cell == anchor {
            states[0]
        } else {
            states[1]
        };
        coords.push((cell.x, cell.y, cell.z, block));
        if catalog.block_flags(before) & PLANT != 0 {
            displaced_plants.push((before, [cell.x, cell.y, cell.z]));
        }
    }
    let entities = state
        .entities
        .prepare_spawn(
            payload
                .spawn(anchor, tick.get(), &catalog)
                .map_err(io::Error::other)?,
        )
        .map_err(io::Error::other)?;
    let mut inventory = inventory_before.clone();
    if !inventory.consume(command.slot, KILN_ITEM) {
        return Err(io::Error::new(
            ErrorKind::PermissionDenied,
            "selected kiln stack empty",
        ));
    }
    let world_edits = state.world.prepare_edits(&coords)?;
    let deltas = prepared_deltas(&coords, &world_edits);
    let mut drop_spawns = Vec::new();
    for (plant, at) in displaced_plants {
        let version = version_at(&deltas, at).unwrap_or(0);
        push_harvest_spawns(&mut drop_spawns, &catalog, plant, at, version, state.seed);
    }
    let drops = state.drops.plan_spawns(&drop_spawns)?;
    Ok(CommitAction {
        client_id: Some(command.id),
        profile: Some(command.profile),
        action_id: Some(command.action_id),
        receipt_value: Some(command.receipt_value),
        receipt_transition: None,
        inventory_before: Some(InventoryStore::encode_snapshot_with_catalog(
            &inventory_before,
            &catalog,
        )?),
        inventory: Some(inventory),
        world_edits,
        drops,
        deltas,
        changed_cells: coords
            .iter()
            .map(|&(x, y, z, _)| EffectCell::new(x, y, z))
            .collect(),
        pickups: Vec::new(),
        fire_seed: None,
        entities: Some(entities),
    })
}

pub(in crate::server) fn plan_break(
    state: &mut State,
    _tick: TickId,
    command: BlockEditCommand,
    _previous: BlockId,
) -> io::Result<CommitAction> {
    let catalog = state.world.catalog_arc();
    let broken = CellCoord::new(command.x, command.y, command.z);
    let id = state
        .entities
        .anchored_at(broken)
        .ok_or_else(|| corrupt("kiln block has no anchored entity"))?;
    let snapshot = state
        .entities
        .snapshot(id)
        .ok_or_else(|| corrupt("kiln footprint references a missing entity"))?;
    if snapshot.entity_type != crate::content::KILN_ENTITY_TYPE {
        return Err(corrupt("kiln footprint references a different entity type"));
    }
    let anchor = snapshot
        .anchor()
        .ok_or_else(|| corrupt("kiln entity is not anchored"))?;
    let payload =
        kiln_payload(&snapshot).ok_or_else(|| corrupt("kiln entity payload type mismatch"))?;
    let planned = plan_kiln_break(anchor, broken, payload, &catalog)
        .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))?;
    let states = kiln_block_states(&catalog, payload).map_err(io::Error::other)?;
    let mut coords = Vec::with_capacity(planned.removed_cells.len());
    for cell in &planned.removed_cells {
        if state.entities.anchored_at(*cell) != Some(id) {
            return Err(corrupt("kiln footprint index is incomplete"));
        }
        let actual = super::cached_block_or_request(
            state,
            cell.x,
            cell.y,
            cell.z,
            "kiln footprint chunk is not resident",
        )?;
        let expected = if *cell == anchor {
            states[0]
        } else {
            states[1]
        };
        if actual != expected {
            return Err(corrupt("kiln block state differs from anchored entity"));
        }
        coords.push((cell.x, cell.y, cell.z, AIR));
    }
    let entities = state
        .entities
        .prepare_despawn(id, snapshot.revision)
        .map_err(io::Error::other)?;
    let world_edits = state.world.prepare_edits(&coords)?;
    let deltas = prepared_deltas(&coords, &world_edits);
    let drop_position = [
        anchor.x as f32 + 0.5,
        anchor.y as f32 + 0.5,
        anchor.z as f32 + 0.5,
    ];
    let spawns: Vec<_> = planned
        .drops
        .into_iter()
        .map(|stack: Stack| (drop_position, stack, Duration::from_millis(250)))
        .collect();
    let drops = state.drops.plan_stack_spawns(&spawns)?;
    Ok(CommitAction {
        client_id: Some(command.id),
        profile: Some(command.profile),
        action_id: Some(command.action_id),
        receipt_value: Some(command.receipt_value),
        receipt_transition: None,
        inventory_before: None,
        inventory: None,
        world_edits,
        drops,
        deltas,
        changed_cells: coords
            .iter()
            .map(|&(x, y, z, _)| EffectCell::new(x, y, z))
            .collect(),
        pickups: Vec::new(),
        fire_seed: None,
        entities: Some(entities),
    })
}

fn version_at(deltas: &[BlockDelta], at: [i32; 3]) -> Option<u64> {
    let (key, _) = crate::world::world_to_chunk(at[0], at[1], at[2]);
    deltas
        .iter()
        .find(|delta| delta.key == key)
        .map(|delta| delta.version)
}

fn corrupt(reason: &'static str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, reason)
}
