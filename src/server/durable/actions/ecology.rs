//! Recheck natural changes and prepare the usual terrain/drop/lifecycle record.
use super::*;
use crate::server::{
    durable::TerrainReads,
    ecology::{Rule, rules},
    effects::CellCoord,
};

pub(super) fn plan(
    state: &mut State,
    cell: [i32; 3],
    rule: Rule,
    tick: TickId,
) -> io::Result<Option<CommitAction>> {
    let mut reads = TerrainReads::default();
    let mut requested = Vec::new();
    let clock = state.world_time.capture();
    let eligible = rules::check(
        &mut state.world,
        &mut reads,
        &mut requested,
        cell,
        rules::daylight(clock.time.elapsed_ms),
    );
    for key in requested.drain(..) {
        let _ = request_chunk(state, key)?;
    }
    if eligible? != Some(rule) {
        return Ok(None);
    }
    if rule == Rule::GrassGrowth {
        reads.clock = Some(clock.stamp);
    }
    let (before, after) = rule.transition();
    let edits = [(cell[0], cell[1], cell[2], after)];
    let cause = if rule == Rule::LeafDecay {
        RemovalCause::SupportLoss
    } else {
        RemovalCause::Transformation
    };
    let removals = [(before, cell, cause)];
    let planned = crate::server::gameplay::plan_with_lifecycles(
        &mut state.world,
        &mut reads,
        &mut requested,
        crate::server::gameplay::OperationInput {
            edits: &edits,
            removals: &removals,
            seed: state.seed,
            tick: tick.get(),
            action: None,
        },
        crate::server::gameplay::Participants {
            actor_inventory_revision: None,
            profile_inventories: None,
            profile_services: None,
            players: &[],
            action_id: None,
            clock: None,
            weather: Some(state.weather.capture()),
            actor: None,
            actor_position: None,
            admin: false,
            entities: &state.entities,
        },
        Some(&state.lifecycles),
    );
    for key in requested {
        let _ = request_chunk(state, key)?;
    }
    let planned = planned?;
    ensure_no_unhandled_anchor(state, &planned.edits)?;
    let catalog = state.world.catalog_arc();
    for &(x, y, z, block) in &planned.edits {
        // Converting one solid soil block to another preserves its collider.
        if catalog.block_flags(block) & crate::content::SOLID != 0
            && state
                .world
                .cached_block(x, y, z)
                .is_none_or(|old| catalog.block_flags(old) & crate::content::SOLID == 0)
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
                "ecology effect overlaps a player",
            ));
        }
    }
    let entities = crate::server::drops::plan_stack_spawns_with_extra(
        &state.entities,
        &catalog,
        &planned.drops,
        planned.entity_spawns,
        tick.get(),
        crate::server::drops::unix_ms(),
    )?;
    let entities = crate::server::gameplay::combine_entities(
        &state.entities,
        entities,
        planned.entity_updates,
    )?;
    Ok(Some(CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        terrain_reads: reads,
        inventory_before: None,
        inventory: None,
        deltas: prepared_deltas(&planned.edits, &planned.prepared),
        changed_cells: planned
            .edits
            .iter()
            .map(|&(x, y, z, _)| CellCoord::new(x, y, z))
            .collect(),
        world_edits: planned.prepared,
        pickups: Vec::new(),
        fire_seed: None,
        clock_change: None,
        weather_change: None,
        entities,
        entity_wakes: Vec::new(),
        owner_changes: Vec::new(),
        sounds: planned.sounds,
        player_publication: None,
    }))
}
