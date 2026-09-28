//! Server-owned supported terrain and population checks for creative spawns.

use std::io::{self, ErrorKind};

/// One spawn, at one of twelve nearby supported locations, with a bounded
/// population query. The allocator and destination chunk serialize concurrent
/// spawns; terrain read keys remain fenced through the shared WAL receipt.
#[cfg(test)]
pub(super) fn plan_spawn(
    state: &mut crate::server::State,
    profile: u128,
    position: [f32; 3],
    tick: u64,
    entity_type: crate::content::EntityTypeId,
) -> io::Result<crate::server::entities::PreparedEntityTransaction> {
    let (spawn, keys) = validate_spawn(state, profile, position, tick, entity_type)?;
    let mut prepared = state
        .entities
        .prepare_spawn_batch(vec![spawn])
        .map_err(io::Error::other)?;
    for key in keys {
        prepared.add_read_key(super::super::chunk_state_key(key));
    }
    Ok(prepared)
}

pub(super) fn validate_spawn(
    state: &mut crate::server::State,
    profile: u128,
    position: [f32; 3],
    tick: u64,
    entity_type: crate::content::EntityTypeId,
) -> io::Result<(
    crate::server::entities::EntitySpawn,
    Vec<crate::world::ChunkKey>,
)> {
    use crate::server::entities::{EntityLocation, EntitySpawn, mobile};
    if state.admin_profile != Some(profile) || profile == 0 {
        return Err(io::Error::new(
            ErrorKind::PermissionDenied,
            "admin access denied",
        ));
    }
    let view =
        super::entity::capture_view_for_plan(state, &EntityLocation::Mobile { position }, 1)?;
    let definition = state
        .world
        .catalog()
        .mobile_entity(entity_type)
        .cloned()
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "unregistered creature"))?;
    for [dx, dz] in [[2.0, 0.0], [0.0, 2.0], [-2.0, 0.0], [0.0, -2.0]] {
        for dy in [0.0, 1.0, -1.0] {
            let candidate = [
                position[0].floor() + 0.5 + dx,
                position[1].floor() + dy,
                position[2].floor() + 0.5 + dz,
            ];
            if !mobile::spawn_clear(&view, definition.body, candidate).map_err(io::Error::other)? {
                continue;
            }
            let chunk = crate::server::entities::position_to_cell(candidate)
                .map_err(io::Error::other)?
                .chunk();
            // Fail closed on a crowded page; do not collect the whole population.
            let nearby = state
                .entities
                .public_views_for_chunk_bounded(chunk, 32)
                .map_err(|_| io::Error::new(ErrorKind::QuotaExceeded, "spawn chunk is crowded"))?;
            if nearby
                .iter()
                .filter(|e| e.entity_type == entity_type)
                .count()
                >= 16
            {
                return Err(io::Error::new(
                    ErrorKind::QuotaExceeded,
                    "at most 16 creatures of one type per spawn chunk",
                ));
            }
            let spawn = EntitySpawn::Mobile {
                entity_type,
                position: candidate,
                payload: definition.behavior.initial(),
                spawn_tick: tick,
            };
            return Ok((
                spawn,
                view.revisions().iter().map(|(key, _)| *key).collect(),
            ));
        }
    }
    Err(io::Error::new(
        ErrorKind::InvalidInput,
        "no clear supported spot nearby",
    ))
}
