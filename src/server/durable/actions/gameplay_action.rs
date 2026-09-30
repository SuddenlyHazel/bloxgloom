//! Semantic registered use actions resolve an authoritative target, then run
//! their owner through the same staged gameplay transaction as block decisions.
use super::*;
use crate::server::{durable::TerrainReads, effects::CellCoord};
use bloxgloom_host_api::{
    actions::{Request, Target},
    gameplay::Event,
};
use glam::Vec3;
use std::{
    collections::BTreeSet,
    io::{self, ErrorKind},
};

pub(super) struct Invocation<'a> {
    pub client_id: u64,
    pub profile: u128,
    pub action_id: u128,
    pub target: [i32; 3],
    pub request: &'a Request,
    pub terrain_version: Option<u64>,
    pub kind: &'a Target,
    pub receipt_value: Vec<u8>,
    pub tick: TickId,
}

pub(super) fn plan(state: &mut State, invocation: Invocation<'_>) -> io::Result<CommitAction> {
    let Invocation {
        client_id,
        profile,
        action_id,
        target,
        request,
        terrain_version,
        kind,
        receipt_value,
        tick,
    } = invocation;
    let catalog = state.world.catalog_arc();
    let client = state
        .clients
        .get(&client_id)
        .ok_or_else(|| denied("actor disconnected"))?;
    let position = client.position();
    let before = client.inventory.clone();
    let mut reads = TerrainReads::default();
    let mut cell = None;
    let mut entity = None;
    match kind {
        Target::Empty => {
            if request.entity != 0 || request.entity_revision != 0 {
                return Err(denied("untargeted action has an entity"));
            }
        }
        Target::Item(key) => {
            if request.entity != 0
                || request.entity_revision != 0
                || before
                    .slots
                    .get(usize::from(request.slot))
                    .and_then(Option::as_ref)
                    .and_then(|stack| catalog.item(stack.item))
                    .is_none_or(|item| item.key != *key)
            {
                return Err(denied("item use needs the selected matching stack"));
            }
        }
        Target::Block(key) => {
            if request.entity != 0 || request.entity_revision != 0 {
                return Err(denied("block use has an entity"));
            }
            verify_reach(client, target, catalog.player_rules())?;
            let actual = read_target(state, &mut reads, target)?;
            // read_target captured the authority stamp carried through admission
            // and confirmed apply. Compare the client's persisted chunk version
            // on that same resident basis; retries never refresh the observation.
            let observed =
                terrain_version.ok_or_else(|| denied("block use needs terrain fence"))?;
            let chunk_key = world_to_chunk(target[0], target[1], target[2]).0;
            if state.world.cached_version(chunk_key) != Some(observed) {
                return Err(denied("block action target changed since observation"));
            }
            if catalog
                .state(actual)
                .and_then(|s| catalog.block_type(s.block_type))
                .is_none_or(|block| block.key != *key)
            {
                return Err(denied("block action target changed"));
            }
            sight(state, &mut reads, position, target, false)?;
            cell = Some(target);
        }
        Target::Entity(key) => {
            verify_reach(client, target, catalog.player_rules())?;
            let id = crate::server::entities::EntityId::new(request.entity)
                .ok_or_else(|| denied("missing action entity"))?;
            reads.entities(state.entities.capture_entity_dependency(id))?;
            let snapshot = state
                .entities
                .snapshot(id)
                .ok_or_else(|| denied("action entity gone"))?;
            if snapshot.revision != request.entity_revision
                || catalog
                    .entity_type(snapshot.entity_type)
                    .is_none_or(|definition| definition.key != *key)
            {
                return Err(denied("stale or incompatible action entity"));
            }
            match &snapshot.location {
                crate::server::entities::EntityLocation::Mobile { position } => {
                    if crate::server::entities::position_to_cell(*position)
                        .ok()
                        .is_none_or(|at| [at.x, at.y, at.z] != target)
                    {
                        return Err(denied("entity moved"));
                    }
                }
                crate::server::entities::EntityLocation::Anchored { footprint, .. } => {
                    if !footprint.contains(&crate::server::entities::CellCoord::new(
                        target[0], target[1], target[2],
                    )) || state
                        .entities
                        .anchored_at(crate::server::entities::CellCoord::new(
                            target[0], target[1], target[2],
                        ))
                        != Some(id)
                    {
                        return Err(denied("anchor footprint changed"));
                    }
                }
            }
            sight(
                state,
                &mut reads,
                position,
                target,
                matches!(
                    snapshot.location,
                    crate::server::entities::EntityLocation::Mobile { .. }
                ),
            )?;
            cell = Some(target);
            entity = Some(id.get());
        }
    }
    let event = Event::ActionRequested {
        action: request.key.clone(),
        position,
        cell,
        entity,
        slot: request.slot,
        arguments: request.arguments.clone(),
    };
    let mut requested = Vec::new();
    let players = crate::server::players::capture(state);
    let plan = crate::server::gameplay::plan_with_lifecycles(
        &mut state.world,
        &mut reads,
        &mut requested,
        crate::server::gameplay::OperationInput {
            edits: &[],
            removals: &[],
            seed: state.seed,
            tick: tick.get(),
            action: Some(event),
        },
        crate::server::gameplay::Participants {
            players: &players,
            action_id: Some(action_id),
            clock: Some(state.world_time.capture()),
            actor: Some((profile, &before)),
            actor_position: Some(position),
            admin: state.admin_profile == Some(profile),
            entities: &state.entities,
        },
        Some(&state.lifecycles),
    );
    for key in requested {
        let _ = request_chunk(state, key);
    }
    let mut plan = plan?;
    if !plan.admin_spawns.is_empty() && !plan.edits.is_empty() {
        // Supported ground is read from authoritative terrain. Do not validate
        // against a pre-edit world and then commit a conflicting terrain edit.
        return Err(denied("admin spawn cannot share a terrain edit"));
    }
    let admin_spawns = plan
        .admin_spawns
        .iter()
        .map(|key| {
            let entity_type = catalog
                .entity_type_id_by_key(key)
                .ok_or_else(|| denied("unknown creature"))?;
            admin::validate_spawn(state, profile, position, tick.get(), entity_type)
        })
        .collect::<io::Result<Vec<_>>>()?;
    let mut spawn_read_keys = Vec::new();
    for (spawn, keys) in admin_spawns {
        plan.entity_spawns.push(spawn);
        spawn_read_keys.extend(keys);
    }
    for &(x, y, z, block) in &plan.edits {
        // Expanding one touched cell cannot bypass player reach for the rest
        // of a footprint. Ordinary non-anchor effects keep their existing
        // gameplay-radius contract. Expansion already fenced these occupants.
        if state
            .entities
            .anchored_at(crate::server::entities::CellCoord::new(x, y, z))
            .is_some()
        {
            verify_reach(
                &state.clients[&client_id],
                [x, y, z],
                catalog.player_rules(),
            )?;
        }
        if catalog.block_flags(block) & crate::content::SOLID != 0
            && state.clients.values().any(|client| {
                block_intersects_player(catalog.player_rules().body(), [x, y, z], client.position())
            })
        {
            return Err(denied("gameplay block overlaps a player"));
        }
    }
    let mut entities = crate::server::drops::plan_stack_spawns_with_extra(
        &state.entities,
        &catalog,
        &plan.drops,
        plan.entity_spawns,
        tick.get(),
        crate::server::drops::unix_ms(),
    )?;
    if let Some(prepared) = &mut entities {
        for key in spawn_read_keys {
            prepared.add_read_key(super::super::chunk_state_key(key));
        }
    }
    let entities =
        crate::server::gameplay::combine_entities(&state.entities, entities, plan.entity_updates)?;
    let deltas = prepared_deltas(&plan.edits, &plan.prepared);
    Ok(CommitAction {
        client_id: Some(client_id),
        profile: Some(profile),
        action_id: Some(action_id),
        receipt_value: Some(receipt_value),
        receipt_transition: None,
        terrain_reads: reads,
        inventory_before: plan
            .inventory
            .as_ref()
            .map(|_| InventoryStore::encode_snapshot_with_catalog(&before, &catalog))
            .transpose()?,
        inventory: plan.inventory,
        world_edits: plan.prepared,
        deltas,
        changed_cells: plan
            .edits
            .into_iter()
            .map(|(x, y, z, _)| CellCoord::new(x, y, z))
            .collect(),
        pickups: vec![],
        fire_seed: None,
        clock_change: plan
            .world_time
            .map(|time| state.world_time.prepare(time))
            .transpose()?,
        entities,
        entity_wakes: vec![],
        owner_changes: vec![],
        player_publication: crate::server::players::Published::operations(plan.player_operations),
    })
}

fn verify_reach(
    client: &crate::server::Client,
    target: [i32; 3],
    rules: bloxgloom_host_api::player::PlayerRules,
) -> io::Result<()> {
    if target[1] <= BEDROCK_Y
        || !client.interested(world_to_chunk(target[0], target[1], target[2]).0)
    {
        return Err(denied("target outside world or interest"));
    }
    let eye = Vec3::from_array(client.position()) + Vec3::Y * rules.eye_height();
    let center = Vec3::from_array(target.map(|n| n as f32 + 0.5));
    if !eye.is_finite() || (center - eye).length() > EDIT_REACH {
        return Err(denied("action target out of reach"));
    }
    Ok(())
}

fn read_target(
    state: &mut State,
    reads: &mut TerrainReads,
    target: [i32; 3],
) -> io::Result<BlockId> {
    let [x, y, z] = target;
    if let Some(block) = reads.read(&mut state.world, x, y, z)? {
        return Ok(block);
    }
    let _ = request_chunk(state, world_to_chunk(x, y, z).0);
    Err(io::Error::new(
        ErrorKind::WouldBlock,
        "action target chunk unavailable",
    ))
}

/// Capture each inspected ray cell through the same read stamps as the handler;
/// an unloaded cell on the visible ray defers instead of acting as empty space.
fn sight(
    state: &mut State,
    reads: &mut TerrainReads,
    position: [f32; 3],
    target: [i32; 3],
    mobile: bool,
) -> io::Result<()> {
    let eye =
        Vec3::from_array(position) + Vec3::Y * state.world.catalog().player_rules().eye_height();
    let center = Vec3::from_array(target.map(|n| n as f32 + 0.5));
    let delta = center - eye;
    let mut missing = BTreeSet::new();
    let mut failure = None;
    let catalog = state.world.catalog_arc();
    let hit = crate::raycast::raycast_with_catalog(
        eye,
        delta.normalize_or_zero(),
        delta.length(),
        |x, y, z| match reads.read(&mut state.world, x, y, z) {
            Ok(Some(block)) => Some(block),
            Ok(None) => {
                missing.insert(world_to_chunk(x, y, z).0);
                None
            }
            Err(error) => {
                failure = Some(error);
                None
            }
        },
        &catalog,
    );
    if let Some(error) = failure {
        return Err(error);
    }
    if !missing.is_empty() {
        for key in missing {
            let _ = request_chunk(state, key);
        }
        return Err(io::Error::new(
            ErrorKind::WouldBlock,
            "action sight chunk unavailable",
        ));
    }
    if hit.is_some_and(|hit| mobile || hit.block != target) || (!mobile && hit.is_none()) {
        return Err(denied("action target is occluded"));
    }
    Ok(())
}

fn denied(message: &'static str) -> io::Error {
    io::Error::new(ErrorKind::PermissionDenied, message)
}
