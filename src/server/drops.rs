//! World drops on the shared entity path.
//!
//! Drops are registered owner-batched mobile entities
//! ([`entity::DROP_ENTITY_TYPE`]) living in the single [`EntityStore`]. There
//! is one allocator (the store's), one spatial index (the store's mobile
//! index), one movement model (the registered [`tick::DropTickPlanner`]
//! running on the sparse due-tick schedule), and one persistence path (entity
//! WAL records plus the BGEN checkpoint). This module holds only the drop
//! payload type, its tick policy, gameplay planning, and live queries.

mod entity;
mod planning;
mod queries;
pub(in crate::server) use queries::{capture_nearby, project_nearby};
mod tick;

#[cfg(test)]
pub(super) use entity::DropEntityPayload;
pub(super) use entity::{DROP_ENTITY_TYPE, register_entity_type};
#[cfg(test)]
pub(in crate::server) use planning::plan_spawn_stack;
pub(in crate::server) use planning::{
    plan_expired, plan_spawns, plan_stack_spawns, plan_stack_spawns_with_extra, plan_take,
};
#[cfg(test)]
pub(in crate::server) use queries::nearby;
pub(in crate::server) use queries::{
    airborne_count, extractable, has_expired, pickup_candidates, pickup_eligible, stack,
};

use std::time::{SystemTime, UNIX_EPOCH};

use crate::server::entities::EntityDelta;

pub(super) const VIEW_RANGE: f32 = 64.0;
pub(super) const VIEW_RANGE_SQ: f32 = VIEW_RANGE * VIEW_RANGE;
#[cfg(test)]
pub(super) const LIFETIME: std::time::Duration = std::time::Duration::from_secs(600);
#[cfg(test)]
pub(super) const DROP_RADIUS: f32 = 0.18;
#[cfg(test)]
pub(super) const GRAVITY: f32 = 24.0;
#[cfg(test)]
pub(super) const TERMINAL_SPEED: f32 = 30.0;

pub(in crate::server) fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

/// Presentation age for one drop: saturates instead of wrapping on clock
/// skew, and never feeds back into ownership decisions.
pub(super) fn age_ms_now(created_unix_ms: u64, now_ms: u64) -> u32 {
    now_ms
        .saturating_sub(created_unix_ms)
        .min(u64::from(u32::MAX)) as u32
}

fn invalid(message: &'static str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message)
}

/// True when a committed entity delta touches a drop record. Publication
/// bumps the client drop stream revision from this instead of a second
/// live map.
pub(super) fn is_drop_delta(delta: &EntityDelta) -> bool {
    match delta {
        EntityDelta::Spawned(view)
        | EntityDelta::Updated { view, .. }
        | EntityDelta::Transferred { view, .. }
        | EntityDelta::Moved(view) => view.entity_type == DROP_ENTITY_TYPE,
        EntityDelta::Despawned { entity_type, .. } => *entity_type == DROP_ENTITY_TYPE,
    }
}

#[cfg(test)]
mod tests;
