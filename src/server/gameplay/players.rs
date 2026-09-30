//! Player-service decisions share the gameplay overlay and captured dependencies.
use super::*;
use bloxgloom_host_api::players::{Decision, Event, Registration, State};
#[allow(
    clippy::too_many_arguments,
    reason = "One borrowed capture mirrors the existing gameplay planner without retaining live state."
)]
pub(in crate::server) fn invoke(
    world: &mut World,
    reads: &mut TerrainReads,
    requested: &mut Vec<ChunkKey>,
    participants: Participants<'_>,
    tick: u64,
    seed: u64,
    registration: &Registration,
    event: &Event,
    state: &State,
    session_data: &[u8],
) -> io::Result<(Decision, Option<crate::inventory::Inventory>)> {
    let actor = participants.actor;
    let catalog = world.catalog_arc();
    let mut snapshot = WorldSnapshot {
        players: participants.players,
        action_id: None,
        clock: participants.clock,
        world,
        reads,
        requested,
        actor,
        actor_position: participants.actor_position,
        admin: false,
        inventory_read: false,
        entities: Some(participants.entities),
        tick,
        seed,
        origins: participants
            .actor_position
            .map(|p| {
                vec![[
                    p[0].floor() as i32,
                    p[1].floor() as i32,
                    p[2].floor() as i32,
                ]]
            })
            .unwrap_or_default(),
    };
    let mut context = Context::new(&mut snapshot, 4096);
    let decision = context
        .dispatch_player(registration, event, state, session_data)
        .map_err(error)?;
    let mut plan = context.finish().map_err(error)?;
    // Lifecycle state/rewards are one transaction. World/entity mutation remains
    // in ordinary gameplay callbacks until its complete expansion is supported.
    if plan.world_time.is_some()
        || !plan.blocks.is_empty()
        || !plan.drops.is_empty()
        || !plan.entity_spawns.is_empty()
        || !plan.entity_changes.is_empty()
        || !plan.entity_schedules.is_empty()
        || !plan.admin_spawns.is_empty()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "player lifecycle callbacks cannot stage world/entity writes",
        ));
    }
    let inventory = if let Some((profile, before)) = actor {
        plan.inventories
            .remove(&bloxgloom_host_api::gameplay::InventoryId::Player(profile))
            .map(|slots| inventory::apply(&catalog, before, slots).map_err(error))
            .transpose()?
    } else {
        None
    };
    if !plan.inventories.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "uncaptured lifecycle inventory",
        ));
    }
    Ok((decision, inventory))
}
