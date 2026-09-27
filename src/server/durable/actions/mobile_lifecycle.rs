//! Validate declared mobile spawns against the worker's fenced terrain, then
//! combine them with the parent's update/removal in one allocator/WAL batch.
use crate::server::{State, entities::*, voxel_view::VoxelView};
use std::io;
pub(super) fn spawn_effects(
    state: &State,
    parent: &EntitySnapshot,
    view: &VoxelView,
    requests: Vec<bloxgloom_host_api::entity::Spawn>,
    tick: u64,
    base: PreparedEntityTransaction,
) -> io::Result<PreparedEntityTransaction> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidInput, "invalid mobile spawn effect");
    let catalog = state.world.catalog();
    let EntityLocation::Mobile { position: origin } = parent.location else {
        return Err(invalid());
    };
    if catalog.mobile_entity(parent.entity_type).is_none() || requests.len() > 4 {
        return Err(invalid());
    }
    let mut spawns = Vec::new();
    for request in requests {
        let id = catalog
            .entity_type_id_by_key(&request.key)
            .ok_or_else(invalid)?;
        let definition = catalog.mobile_entity(id).ok_or_else(invalid)?;
        if request.position.iter().any(|v| !v.is_finite())
            || (0..3).any(|i| (request.position[i] - origin[i]).abs() > 8.0)
            || !mobile::spawn_clear(view, definition.body, request.position)
                .map_err(|_| invalid())?
        {
            return Err(invalid());
        }
        spawns.push(EntitySpawn::Mobile {
            entity_type: id,
            position: request.position,
            payload: request.state,
            spawn_tick: tick,
        });
    }
    let spawned = state
        .entities
        .prepare_spawn_batch(spawns)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    state
        .entities
        .combine_prepared(vec![base, spawned])
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))
}
