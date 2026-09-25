//! Type-agnostic anchored-entity planning for interactions and due ticks.
//!
//! The registered type policy owns request decoding, tick scheduling, and
//! exact payload rules; this layer resolves reach, validates footprint
//! preimages against resident chunks, and stages one WAL record through
//! `CommitAction`. No block or entity type is named here: dispatch goes
//! through the entity descriptor resolved from the live record.

use super::prepared_deltas;
use crate::inventory::InventoryStore;
use crate::server::State;
use crate::server::durable::CommitAction;
use crate::server::effects::CellCoord as EffectCell;
use crate::server::entities::{
    CellCoord, EntityBlockStateChange, EntityId, EntityLocation, EntityPatch,
};
use crate::world::{BlockId, ChunkKey, PreparedEdit};
use std::collections::BTreeSet;
use std::io::{self, ErrorKind};

/// Staged footprint outcome: prepared chunk edits, changed cells, read
/// chunks for WAL fences, and the exact written coordinates for deltas.
type FootprintPlan = (
    Vec<PreparedEdit>,
    Vec<CellCoord>,
    Vec<ChunkKey>,
    Vec<(i32, i32, i32, BlockId)>,
);

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
) -> io::Result<FootprintPlan> {
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

pub(super) fn corrupt(reason: &'static str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, reason)
}

pub(super) fn permission(reason: &'static str) -> io::Error {
    io::Error::new(ErrorKind::PermissionDenied, reason)
}
