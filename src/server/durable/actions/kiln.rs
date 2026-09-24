//! Trusted two-cell kiln edit planning. No world, item, or entity mutation is
//! visible here; all participants enter one WAL record through `CommitAction`.

use super::{BlockEditCommand, prepared_deltas, push_harvest_spawns};
use crate::content::{KILN_ITEM, PLANT, REPLACEABLE, SOLID};
use crate::inventory::{HOTBAR_SLOTS, InventoryStore, Stack};
use crate::server::durable::{BlockDelta, CommitAction};
use crate::server::effects::CellCoord as EffectCell;
use crate::server::entities::{
    CellCoord, EntityBlockStateChange, EntityId, EntityLocation, EntityPatch, KilnFacing,
    KilnPayload, kiln_block_states, kiln_footprint, kiln_payload, plan_break as plan_kiln_break,
};
use crate::server::simulation::TickId;
use crate::server::{AIR, State, block_intersects_player};
use crate::world::{BlockId, ChunkKey};
use std::collections::BTreeSet;
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

/// Plan an opaque entity interaction after the coordinator has validated its
/// action receipt. The registered type policy owns request decoding and exact
/// item transfer rules; this layer resolves reach, footprint, and WAL keys.
pub(in crate::server) fn plan_interact(
    state: &mut State,
    client_id: u64,
    profile: u128,
    action_id: u128,
    target: [i32; 3],
    request: &[u8],
    receipt_value: Vec<u8>,
) -> io::Result<CommitAction> {
    let target_cell = CellCoord::new(target[0], target[1], target[2]);
    if target[1] <= crate::world::BEDROCK_Y {
        return Err(permission(
            "entity interaction target is outside world bounds",
        ));
    }
    let client = state
        .clients
        .get(&client_id)
        .ok_or_else(|| io::Error::new(ErrorKind::NotConnected, "entity client disconnected"))?;
    let position = client.position();
    let distance_sq = (target[0] as f32 + 0.5 - position[0]).powi(2)
        + (target[1] as f32 + 0.5 - (position[1] + 1.6)).powi(2)
        + (target[2] as f32 + 0.5 - position[2]).powi(2);
    if distance_sq > crate::server::EDIT_REACH * crate::server::EDIT_REACH
        || !client.interested(target_cell.chunk())
    {
        return Err(permission("entity interaction target is out of reach"));
    }
    let inventory_before = client.inventory.clone();
    let id = state
        .entities
        .anchored_at(target_cell)
        .ok_or_else(|| permission("no anchored entity at target"))?;
    let snapshot = state
        .entities
        .snapshot(id)
        .ok_or_else(|| corrupt("entity footprint references a missing record"))?;
    let descriptor = state
        .entities
        .types()
        .descriptor(snapshot.entity_type)
        .map_err(io::Error::other)?;
    if !descriptor.has_interaction_policy() {
        return Err(permission("entity type does not support interactions"));
    }
    let catalog = state.world.catalog_arc();
    let plan = descriptor
        .plan_interaction(&snapshot, request, &inventory_before, &catalog)
        .map_err(|error| io::Error::new(ErrorKind::InvalidInput, error))?;
    let expected_inventory_revision = inventory_before
        .revision
        .checked_add(1)
        .ok_or_else(|| permission("inventory revision exhausted"))?;
    if plan.inventory.revision != expected_inventory_revision
        || plan
            .inventory
            .slots
            .iter()
            .flatten()
            .any(|stack| !stack.valid_in(&catalog))
    {
        return Err(corrupt(
            "entity policy returned an invalid inventory transition",
        ));
    }
    let (world_edits, changed_cells, read_chunks, write_coords) =
        validate_footprint_plan(state, id, &snapshot.location, &plan.block_states, &catalog)?;
    let mut entities = state
        .entities
        .prepare_update(
            id,
            snapshot.revision,
            EntityPatch {
                payload: Some(plan.payload),
                next_tick: None,
                position: None,
            },
        )
        .map_err(|error| io::Error::new(ErrorKind::InvalidInput, error))?;
    for chunk in read_chunks {
        entities.add_read_key(super::super::chunk_state_key(chunk));
    }
    let deltas = prepared_deltas(&write_coords, &world_edits);
    Ok(CommitAction {
        client_id: Some(client_id),
        profile: Some(profile),
        action_id: Some(action_id),
        receipt_value: Some(receipt_value),
        receipt_transition: None,
        inventory_before: Some(InventoryStore::encode_snapshot_with_catalog(
            &inventory_before,
            &catalog,
        )?),
        inventory: Some(plan.inventory),
        world_edits,
        drops: Default::default(),
        deltas,
        changed_cells: changed_cells
            .into_iter()
            .map(|cell| EffectCell::new(cell.x, cell.y, cell.z))
            .collect(),
        pickups: Vec::new(),
        fire_seed: None,
        entities: Some(entities),
    })
}

/// Plan one due tick through the registered type policy. All block preimages
/// and the entity payload/next due time are staged under the same WAL receipt.
pub(in crate::server) fn plan_entity_tick(
    state: &mut State,
    id: EntityId,
    current_tick: u64,
) -> io::Result<Option<CommitAction>> {
    let Some(snapshot) = state.entities.snapshot(id) else {
        return Ok(None);
    };
    if snapshot.next_tick.is_none_or(|due| due > current_tick) {
        return Ok(None);
    }
    let descriptor = state
        .entities
        .types()
        .descriptor(snapshot.entity_type)
        .map_err(io::Error::other)?;
    if !descriptor.has_tick_planner() {
        return Err(corrupt("due entity type has no tick planner"));
    }
    let catalog = state.world.catalog_arc();
    let plan = descriptor
        .plan_tick(&snapshot, current_tick, &catalog)
        .map_err(|error| io::Error::new(ErrorKind::InvalidInput, error))?;
    if snapshot
        .next_tick
        .is_none_or(|previous| plan.next_tick <= previous)
        || !descriptor.tick_policy().validates(Some(plan.next_tick))
    {
        return Err(corrupt("entity tick planner returned an invalid due time"));
    }
    if let (
        Some(anchor_update),
        EntityLocation::Anchored {
            anchor, footprint, ..
        },
    ) = (&plan.anchor_update, &snapshot.location)
    {
        if anchor_update.anchor != *anchor
            || anchor_update.footprint != *footprint
            || catalog.state(anchor_update.anchor_state).is_none()
        {
            return Err(corrupt("entity tick planner changed its footprint"));
        }
    } else if plan.anchor_update.is_some() {
        return Err(corrupt("mobile entity planner returned an anchor update"));
    }
    let (world_edits, changed_cells, read_chunks, write_coords) =
        validate_footprint_plan(state, id, &snapshot.location, &plan.block_states, &catalog)?;
    let patch = EntityPatch {
        payload: plan.payload,
        next_tick: Some(Some(plan.next_tick)),
        position: None,
    };
    let mut entities = if let Some(anchor_update) = plan.anchor_update {
        state
            .entities
            .prepare_anchor_update(id, snapshot.revision, anchor_update, patch)
    } else {
        state.entities.prepare_update(id, snapshot.revision, patch)
    }
    .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))?;
    for chunk in read_chunks {
        entities.add_read_key(super::super::chunk_state_key(chunk));
    }
    let deltas = prepared_deltas(&write_coords, &world_edits);
    Ok(Some(CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        inventory_before: None,
        inventory: None,
        world_edits,
        drops: Default::default(),
        deltas,
        changed_cells: changed_cells
            .into_iter()
            .map(|cell| EffectCell::new(cell.x, cell.y, cell.z))
            .collect(),
        pickups: Vec::new(),
        fire_seed: None,
        entities: Some(entities),
    }))
}

fn validate_footprint_plan(
    state: &mut State,
    id: EntityId,
    location: &EntityLocation,
    block_states: &[EntityBlockStateChange],
    catalog: &crate::content::Catalog,
) -> io::Result<(
    Vec<crate::world::PreparedEdit>,
    Vec<CellCoord>,
    Vec<ChunkKey>,
    Vec<(i32, i32, i32, BlockId)>,
)> {
    let footprint: Vec<CellCoord> = match location {
        EntityLocation::Mobile { .. } => Vec::new(),
        EntityLocation::Anchored { footprint, .. } => footprint.clone(),
    };
    if block_states.len() != footprint.len()
        || block_states
            .windows(2)
            .any(|pair| pair[0].cell >= pair[1].cell)
        || block_states
            .iter()
            .map(|change| change.cell)
            .ne(footprint.iter().copied())
    {
        return Err(corrupt("entity policy returned an incomplete footprint"));
    }
    let mut write_coords = Vec::new();
    let mut changed_cells = Vec::new();
    let mut read_chunks = BTreeSet::new();
    for change in block_states {
        if catalog.state(change.before).is_none() || catalog.state(change.after).is_none() {
            return Err(corrupt("entity policy returned an unknown block state"));
        }
        if state.entities.anchored_at(change.cell) != Some(id) {
            return Err(corrupt("entity footprint index is incomplete"));
        }
        let actual = super::cached_block_or_request(
            state,
            change.cell.x,
            change.cell.y,
            change.cell.z,
            "entity footprint chunk is not resident",
        )?;
        if actual != change.before {
            return Err(corrupt(
                "entity block preimage differs from registered policy",
            ));
        }
        read_chunks.insert(change.cell.chunk());
        if change.before != change.after {
            write_coords.push((change.cell.x, change.cell.y, change.cell.z, change.after));
            changed_cells.push(change.cell);
        }
    }
    let world_edits = if write_coords.is_empty() {
        Vec::new()
    } else {
        state.world.prepare_edits(&write_coords)?
    };
    Ok((
        world_edits,
        changed_cells,
        read_chunks.into_iter().collect(),
        write_coords,
    ))
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

fn permission(reason: &'static str) -> io::Error {
    io::Error::new(ErrorKind::PermissionDenied, reason)
}
