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
    CellCoord, EntityBlockStateChange, EntityId, EntityLocation, EntityPatch, EntityView,
    MAX_PLAN_NEIGHBOUR_BYTES, MAX_PLAN_NEIGHBOURS, position_to_cell,
};
use crate::server::streaming::request_chunk;
use crate::server::voxel_view::VoxelView;
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

/// The planner's declared read set: footprint chunks plus the Chebyshev
/// neighborhood of the entity's chunk. Both views are captured over exactly
/// these keys.
fn plan_chunk_keys(location: &EntityLocation, radius_chunks: u8) -> io::Result<BTreeSet<ChunkKey>> {
    let center_chunk = match location {
        EntityLocation::Anchored { anchor, .. } => anchor.chunk(),
        EntityLocation::Mobile { position } => position_to_cell(*position)
            .map_err(|_| corrupt("entity position is outside the world"))?
            .chunk(),
    };
    let mut keys = BTreeSet::new();
    if let EntityLocation::Anchored { footprint, .. } = location {
        keys.extend(footprint.iter().map(|cell| cell.chunk()));
    }
    let radius = i64::from(radius_chunks);
    for dx in -radius..=radius {
        for dy in -radius..=radius {
            for dz in -radius..=radius {
                keys.insert(ChunkKey {
                    x: center_chunk.x + dx as i32,
                    y: center_chunk.y + dy as i32,
                    z: center_chunk.z + dz as i32,
                });
            }
        }
    }
    Ok(keys)
}

/// Captures the planner's declared read set as an immutable voxel view.
/// Every key must already be resident; when any are missing, the whole
/// missing set is requested before planning defers with `WouldBlock`.
/// Requesting all of them converges in one loader round instead of stalling
/// one round per chunk, and matches the footprint-preimage retry shape.
/// Nothing is generated or read through to storage here, and the returned
/// view cannot mutate the world.
pub(super) fn capture_view_for_plan(
    state: &mut State,
    location: &EntityLocation,
    radius_chunks: u8,
) -> io::Result<VoxelView> {
    let keys = plan_chunk_keys(location, radius_chunks)?;
    let catalog = state.world.catalog_arc();
    let mut chunks = Vec::with_capacity(keys.len());
    let mut missing = Vec::new();
    for key in keys {
        match state.world.cached_arc_chunk(key) {
            Some(chunk) => chunks.push(chunk),
            None => missing.push(key),
        }
    }
    if !missing.is_empty() {
        for key in missing {
            // Best-effort prefetch, matching invoke_hook: a failed request
            // must not decide the plan. Anything not queued stays missing,
            // so planning still defers below.
            let _ = request_chunk(state, key);
        }
        return Err(io::Error::new(
            ErrorKind::WouldBlock,
            "entity view chunk is not resident",
        ));
    }
    VoxelView::from_resident_chunks_in(chunks, catalog).map_err(|error| {
        io::Error::new(
            ErrorKind::InvalidData,
            format!("entity view snapshot invalid: {error:?}"),
        )
    })
}

/// Assembles the planner's neighbour view from bounded per-chunk public
/// projections over the same captured keys. The planner's own record is
/// excluded; entries are sorted by entity ID for deterministic planning.
/// Only public projections cross: another entity's private payload is never
/// consulted here, and no path below reaches it.
///
/// Missing chunks defer (see `capture_view_for_plan`); over-cap pages and
/// views do not. Unlike a chunk that simply has not loaded, exceeding the
/// capture bound is stable: no amount of waiting makes more than
/// `MAX_PLAN_NEIGHBOURS` neighbours fit. Deferring would re-queue forever
/// and pin a deferred queue slot, starving other entities' work; truncating
/// would plan from a partial neighbour set; silently rejecting the request
/// would let the due scan re-enqueue the same unplannable entity every
/// pass. So the stable case escalates as the coordinator's reported
/// unrecoverable outcome (`InvalidData` closes durable admission), matching
/// how other entity-state violations fail closed.
pub(super) fn capture_entity_view_for_plan(
    state: &mut State,
    location: &EntityLocation,
    radius_chunks: u8,
    exclude: EntityId,
) -> io::Result<EntityView> {
    let keys = plan_chunk_keys(location, radius_chunks)?;
    let mut collected = Vec::new();
    for key in keys {
        let views = state
            .entities
            .public_views_for_chunk_bounded(key, MAX_PLAN_NEIGHBOURS)
            .map_err(|_| {
                io::Error::new(
                    ErrorKind::InvalidData,
                    "entity neighbour page exceeds its capture bound",
                )
            })?;
        collected.extend(views);
    }
    let view = EntityView::assemble(collected, exclude);
    if view.len() > MAX_PLAN_NEIGHBOURS {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "entity neighbour view exceeds its capture count bound",
        ));
    }
    if view.bytes() > MAX_PLAN_NEIGHBOUR_BYTES {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "entity neighbour view exceeds its capture byte bound",
        ));
    }
    Ok(view)
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
    // Copy the declared radius out so view capture can borrow the world
    // while no entity borrow is live; the descriptor is re-resolved below
    // over unchanged entity state.
    let read_radius = descriptor.interaction_read_radius();
    let catalog = state.world.catalog_arc();
    let view = capture_view_for_plan(state, &snapshot.location, read_radius)?;
    let neighbours =
        capture_entity_view_for_plan(state, &snapshot.location, read_radius, snapshot.id)?;
    let descriptor = state
        .entities
        .types()
        .descriptor(snapshot.entity_type)
        .map_err(io::Error::other)?;
    let plan = descriptor
        .plan_interaction(
            &snapshot,
            request,
            &inventory_before,
            &catalog,
            &view,
            &neighbours,
        )
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
    // Copy the declared radius out so view capture can borrow the world
    // while no entity borrow is live; the descriptor is re-resolved below
    // over unchanged entity state.
    let read_radius = descriptor.tick_read_radius();
    let catalog = state.world.catalog_arc();
    let view = capture_view_for_plan(state, &snapshot.location, read_radius)?;
    let neighbours =
        capture_entity_view_for_plan(state, &snapshot.location, read_radius, snapshot.id)?;
    let descriptor = state
        .entities
        .types()
        .descriptor(snapshot.entity_type)
        .map_err(io::Error::other)?;
    let plan = descriptor
        .plan_tick(&snapshot, current_tick, &catalog, &view, &neighbours)
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
        // Footprint states must be catalog-known. Anchor states additionally
        // exclude air by construction: registration rejects `BlockStateId(0)`
        // in compatible sets and spawning requires compatible membership, so
        // the anchor representation is always a real block.
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
