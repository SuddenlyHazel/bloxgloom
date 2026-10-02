//! Scheduled general entities use the existing persisted due index and the
//! shared gameplay planner, not a second timer, WAL or native entity archetype.
use super::*;
use crate::server::{
    durable::TerrainReads,
    effects::CellCoord,
    entities::{EntityId, EntityLocation},
};
use bloxgloom_host_api::gameplay::Event;

pub(in crate::server) fn is_registered(state: &State, id: EntityId) -> bool {
    state.entities.snapshot(id).is_some_and(|snapshot| {
        state
            .world
            .catalog()
            .entity_type(snapshot.entity_type)
            .is_some_and(|definition| {
                state
                    .world
                    .catalog()
                    .gameplay_entity(&definition.key)
                    .is_some()
            })
    })
}

pub(in crate::server) fn plan(
    state: &mut State,
    id: EntityId,
    tick: u64,
    woken: bool,
) -> io::Result<Option<CommitAction>> {
    let Some(snapshot) = state.entities.snapshot(id) else {
        return Ok(None);
    };
    // Early wake hints are advisory. Only persisted due times authorize an
    // entity's timer callback, and restart reconstructs these from the index.
    if woken && snapshot.next_tick.is_none_or(|due| due > tick) {
        return Ok(None);
    }
    if snapshot.next_tick.is_none_or(|due| due > tick) {
        return Ok(None);
    }
    let EntityLocation::Mobile { position } = snapshot.location else {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "general scheduled entity is not mobile",
        ));
    };
    plan_event(
        state,
        id,
        tick,
        Event::EntityTick {
            entity: id.get(),
            position,
            tick,
        },
    )
}
pub(in crate::server) fn plan_event(
    state: &mut State,
    id: EntityId,
    tick: u64,
    event: Event,
) -> io::Result<Option<CommitAction>> {
    let mut reads = TerrainReads::default();
    reads.entities(state.entities.capture_entity_dependency(id))?;
    let moving = matches!(
        &event,
        Event::MovingTick { .. } | Event::MovingImpact { .. } | Event::MovingExpiry { .. }
    );
    let players = if moving {
        reads.players(state)?;
        crate::server::players::capture(state)
    } else {
        Vec::new()
    };
    let mut requested = Vec::new();
    let plan = crate::server::gameplay::plan_with_lifecycles(
        &mut state.world,
        &mut reads,
        &mut requested,
        crate::server::gameplay::OperationInput {
            edits: &[],
            removals: &[],
            seed: state.seed,
            tick,
            action: Some(event),
        },
        crate::server::gameplay::Participants {
            actor_inventory_revision: None,
            profile_inventories: moving.then_some(crate::server::gameplay::InventoryCapture {
                clients: &state.clients,
                overlay: &state.durability.inventory_overlay,
                revisions: &state.durability.inventory_revisions,
                cache: &mut state.profile_inventory_cache,
            }),
            profile_services: moving.then_some(&state.system_runtime),
            players: &players,
            action_id: None,
            clock: Some(state.world_time.capture()),
            weather: Some(state.weather.capture()),
            actor: None,
            actor_position: None,
            admin: false,
            entities: &state.entities,
        },
        Some(&state.lifecycles),
    );
    for key in requested {
        let _ = request_chunk(state, key);
    }
    let plan = plan?;
    let catalog = state.world.catalog_arc();
    for &(x, y, z, block) in &plan.edits {
        if catalog.block_flags(block) & crate::content::SOLID != 0
            && state.clients.values().any(|client| {
                block_intersects_player(
                    catalog
                        .player_rules()
                        .for_stance(client.movement.crouching())
                        .body(),
                    [x, y, z],
                    client.position(),
                )
            })
        {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "scheduled block overlaps a player",
            ));
        }
    }
    let entities = crate::server::drops::plan_stack_spawns_with_extra(
        &state.entities,
        &catalog,
        &plan.drops,
        plan.entity_spawns,
        tick,
        crate::server::drops::unix_ms(),
    )?;
    let entities =
        crate::server::gameplay::combine_entities(&state.entities, entities, plan.entity_updates)?;
    let mut profile_changes = crate::server::players::state::prepare_writes(
        &state.system_runtime,
        &catalog,
        state.durability.pending_profile_inserts(),
        plan.profile_states,
    )?;
    profile_changes.extend(plan.profile_inventory_changes);
    let deltas = prepared_deltas(&plan.edits, &plan.prepared);
    Ok(Some(CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        terrain_reads: reads,
        inventory_before: None,
        inventory: None,
        world_edits: plan.prepared,
        deltas,
        changed_cells: plan
            .edits
            .into_iter()
            .map(|(x, y, z, _)| CellCoord::new(x, y, z))
            .collect(),
        pickups: Vec::new(),
        fire_seed: None,
        weather_change: plan
            .weather
            .map(|(kind, ms)| state.weather.prepare(kind, ms))
            .transpose()?,
        clock_change: plan
            .world_time
            .map(|time| state.world_time.prepare(time))
            .transpose()?,
        entities,
        entity_wakes: Vec::new(),
        owner_changes: profile_changes,
        sounds: plan.sounds,
        player_publication: crate::server::players::Published::operations(plan.player_operations),
    }))
}
