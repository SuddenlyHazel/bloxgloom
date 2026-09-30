//! Translate a worker-prepared fire burn into the same retryable block/neighbor
//! decisions and WAL participants as semantic actions. Fire's frontier,
//! cross-owner mailbox and cursor remain in their existing durable records.
use super::*;
use crate::server::{durable::TerrainReads, effects::CellCoord, fire::FireTransaction};

pub(in crate::server) fn plan(
    state: &mut State,
    transaction: &FireTransaction,
    tick: u64,
) -> io::Result<CommitAction> {
    let worker_edit = transaction.world_edit.as_ref().ok_or_else(|| {
        io::Error::new(
            ErrorKind::InvalidData,
            "burn has no worker-prepared world edit",
        )
    })?;
    state
        .world
        .validate_owner_apply_batch(std::slice::from_ref(worker_edit))?;
    let mut edits = Vec::with_capacity(transaction.changed_cells.len());
    let mut removals = Vec::with_capacity(edits.capacity());
    for cell in &transaction.changed_cells {
        let old = state
            .world
            .cached_block(cell.x, cell.y, cell.z)
            .ok_or_else(|| {
                io::Error::new(ErrorKind::WouldBlock, "fire source chunk unavailable")
            })?;
        edits.push((cell.x, cell.y, cell.z, AIR));
        removals.push((old, [cell.x, cell.y, cell.z], RemovalCause::Burn));
    }
    let mut reads = TerrainReads::default();
    let mut requested = Vec::new();
    let planned = crate::server::gameplay::plan_removals(
        &mut state.world,
        &mut reads,
        &mut requested,
        crate::server::gameplay::OperationInput {
            edits: &edits,
            removals: &removals,
            seed: state.seed,
            tick,
            action: None,
        },
        crate::server::gameplay::Participants {
            players: &[],
            action_id: None,
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
    let planned = planned?;
    ensure_no_unhandled_anchor(state, &planned.edits)?;
    let catalog = state.world.catalog_arc();
    for &(x, y, z, block) in &planned.edits {
        if catalog.block_flags(block) & crate::content::SOLID != 0
            && state.clients.values().any(|client| {
                block_intersects_player(catalog.player_rules().body(), [x, y, z], client.position())
            })
        {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "fire effect overlaps a player",
            ));
        }
    }
    let entities = crate::server::drops::plan_stack_spawns_with_extra(
        &state.entities,
        &catalog,
        &planned.drops,
        planned.entity_spawns,
        tick,
        crate::server::drops::unix_ms(),
    )?;
    let entities = crate::server::gameplay::combine_entities(
        &state.entities,
        entities,
        planned.entity_updates,
    )?;
    Ok(CommitAction {
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
        entity_wakes: Vec::new(),
        entities,
    })
}
