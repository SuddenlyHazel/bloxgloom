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
use crate::server::durable::{CommitAction, TerrainReads};
use crate::server::effects::CellCoord as EffectCell;
use crate::server::entities::{
    AnchorUpdate, CellCoord, EntityBlockStateChange, EntityDependencies, EntityId,
    EntityItemTransfer, EntityLocation, EntityPatch, EntityPayload, EntitySnapshot, EntityTickPlan,
    EntityView, MAX_PLAN_NEIGHBOUR_BYTES, MAX_PLAN_NEIGHBOURS, PreparedEntityTransaction,
    canonical_wakes, interact_producer, position_to_cell, route_wakes, tick_producer,
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
/// Only public projections and opaque automation equality keys cross. The host
/// interns bounded exact inventories during capture, discarding their component
/// bytes before worker dispatch. Player entries are exactly
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
    state: &State,
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
    let mut view = EntityView::assemble(collected, exclude);
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
    view.inventory_policies(&state.entities, exclude)
        .map_err(io::Error::other)?;
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
    id: EntityId,
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
        + (target[1] as f32 + 0.5
            - (position[1] + state.world.catalog().player_rules().eye_height()))
        .powi(2)
        + (target[2] as f32 + 0.5 - position[2]).powi(2);
    if distance_sq > crate::server::EDIT_REACH * crate::server::EDIT_REACH
        || !client.interested(target_cell.chunk())
    {
        return Err(permission("entity interaction target is out of reach"));
    }
    let inventory_before = client.inventory.clone();
    let snapshot = state
        .entities
        .snapshot(id)
        .ok_or_else(|| permission("interaction entity is no longer present"))?;
    let matches = match &snapshot.location {
        EntityLocation::Mobile { position } => {
            position_to_cell(*position).ok() == Some(target_cell)
        }
        EntityLocation::Anchored { footprint, .. } => {
            footprint.contains(&target_cell) && state.entities.anchored_at(target_cell) == Some(id)
        }
    };
    if !matches {
        return Err(permission("registered target location mismatch"));
    }
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
    let reads_neighbours = descriptor.interaction_reads_neighbours();
    let catalog = state.world.catalog_arc();
    let sight_reads = interaction_sight(state, position, target, &snapshot, &catalog)?;
    let view = capture_view_for_plan(state, &snapshot.location, read_radius)?;
    let neighbours = if reads_neighbours {
        capture_entity_view_for_plan(state, &snapshot.location, read_radius, snapshot.id)?
    } else {
        EntityView::assemble(Vec::new(), snapshot.id)
    };
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
    let (world_edits, changed_cells, _read_chunks, write_coords) =
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
    for (chunk, _) in view.revisions() {
        entities.add_read_key(super::super::chunk_state_key(*chunk));
    }
    for chunk in sight_reads {
        entities.add_read_key(super::super::chunk_state_key(chunk));
    }
    if reads_neighbours {
        entities
            .add_dependencies(capture_dependencies(state, &view)?)
            .map_err(|error| io::Error::new(ErrorKind::QuotaExceeded, error))?;
    }
    let deltas = prepared_deltas(&write_coords, &world_edits);
    Ok(CommitAction {
        client_id: Some(client_id),
        profile: Some(profile),
        action_id: Some(action_id),
        receipt_value: Some(receipt_value),
        receipt_transition: None,
        terrain_reads: Default::default(),
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
        clock_change: None,
        entities: Some(entities),
        entity_wakes: wakes,
    })
}

/// Reach bounds DDA traversal, captured keys, and load requests. Every cell read
/// for visibility is fenced through admission and receipt apply. Missing terrain
/// defers, never becomes transparent. Actor position is sampled at planning.
fn interaction_sight(
    state: &mut State,
    actor: [f32; 3],
    target: [i32; 3],
    snapshot: &EntitySnapshot,
    catalog: &crate::content::Catalog,
) -> io::Result<BTreeSet<ChunkKey>> {
    let eye = glam::Vec3::from_array(actor) + glam::Vec3::Y * catalog.player_rules().eye_height();
    let center = match snapshot.location {
        EntityLocation::Mobile { position } => {
            let body = catalog
                .mobile_entity(snapshot.entity_type)
                .ok_or_else(|| permission("mobile has no targeting body"))?
                .body;
            glam::Vec3::from_array(position) + glam::Vec3::Y * (body.height * 0.5)
        }
        _ => glam::Vec3::from_array(target.map(|v| v as f32 + 0.5)),
    };
    let delta = center - eye;
    if !delta.is_finite() || delta.length() > crate::server::EDIT_REACH {
        return Err(permission("target center out of reach"));
    }
    let mut reads = BTreeSet::new();
    let mut missing = BTreeSet::new();
    let hit = crate::raycast::raycast_with_catalog(
        eye,
        delta.normalize_or_zero(),
        delta.length(),
        |x, y, z| {
            let key = crate::world::world_to_chunk(x, y, z).0;
            reads.insert(key);
            let block = state.world.cached_block(x, y, z);
            if block.is_none() {
                missing.insert(key);
            }
            block
        },
        catalog,
    );
    if !missing.is_empty() {
        for key in missing {
            let _ = request_chunk(state, key);
        }
        return Err(io::Error::new(
            ErrorKind::WouldBlock,
            "interaction sight terrain unavailable",
        ));
    }
    if hit.is_some_and(|hit| {
        !matches!(&snapshot.location, EntityLocation::Anchored { footprint, .. } if footprint.contains(&CellCoord::new(hit.block[0],hit.block[1],hit.block[2])))
    }) { return Err(permission("interaction is occluded")); }
    Ok(reads)
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
    let Some(input) = capture_tick_input(state, id, current_tick, woken)? else {
        return Ok(None);
    };
    let plan = input
        .plan()
        .map_err(|error| io::Error::new(ErrorKind::InvalidInput, error))?;
    commit_tick_plan(state, input, plan)
}

/// Only owned, immutable policy inputs cross the worker boundary.
pub(in crate::server) struct TickInput {
    pub snapshot: EntitySnapshot,
    pub descriptor: crate::server::entities::EntityTypeDescriptor,
    pub catalog: std::sync::Arc<crate::content::Catalog>,
    pub view: VoxelView,
    pub neighbours: EntityView,
    pub current_tick: u64,
    pub woken: bool,
    pub dependencies: EntityDependencies,
}

pub(in crate::server) struct TickWorkerResult {
    pub input: TickInput,
    pub plan: EntityTickPlan,
}

impl TickInput {
    pub fn plan(&self) -> Result<EntityTickPlan, crate::server::entities::EntityError> {
        self.descriptor.plan_tick(
            &self.snapshot,
            self.current_tick,
            &self.catalog,
            &self.view,
            &self.neighbours,
        )
    }

    pub fn is_current(&self, state: &State) -> bool {
        self.dependencies.is_current(&state.entities)
            // Session players are not WAL participants. Validate their captured
            // public state (including absence) at the coordinator planning
            // boundary; subsequent movement may occur after this decision.
            && (!self.descriptor.tick_reads_neighbours()
                || capture_entity_view_for_plan(state, &self.snapshot.location,
                    self.descriptor.tick_read_radius(), self.snapshot.id)
                    .is_ok_and(|current| current.iter().eq(self.neighbours.iter())))
            && state
                .entities
                .snapshot(self.snapshot.id)
                .is_some_and(|snapshot| {
                    snapshot.revision == self.snapshot.revision
                        && snapshot.motion_revision == self.snapshot.motion_revision
                })
            && self
                .view
                .revisions_match(|key| state.world.cached_version(key))
    }
}

pub(in crate::server) fn capture_tick_input(
    state: &mut State,
    id: EntityId,
    current_tick: u64,
    woken: bool,
) -> io::Result<Option<TickInput>> {
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
        .map_err(io::Error::other)?
        .clone();
    if !descriptor.has_tick_planner() {
        return Err(corrupt("due entity type has no tick planner"));
    }
    // Copy the declared radius out so view capture can borrow the world
    // while no entity borrow is live; the descriptor is re-resolved below
    // over unchanged entity state.
    let read_radius = descriptor.tick_read_radius();
    let catalog = state.world.catalog_arc();
    let view = capture_view_for_plan(state, &snapshot.location, read_radius)?;
    let neighbours = if descriptor.tick_reads_neighbours() {
        capture_entity_view_for_plan(state, &snapshot.location, read_radius, snapshot.id)?
    } else {
        EntityView::assemble(Vec::new(), snapshot.id)
    };
    let dependencies = if descriptor.tick_reads_neighbours() {
        capture_dependencies(state, &view)?
    } else {
        EntityDependencies::default()
    };
    Ok(Some(TickInput {
        snapshot,
        descriptor,
        catalog,
        view,
        neighbours,
        current_tick,
        woken,
        dependencies,
    }))
}

pub(in crate::server) fn commit_tick_plan(
    state: &mut State,
    input: TickInput,
    plan: EntityTickPlan,
) -> io::Result<Option<CommitAction>> {
    if !input.is_current(state) {
        return Err(io::Error::new(
            ErrorKind::WouldBlock,
            "entity tick snapshot became stale",
        ));
    }
    let TickInput {
        snapshot,
        descriptor,
        catalog,
        view,
        neighbours,
        current_tick,
        woken,
        dependencies,
        ..
    } = input;
    let id = snapshot.id;
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
        && !plan.lifecycle.despawn
        && plan.lifecycle.spawns.is_empty()
        && snapshot.next_tick == plan.next_tick
    {
        return Ok(None);
    }
    if !descriptor.tick_policy().validates(plan.next_tick) {
        return Err(corrupt("entity tick planner returned an invalid due time"));
    }
    // A dependency wake may interrupt a future AI deadline (e.g. support
    // removal during idle). Its replacement must be after this tick, but
    // need not be after the old deadline. Ordinary due steps still advance it.
    if let (Some(previous), Some(next)) = (snapshot.next_tick, plan.next_tick)
        && (if woken {
            next <= current_tick
        } else {
            next <= previous
        })
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
    let (mut world_edits, mut changed_cells, _read_chunks, mut write_coords) =
        validate_footprint_plan(state, id, &snapshot.location, &plan.block_states, &catalog)?;
    let mut terrain_reads = TerrainReads::default();
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
    let mut entities = if plan.lifecycle.despawn {
        if (catalog.mobile_entity(snapshot.entity_type).is_none()
            && catalog.anchored_entity(snapshot.entity_type).is_none())
            || plan.position.is_some()
            || plan.transfer.is_some()
        {
            return Err(permission("invalid mobile removal effect"));
        }
        state
            .entities
            .prepare_despawn(id, snapshot.revision)
            .map_err(tick_preparation_error)?
    } else if let Some(position) = plan.position {
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
                .map_err(tick_preparation_error)?,
            Err(error) => return Err(tick_preparation_error(error)),
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
            .map_err(tick_preparation_error)?
    } else {
        state
            .entities
            .prepare_update(id, snapshot.revision, patch)
            .map_err(tick_preparation_error)?
    };
    if plan.lifecycle.despawn && catalog.anchored_entity(snapshot.entity_type).is_some() {
        if plan
            .block_states
            .iter()
            .any(|c| c.after != crate::world::AIR)
        {
            return Err(permission(
                "anchored removal must clear the complete footprint",
            ));
        }
        let (removal, combined) = plan_reaction_removal(
            state,
            &snapshot,
            current_tick,
            &write_coords,
            &mut terrain_reads,
            entities,
        )?;
        world_edits = removal.prepared;
        changed_cells = removal
            .edits
            .iter()
            .map(|&(x, y, z, _)| CellCoord::new(x, y, z))
            .collect();
        write_coords = removal.edits;
        entities = combined;
    }
    if !plan.lifecycle.spawns.is_empty() {
        entities = super::mobile_lifecycle::spawn_effects(
            state,
            &snapshot,
            &view,
            plan.lifecycle.spawns,
            current_tick,
            entities,
        )?;
    }
    // The footprint preimages cover only block changes. Physics and other
    // tick policies may read terrain without writing any blocks; keep their
    // whole captured terrain set fenced until the WAL receipt is applied.
    for (chunk, _) in view.revisions() {
        entities.add_read_key(super::super::chunk_state_key(*chunk));
    }
    if let Err(error) = entities.add_dependencies(dependencies) {
        state.entities.cancel_prepared(&entities);
        return Err(io::Error::new(ErrorKind::QuotaExceeded, error));
    }
    let deltas = prepared_deltas(&write_coords, &world_edits);
    Ok(Some(CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        terrain_reads,
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
        clock_change: None,
        entities: Some(entities),
        entity_wakes: wakes,
    }))
}

/// Registered reactions retain their lifecycle-owned footprint and refund, but
/// removal/neighbor decisions share the same overlay as player destruction.
/// Missing decision inputs defer the whole tick; no despawn or refund is applied
/// until these reads and every resulting participant pass WAL admission.
fn plan_reaction_removal(
    state: &mut State,
    snapshot: &EntitySnapshot,
    tick: u64,
    coords: &[crate::server::gameplay::Edit],
    reads: &mut TerrainReads,
    base: PreparedEntityTransaction,
) -> io::Result<(
    crate::server::gameplay::WorldPlan,
    PreparedEntityTransaction,
)> {
    let EntityLocation::Anchored {
        anchor,
        anchor_state,
        ..
    } = snapshot.location
    else {
        return Err(corrupt("reaction removal requires an anchor"));
    };
    let mut requested = Vec::new();
    let planned = crate::server::gameplay::plan_removals(
        &mut state.world,
        reads,
        &mut requested,
        crate::server::gameplay::OperationInput {
            edits: coords,
            // Like player anchored removal, dispatch once for the anchor with
            // lifecycle-owned loot. Generic harvest must not refund each cell.
            removals: &[(
                anchor_state,
                [anchor.x, anchor.y, anchor.z],
                bloxgloom_host_api::gameplay::RemovalCause::AnchoredBreak,
            )],
            seed: state.seed,
            tick,
            action: None,
        },
        crate::server::gameplay::Participants {
            clock: None,
            actor: None,
            actor_position: None,
            admin: false,
            entities: &state.entities,
        },
    );
    for key in requested {
        let _ = request_chunk(state, key);
    }
    let mut planned = planned?;
    if planned.inventory.is_some()
        || !planned.admin_spawns.is_empty()
        || !planned.drop_takes.is_empty()
    {
        return Err(permission(
            "reaction removal returned unsupported actor effects",
        ));
    }
    if coords.iter().any(|edit| !planned.edits.contains(edit)) {
        return Err(permission("gameplay handler changed the removed footprint"));
    }
    let catalog = state.world.catalog_arc();
    for &(x, y, z, block) in &planned.edits {
        if state
            .entities
            .anchored_at(CellCoord::new(x, y, z))
            .is_some_and(|id| id != snapshot.id)
            || (catalog.block_flags(block) & crate::content::SOLID != 0
                && state.clients.values().any(|client| {
                    crate::server::block_intersects_player(
                        catalog.player_rules().body(),
                        [x, y, z],
                        client.position(),
                    )
                }))
        {
            return Err(permission(
                "gameplay edit conflicts with an anchor or player",
            ));
        }
    }
    let entities = if planned.drops.is_empty() && planned.entity_spawns.is_empty() {
        super::anchored::refund_removal(state, snapshot, tick, base)?
    } else {
        // All new entities must use ONE allocator batch. Preparing the refund
        // and decision drops separately would reuse IDs (and could also merge
        // twice into the same existing drop's preimage).
        let definition = catalog
            .anchored_entity(snapshot.entity_type)
            .ok_or_else(|| corrupt("missing reaction removal definition"))?;
        planned.drops.extend(
            super::anchored::refund_stacks(
                &catalog,
                definition,
                &snapshot.private_payload,
                bloxgloom_host_api::anchored::RemovalCause::Reaction,
            )?
            .into_iter()
            .map(|stack| {
                (
                    [
                        anchor.x as f32 + 0.5,
                        anchor.y as f32 + 0.5,
                        anchor.z as f32 + 0.5,
                    ],
                    stack,
                    std::time::Duration::from_millis(250),
                )
            }),
        );
        // Bound the merge candidate pages before the drop planner collects
        // them. That planner retains the same dependencies through receipt.
        for (position, _, _) in &planned.drops {
            state
                .entities
                .capture_mobile_dependencies(*position, 1.0)
                .map_err(|error| {
                    let kind = if matches!(
                        error,
                        crate::server::entities::EntityError::SpatialQueryTooBroad
                    ) {
                        ErrorKind::QuotaExceeded
                    } else {
                        ErrorKind::InvalidData
                    };
                    io::Error::new(kind, error)
                })?;
        }
        let drops = crate::server::drops::plan_stack_spawns_with_extra(
            &state.entities,
            &catalog,
            &planned.drops,
            std::mem::take(&mut planned.entity_spawns),
            tick,
            crate::server::drops::unix_ms(),
        )?;
        crate::server::gameplay::combine_entities(
            &state.entities,
            Some(base),
            drops.into_iter().collect(),
        )?
        .expect("reaction removal includes a despawn")
    };
    let entities = crate::server::gameplay::combine_entities(
        &state.entities,
        Some(entities),
        std::mem::take(&mut planned.entity_updates),
    )?
    .expect("reaction removal includes a despawn");
    Ok((planned, entities))
}

/// Assembles one atomic cross-entity item transfer as a single WAL batch.
///
/// The initiating entity declared `transfer` in its tick plan; this trusted layer
/// resolves both snapshots, runs both pure exchange hooks, and stages both
/// payload updates with real `before` preimages in one `Operation::Batch`.
/// The check and the move are the same statement: a recipient `before` that
/// goes stale before the receipt rejects the whole transaction, so items are
/// neither created nor destroyed and a non-fitting transfer never partially
/// applies. The 128 stack cap is enforced on the taken stack and by both
/// type validators during preparation.
///
/// The non-initiating peer's schedule is untouched. The sender never deducts speculatively; it
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
    let Some(captured_peer) = neighbours.iter().find(|view| view.id == transfer.source) else {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "transfer source is outside the declared neighbour view",
        ));
    };
    let other_snapshot = state
        .entities
        .snapshot(transfer.source)
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "transfer source is unknown"))?;
    if other_snapshot.revision != captured_peer.revision {
        return Err(io::Error::new(
            ErrorKind::WouldBlock,
            "transfer peer revision changed",
        ));
    }
    let initiator = snapshot;
    let initiator_base = receiver_base;
    let initiator_anchor = anchor_update;
    let (receiver, snapshot, receiver_base, anchor_update, source_snapshot, source_base) =
        if transfer.push {
            (
                transfer.source,
                &other_snapshot,
                &other_snapshot.private_payload,
                None,
                initiator,
                initiator_base,
            )
        } else {
            (
                receiver,
                snapshot,
                receiver_base,
                anchor_update,
                &other_snapshot,
                &other_snapshot.private_payload,
            )
        };
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
    let (source_exchange, receiver_exchange) = if let Some(route) = transfer.route {
        let from = CellCoord::new(route.from[0], route.from[1], route.from[2]);
        let to = CellCoord::new(route.to[0], route.to[1], route.to[2]);
        let face = std::array::from_fn(|i| route.to[i].saturating_sub(route.from[i]));
        let touches = |s: &EntitySnapshot, c| matches!(&s.location,EntityLocation::Anchored {footprint,..} if footprint.contains(&c));
        if !bloxgloom_host_api::machine::FACES.contains(&face)
            || !touches(source_snapshot, from)
            || !touches(snapshot, to)
        {
            return Err(permission("invalid automation face"));
        }
        let source = source_exchange
            .port(route.source, face)
            .and_then(|p| p.at_slot(route.source_slot))
            .ok_or_else(|| permission("source port or slot unavailable"))?;
        let mut destination = receiver_exchange
            .port(route.destination, face.map(|v| -v))
            .ok_or_else(|| permission("destination port unavailable"))?;
        if let Some(slot) = route.destination_slot {
            destination = destination
                .at_slot(slot)
                .ok_or_else(|| permission("destination slot unavailable"))?;
        }
        (source, destination)
    } else {
        if catalog.machine(source_snapshot.entity_type).is_some()
            || catalog.machine(snapshot.entity_type).is_some()
        {
            return Err(permission("machine transfer requires explicit ports"));
        }
        (source_exchange.clone(), receiver_exchange.clone())
    };
    let selected = if let Some(route) = transfer.route {
        Some(
            source_exchange
                .inventory_slots(&source_snapshot.private_payload)
                .map_err(io::Error::other)?
                .get(route.source_slot as usize)
                .and_then(Clone::clone)
                .ok_or_else(|| {
                    io::Error::new(ErrorKind::WouldBlock, "selected source slot is unavailable")
                })?,
        )
    } else {
        None
    };
    let (sender_after, taken) = source_exchange
        .withdraw(source_base, transfer.item, transfer.count, catalog)
        .map_err(|error| io::Error::new(ErrorKind::InvalidInput, error))?
        .ok_or_else(|| io::Error::new(ErrorKind::WouldBlock, "transfer source has no stock"))?;
    if taken.item != transfer.item
        || taken.count != transfer.count
        || !taken.valid_in(catalog)
        || selected.as_ref().is_some_and(|s| {
            s.item != taken.item || s.components != taken.components || s.count < taken.count
        })
    {
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
        next_tick: (!transfer.push).then_some(Some(next_tick)),
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
    let sender_patch = EntityPatch {
        payload: Some(sender_after),
        next_tick: transfer.push.then_some(Some(next_tick)),
        position: None,
    };
    let sender_prepared = if transfer.push
        && let Some(anchor) = initiator_anchor
    {
        state.entities.prepare_anchor_update(
            source_snapshot.id,
            source_snapshot.revision,
            anchor.clone(),
            sender_patch,
        )
    } else {
        state
            .entities
            .prepare_update(source_snapshot.id, source_snapshot.revision, sender_patch)
    }
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

fn capture_dependencies(state: &State, view: &VoxelView) -> io::Result<EntityDependencies> {
    state
        .entities
        .capture_dependencies(
            view.revisions().iter().map(|(chunk, _)| *chunk),
            MAX_PLAN_NEIGHBOURS + 1, // the policy view excludes its own record
        )
        .map_err(|error| io::Error::new(ErrorKind::QuotaExceeded, error))
}

fn tick_preparation_error(error: crate::server::entities::EntityError) -> io::Error {
    use crate::server::entities::EntityError;
    let kind = match error {
        EntityError::MotionFenced
        | EntityError::StaleRevision { .. }
        | EntityError::StaleMotionRevision { .. } => ErrorKind::WouldBlock,
        _ => ErrorKind::InvalidData,
    };
    io::Error::new(kind, error)
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
