//! Stock automatic pickup is a registered decision using the exact inventory
//! operations available to other gameplay handlers. Selection is server-owned;
//! the handler may alter or reject a candidate, never invent its eligibility.
use super::*;
use crate::server::{durable::TerrainReads, effects::CellCoord};
use bloxgloom_host_api::gameplay::Event;

pub(super) fn plan(
    state: &mut State,
    client_id: u64,
    tick: u64,
) -> io::Result<Option<CommitAction>> {
    let Some(client) = state.clients.get(&client_id) else {
        return Ok(None);
    };
    if state.durability.profile_reserved(client.profile) {
        return Err(io::Error::new(
            ErrorKind::WouldBlock,
            "profile has a pending durable action",
        ));
    }
    let profile = client.profile;
    let original = client.inventory.clone();
    let position = client.position();
    let mut candidates =
        crate::server::drops::pickup_candidates(&state.entities, state.world.catalog(), position);
    if candidates.is_empty() {
        return Ok(None);
    }
    // Bound public handler work and per-drop prepared mutations independently
    // of spatial query density. Walk the stable ID-ordered candidate ring on
    // successive ticks: permanently uncollectable low IDs cannot starve an
    // otherwise collectable stack later in the query.
    let start = (tick % candidates.len() as u64) as usize;
    candidates.rotate_left(start);
    candidates.truncate(32);
    let mut reads = TerrainReads::default();
    reads.entities(
        state
            .entities
            .capture_mobile_dependencies(position, state.world.catalog().max_drop_pickup_range())
            .map_err(|error| io::Error::new(ErrorKind::WouldBlock, error))?,
    )?;
    let event = Event::PickupRequested {
        position,
        drops: candidates
            .iter()
            .map(|item| (item.id, item.count))
            .collect(),
    };
    let mut requested = Vec::new();
    let planned = crate::server::gameplay::plan_removals(
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
            profile_inventories: None,
            profile_services: None,
            players: &[],
            action_id: None,
            clock: None,
            actor: Some((profile, &original)),
            actor_position: Some(position),
            admin: false,
            entities: &state.entities,
        },
    );
    for key in requested {
        let _ = request_chunk(state, key);
    }
    let planned = planned?;
    if planned.drop_takes.is_empty() {
        return Ok(None);
    }
    let credited = planned.inventory.as_ref().ok_or_else(|| {
        io::Error::new(
            ErrorKind::PermissionDenied,
            "pickup did not credit the player inventory",
        )
    })?;
    validate_player_credit(&original, credited, &planned.drop_takes, &state.entities)?;
    let mut pickups = Vec::with_capacity(planned.drop_takes.len());
    for &(id, count) in &planned.drop_takes {
        let Some(candidate) = candidates
            .iter()
            .find(|item| item.id == id && count <= item.count)
        else {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "pickup changed an ineligible drop",
            ));
        };
        pickups.push(DroppedItem {
            count,
            ..*candidate
        });
    }
    ensure_no_unhandled_anchor(state, &planned.edits)?;
    let catalog = state.world.catalog_arc();
    for &(x, y, z, block) in &planned.edits {
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
                "pickup effect overlaps a player",
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
    let deltas = prepared_deltas(&planned.edits, &planned.prepared);
    Ok(Some(CommitAction {
        client_id: Some(client_id),
        profile: Some(profile),
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        terrain_reads: reads,
        inventory_before: Some(InventoryStore::encode_snapshot_with_catalog(
            &original, &catalog,
        )?),
        inventory: planned.inventory,
        world_edits: planned.prepared,
        deltas,
        changed_cells: planned
            .edits
            .into_iter()
            .map(|(x, y, z, _)| CellCoord::new(x, y, z))
            .collect(),
        pickups,
        fire_seed: None,
        clock_change: None,
        entity_wakes: Vec::new(),
        owner_changes: vec![],
        player_publication: None,
        entities,
    }))
}

/// Pickup flight is a confirmed player credit, not an animation for a drop a
/// custom handler destroyed or routed elsewhere. Other handlers may explicitly
/// create rewards, but every reported pickup must be present in the resulting
/// finite inventory with its exact component identity.
fn validate_player_credit(
    before: &Inventory,
    after: &Inventory,
    takes: &[(u64, u16)],
    entities: &crate::server::entities::EntityStore,
) -> io::Result<()> {
    use std::collections::BTreeMap;
    let mut net = BTreeMap::<(u32, u16, Vec<u8>), i32>::new();
    let mut tally = |stack: &crate::inventory::Stack, sign: i32| {
        let key = (
            stack.item.0,
            stack.components.as_ref().map_or(0, |c| c.version),
            stack
                .components
                .as_ref()
                .map_or_else(Vec::new, |c| c.bytes.to_vec()),
        );
        *net.entry(key).or_default() += sign * i32::from(stack.count);
    };
    for stack in before.slots.iter().flatten() {
        tally(stack, -1);
    }
    for stack in after.slots.iter().flatten() {
        tally(stack, 1);
    }
    for &(id, count) in takes {
        let id = crate::server::entities::EntityId::new(id)
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "invalid pickup ID"))?;
        let mut stack = crate::server::drops::stack(entities, id)
            .ok_or_else(|| io::Error::new(ErrorKind::WouldBlock, "pickup drop disappeared"))?;
        stack.count = count;
        tally(&stack, -1);
    }
    if net.values().any(|count| *count < 0) {
        return Err(io::Error::new(
            ErrorKind::PermissionDenied,
            "pickup removed a stack without crediting the player's inventory",
        ));
    }
    Ok(())
}
