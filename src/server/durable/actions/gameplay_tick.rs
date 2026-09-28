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
    let mut reads = TerrainReads::default();
    reads.entities(state.entities.capture_entity_dependency(id))?;
    let mut requested = Vec::new();
    let plan = crate::server::gameplay::plan_removals(
        &mut state.world,
        &mut reads,
        &mut requested,
        crate::server::gameplay::OperationInput {
            edits: &[],
            removals: &[],
            seed: state.seed,
            tick,
            action: Some(Event::EntityTick {
                entity: id.get(),
                position,
                tick,
            }),
        },
        crate::server::gameplay::Participants {
            actor: None,
            actor_position: None,
            admin: false,
            entities: &state.entities,
        },
    );
    for key in requested {
        let _ = request_chunk(state, key);
    }
    let plan = plan?;
    ensure_no_unhandled_anchor(state, &plan.edits)?;
    let catalog = state.world.catalog_arc();
    for &(x, y, z, block) in &plan.edits {
        if catalog.block_flags(block) & crate::content::SOLID != 0
            && state.clients.values().any(|client| {
                block_intersects_player(catalog.player_rules().body(), [x, y, z], client.position())
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
        entities,
        entity_wakes: Vec::new(),
    }))
}
