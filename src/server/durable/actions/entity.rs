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
    AnchorUpdate, CellCoord, EntityBlockStateChange, EntityId, EntityItemTransfer, EntityLocation,
    EntityPatch, EntityPayload, EntitySnapshot, EntityView, MAX_PLAN_NEIGHBOUR_BYTES,
    MAX_PLAN_NEIGHBOURS, PreparedEntityTransaction, canonical_wakes, interact_producer,
    position_to_cell, route_wakes, tick_producer,
};
use crate::server::registry::SystemId;
use crate::server::simulation::TickId;
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
/// projections over the same captured keys, merging WAL-owned entities and
/// session players into one view. The planner's own record is
/// excluded; entries are sorted by entity ID for deterministic planning.
/// Only public projections cross: another entity's private payload is never
/// consulted here, and no path below reaches it. Player entries are exactly
/// the public views other clients already receive for those sessions — the
/// player store keeps no private payload, and session identity, inventory,
/// and movement authority never enter this view.
///
/// Missing chunks defer (see `capture_view_for_plan`); over-cap pages and
/// views do not. Unlike a chunk that simply has not loaded, exceeding the
/// capture bound is stable: no amount of waiting makes more than
/// `MAX_PLAN_NEIGHBOURS` neighbours fit. Deferring would re-queue forever
/// and pin a deferred queue slot, starving other entities' work; truncating
/// would plan from a partial neighbour set. So the stable case rejects just
/// this ONE entity's work as a capacity outcome (`QuotaExceeded`): the
/// coordinator turns that into a per-request rejection, drops the tick, and
/// keeps every other entity progressing. Genuine state corruption keeps the
/// coordinator-fatal `InvalidData` outcome; capacity must never take it.
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
                    ErrorKind::QuotaExceeded,
                    "entity neighbour page exceeds its capture bound",
                )
            })?;
        collected.extend(views);
        // Players observe the same bound through the same outcome: a crowded
        // player page is capacity (`QuotaExceeded`), never corruption, so it
        // defers this one entity's work without stopping the coordinator.
        let players = state
            .player_entities
            .public_views_for_chunk_bounded(key, MAX_PLAN_NEIGHBOURS)
            .map_err(|_| {
                io::Error::new(
                    ErrorKind::QuotaExceeded,
                    "player neighbour page exceeds its capture bound",
                )
            })?;
        collected.extend(players);
    }
    let view = EntityView::assemble(collected, exclude);
    if view.len() > MAX_PLAN_NEIGHBOURS {
        return Err(io::Error::new(
            ErrorKind::QuotaExceeded,
            "entity neighbour view exceeds its capture count bound",
        ));
    }
    if view.bytes() > MAX_PLAN_NEIGHBOUR_BYTES {
        return Err(io::Error::new(
            ErrorKind::QuotaExceeded,
            "entity neighbour view exceeds its capture byte bound",
        ));
    }
    Ok(view)
}

/// Plan an opaque entity interaction after the coordinator has validated its
/// action receipt. The registered type policy owns request decoding and exact
/// item transfer rules; this layer resolves reach, footprint, and WAL keys.
#[allow(clippy::too_many_arguments)]
pub(in crate::server) fn plan_interact(
    state: &mut State,
    client_id: u64,
    profile: u128,
    action_id: u128,
    target: [i32; 3],
    request: &[u8],
    receipt_value: Vec<u8>,
    tick: TickId,
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
    let wakes = plan_wakes(
        state,
        &neighbours,
        &plan.wakes,
        tick,
        interact_producer(),
        id,
    )?;
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
        deltas,
        changed_cells: changed_cells
            .into_iter()
            .map(|cell| EffectCell::new(cell.x, cell.y, cell.z))
            .collect(),
        pickups: Vec::new(),
        fire_seed: None,
        entities: Some(entities),
        entity_wakes: wakes,
    })
}

/// Plan one due tick through the registered type policy. All block preimages
/// and the entity payload/next due time are staged under the same WAL receipt.
///
/// A woken attempt (`woken == true`, delivered from another plan's wake
/// effect) runs the planner even before the persisted due time so the
/// destination's own durable work happens sooner. Every other validation —
/// footprint preimages, due-time advancement, payload rules — is identical to
/// the due path.
pub(in crate::server) fn plan_entity_tick(
    state: &mut State,
    id: EntityId,
    current_tick: u64,
    woken: bool,
) -> io::Result<Option<CommitAction>> {
    let Some(snapshot) = state.entities.snapshot(id) else {
        return Ok(None);
    };
    if !woken && snapshot.next_tick.is_none_or(|due| due > current_tick) {
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
    // A woken planner with nothing to do reaffirms its persisted schedule
    // instead of advancing it. Without this the attempt would either stage a
    // no-op commit (`NoChanges`) or churn the schedule on every stray wake;
    // reaffirming is a no-op that leaves the entity on its durable grid, so
    // dropping the wake converges to the same state. The due path below is
    // unchanged: only woken attempts may reaffirm. A declared transfer or a
    // declared position change is something to do, so neither reaffirms away.
    // A suspended (`None`) schedule reaffirms against `None`: a stray wake
    // on a settled entity stages nothing.
    if woken
        && plan.payload.is_none()
        && plan.transfer.is_none()
        && plan.position.is_none()
        && snapshot.next_tick == plan.next_tick
    {
        return Ok(None);
    }
    if !descriptor.tick_policy().validates(plan.next_tick) {
        return Err(corrupt("entity tick planner returned an invalid due time"));
    }
    if let (Some(previous), Some(next)) = (snapshot.next_tick, plan.next_tick)
        && next <= previous
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
    let wakes = plan_wakes(
        state,
        &neighbours,
        &plan.wakes,
        TickId::new(current_tick),
        tick_producer(),
        id,
    )?;
    let patch = EntityPatch {
        payload: plan.payload.clone(),
        next_tick: Some(plan.next_tick),
        position: None,
    };
    let mut entities = if let Some(position) = plan.position {
        if plan.anchor_update.is_some() {
            return Err(corrupt("mobile entity planner returned an anchor update"));
        }
        if !matches!(&snapshot.location, EntityLocation::Mobile { .. }) {
            return Err(corrupt("anchored entity planner returned a position"));
        }
        if !position.iter().all(|coordinate| coordinate.is_finite()) {
            return Err(corrupt(
                "entity tick planner returned a non-finite position",
            ));
        }
        // Same-owner motion stages as one update; a chunk crossing stages
        // as one fenced barrier transfer carrying the same payload and
        // schedule patch. Either way the tick is exactly one WAL record.
        match state.entities.prepare_update(
            id,
            snapshot.revision,
            EntityPatch {
                payload: plan.payload.clone(),
                next_tick: Some(plan.next_tick),
                position: Some(position),
            },
        ) {
            Ok(prepared) => prepared,
            Err(crate::server::entities::EntityError::TransferRequired) => state
                .entities
                .prepare_transfer(id, snapshot.revision, position, patch)
                .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))?,
            Err(error) => return Err(io::Error::new(ErrorKind::InvalidData, error)),
        }
    } else if let Some(transfer) = &plan.transfer {
        // The recipient pulls on its own schedule: its tick plan carries its
        // own payload update (or none) plus the declared pull, and this layer
        // stages both ends atomically. The sender's schedule is untouched. A
        // pull without a receiver schedule is not expressible: suspension
        // with a transfer would leave the batch half-specified.
        let Some(next_tick) = plan.next_tick else {
            return Err(corrupt(
                "entity tick planner returned a transfer without a due time",
            ));
        };
        let receiver_base = plan
            .payload
            .clone()
            .unwrap_or_else(|| snapshot.private_payload.clone());
        plan_transfer_batch(
            state,
            id,
            &snapshot,
            &neighbours,
            &receiver_base,
            plan.anchor_update.as_ref(),
            next_tick,
            transfer,
            &catalog,
        )?
    } else if let Some(anchor_update) = plan.anchor_update {
        state
            .entities
            .prepare_anchor_update(id, snapshot.revision, anchor_update, patch)
            .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))?
    } else {
        state
            .entities
            .prepare_update(id, snapshot.revision, patch)
            .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))?
    };
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
        deltas,
        changed_cells: changed_cells
            .into_iter()
            .map(|cell| EffectCell::new(cell.x, cell.y, cell.z))
            .collect(),
        pickups: Vec::new(),
        fire_seed: None,
        entities: Some(entities),
        entity_wakes: wakes,
    }))
}

/// Assembles one atomic cross-entity item transfer as a single WAL batch.
///
/// The recipient declared `transfer` in its tick plan; this trusted layer
/// resolves both snapshots, runs both pure exchange hooks, and stages both
/// payload updates with real `before` preimages in one `Operation::Batch`.
/// The check and the move are the same statement: a recipient `before` that
/// goes stale before the receipt rejects the whole transaction, so items are
/// neither created nor destroyed and a non-fitting transfer never partially
/// applies. The 128 stack cap is enforced on the taken stack and by both
/// type validators during preparation.
///
/// The sender's schedule is untouched: it never deducts speculatively, it
/// only loses stock inside this batch. Unavailable stock or a full
/// destination defers (`WouldBlock`) so the work re-plans. Anything the
/// planner got wrong — an unknown source, a source outside its declared
/// neighbour view, a missing exchange hook, a mismatched take — rejects the
/// whole plan (`InvalidInput`). No outcome here stops the coordinator.
#[allow(clippy::too_many_arguments)]
fn plan_transfer_batch(
    state: &State,
    receiver: EntityId,
    snapshot: &EntitySnapshot,
    neighbours: &EntityView,
    receiver_base: &EntityPayload,
    anchor_update: Option<&AnchorUpdate>,
    next_tick: u64,
    transfer: &EntityItemTransfer,
    catalog: &crate::content::Catalog,
) -> io::Result<PreparedEntityTransaction> {
    transfer
        .validate(receiver)
        .map_err(|error| io::Error::new(ErrorKind::InvalidInput, error))?;
    // A policy can only move items it can already see. The source must be in
    // the captured neighbour view: anything else is a write the plan did not
    // declare, so the whole plan is rejected.
    if !neighbours.iter().any(|view| view.id == transfer.source) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "transfer source is outside the declared neighbour view",
        ));
    }
    let source_snapshot = state
        .entities
        .snapshot(transfer.source)
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "transfer source is unknown"))?;
    if catalog.item(transfer.item).is_none() {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "transfer item is unknown",
        ));
    }
    let receiver_descriptor = state
        .entities
        .types()
        .descriptor(snapshot.entity_type)
        .map_err(io::Error::other)?;
    let source_descriptor = state
        .entities
        .types()
        .descriptor(source_snapshot.entity_type)
        .map_err(io::Error::other)?;
    let receiver_exchange = receiver_descriptor.transfer_policy().ok_or_else(|| {
        io::Error::new(
            ErrorKind::InvalidInput,
            "receiving type cannot exchange items",
        )
    })?;
    let source_exchange = source_descriptor.transfer_policy().ok_or_else(|| {
        io::Error::new(
            ErrorKind::InvalidInput,
            "transfer source type cannot exchange items",
        )
    })?;
    let (sender_after, taken) = source_exchange
        .withdraw(
            &source_snapshot.private_payload,
            transfer.item,
            transfer.count,
            catalog,
        )
        .map_err(|error| io::Error::new(ErrorKind::InvalidInput, error))?
        .ok_or_else(|| io::Error::new(ErrorKind::WouldBlock, "transfer source has no stock"))?;
    if taken.item != transfer.item || taken.count != transfer.count || !taken.valid_in(catalog) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "transfer source returned a mismatched take",
        ));
    }
    let receiver_after = receiver_exchange
        .deposit(receiver_base, &taken, catalog)
        .map_err(|error| io::Error::new(ErrorKind::InvalidInput, error))?
        .ok_or_else(|| {
            io::Error::new(
                ErrorKind::WouldBlock,
                "transfer destination cannot fit the stack",
            )
        })?;
    let receiver_patch = EntityPatch {
        payload: Some(receiver_after),
        next_tick: Some(Some(next_tick)),
        position: None,
    };
    let receiver_prepared = if let Some(anchor_update) = anchor_update {
        state.entities.prepare_anchor_update(
            receiver,
            snapshot.revision,
            anchor_update.clone(),
            receiver_patch,
        )
    } else {
        state
            .entities
            .prepare_update(receiver, snapshot.revision, receiver_patch)
    }
    .map_err(|error| io::Error::new(ErrorKind::InvalidInput, error))?;
    let sender_prepared = state
        .entities
        .prepare_update(
            transfer.source,
            source_snapshot.revision,
            EntityPatch {
                payload: Some(sender_after),
                next_tick: None,
                position: None,
            },
        )
        .map_err(|error| io::Error::new(ErrorKind::InvalidInput, error))?;
    state
        .entities
        .combine_prepared(vec![receiver_prepared, sender_prepared])
        .map_err(|error| io::Error::new(ErrorKind::InvalidInput, error))
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

/// Canonicalizes a plan's wake list against its captured neighbours, checks
/// every destination is a scheduled tickable entity, and routes the set
/// through this producer's bounded effect buffer.
///
/// The existing plan validation (footprint preimages, inventory revisions,
/// neighbour-view caps) is untouched; this only constrains the new channel.
/// Over-bound or unroutable sets defer the plan (`WouldBlock`), never the
/// coordinator-fatal `InvalidData` path: a local effect-capacity condition
/// must not be able to stop the server. Destinations the planner cannot see,
/// that are unknown, or that cannot tick reject the whole plan
/// (`InvalidInput`).
fn plan_wakes(
    state: &State,
    neighbours: &EntityView,
    wakes: &[EntityId],
    tick: TickId,
    producer: SystemId,
    source: EntityId,
) -> io::Result<Vec<EntityId>> {
    if wakes.is_empty() {
        return Ok(Vec::new());
    }
    let canonical = canonical_wakes(neighbours, wakes)?;
    for id in &canonical {
        let snapshot = state.entities.snapshot(*id).ok_or_else(|| {
            io::Error::new(ErrorKind::InvalidInput, "wake destination is unknown")
        })?;
        let descriptor = state
            .entities
            .types()
            .descriptor(snapshot.entity_type)
            .map_err(io::Error::other)?;
        if !descriptor.has_tick_planner() || snapshot.next_tick.is_none() {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "wake destination cannot tick",
            ));
        }
    }
    route_wakes(&state.effect_kinds, tick, producer, source, &canonical)
}
