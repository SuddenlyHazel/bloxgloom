//! Bounded committed-pose history and complete dynamic collider capture.
//! The solver uses previous/current committed poses, never client prediction.
use super::{MAX_DYNAMIC_COLLIDERS, STEP_TICKS, solver};
use crate::server::{
    State,
    durable::TerrainReads,
    entities::{EntityId, EntityLocation},
};
use bloxgloom_host_api::motion::{MovingEntity, Record};
use std::collections::{BTreeMap, VecDeque};
use std::io::{self, ErrorKind};

pub(in crate::server) const MAX_HISTORY_CREATURES: usize = 1024;
#[derive(Clone, Copy)]
struct Pose {
    position: [f32; 3],
}
#[derive(Default)]
pub(in crate::server) struct History {
    frames: VecDeque<(u64, BTreeMap<EntityId, Pose>)>,
    overloaded: bool,
}
impl History {
    fn replace(&mut self, tick: u64, poses: BTreeMap<EntityId, Pose>) {
        if self.frames.back().is_some_and(|frame| frame.0 == tick) {
            return;
        }
        self.frames.push_back((tick, poses));
        while self.frames.len() > 3 {
            self.frames.pop_front();
        }
        self.overloaded = false;
    }
    fn start(&self, id: EntityId, tick: u64, current: [f32; 3]) -> [f32; 3] {
        let start_tick = tick.saturating_sub(STEP_TICKS);
        self.frames
            .iter()
            .find(|frame| frame.0 == start_tick)
            .and_then(|frame| frame.1.get(&id))
            .map_or(current, |pose| pose.position)
    }
}
pub(in crate::server) fn sample(state: &mut State, tick: u64) {
    if state
        .motion_history
        .frames
        .back()
        .is_some_and(|frame| frame.0 == tick)
    {
        return;
    }
    let catalog = state.world.catalog();
    let (ids, overloaded) = match state.entities.mobile_ids_of_types(
        catalog.mobile_entities().map(|(id, _)| id),
        MAX_HISTORY_CREATURES,
    ) {
        Ok(ids) => (ids, false),
        Err(_) => (vec![], true),
    };
    let mut poses = BTreeMap::new();
    for id in ids {
        if let Some(snapshot) = state.entities.snapshot(id)
            && let EntityLocation::Mobile { position } = snapshot.location
        {
            poses.insert(id, Pose { position });
        }
    }
    for (&session, client) in &state.clients {
        if let Some(id) = EntityId::for_player_session(session) {
            poses.insert(
                id,
                Pose {
                    position: client.position(),
                },
            );
        }
    }
    state.motion_history.replace(tick, poses);
    state.motion_history.overloaded = overloaded;
}
fn collider(
    target: solver::Target,
    start: [f32; 3],
    end: [f32; 3],
    half_width: f32,
    height: f32,
) -> solver::Collider {
    solver::Collider {
        target,
        min: [
            f64::from(start[0] - half_width),
            f64::from(start[1]),
            f64::from(start[2] - half_width),
        ],
        max: [
            f64::from(start[0] + half_width),
            f64::from(start[1] + height),
            f64::from(start[2] + half_width),
        ],
        displacement: std::array::from_fn(|axis| f64::from(end[axis]) - f64::from(start[axis])),
    }
}
fn intersects(c: &solver::Collider, min: [i32; 3], max: [i32; 3]) -> bool {
    (0..3).all(|axis| {
        c.min[axis].min(c.min[axis] + c.displacement[axis]) <= f64::from(max[axis]) + 1.0
            && c.max[axis].max(c.max[axis] + c.displacement[axis]) >= f64::from(min[axis])
    })
}
fn push(result: &mut Vec<solver::Collider>, value: solver::Collider) -> io::Result<()> {
    if result.len() >= MAX_DYNAMIC_COLLIDERS {
        return Err(io::Error::new(
            ErrorKind::QuotaExceeded,
            "moving dynamic collider capacity",
        ));
    }
    result.push(value);
    Ok(())
}
pub(super) fn capture(
    state: &State,
    id: EntityId,
    tick: u64,
    declaration: &MovingEntity,
    record: &Record,
    bounds: ([i32; 3], [i32; 3]),
    reads: &mut TerrainReads,
) -> io::Result<Vec<solver::Collider>> {
    let mut result = Vec::new();
    if !declaration.body.collisions.creatures && !declaration.body.collisions.players {
        return Ok(result);
    }
    if declaration.body.collisions.creatures && state.motion_history.overloaded {
        return Err(io::Error::new(
            ErrorKind::QuotaExceeded,
            "moving committed collider history capacity",
        ));
    }
    let catalog = state.world.catalog();
    if declaration.body.collisions.creatures {
        let ids = state
            .entities
            .mobile_ids_of_types(
                catalog.mobile_entities().map(|(id, _)| id),
                MAX_HISTORY_CREATURES,
            )
            .map_err(io::Error::other)?;
        // Fence candidate absence and records across the complete historical
        // collider population; filtering only final positions misses crossings.
        for target_id in ids {
            // A previously distant member can change pose into a crossing
            // before admission. Fence every enumerated record, not only hits.
            reads.entities(state.entities.capture_entity_dependency(target_id))?;
            if target_id == id || record.source_ticks > 0 && record.source == Some(target_id.get())
            {
                continue;
            }
            let Some(target) = state.entities.snapshot(target_id) else {
                continue;
            };
            let EntityLocation::Mobile { position } = target.location else {
                continue;
            };
            let Some(definition) = catalog.mobile_entity(target.entity_type) else {
                continue;
            };
            let start = state.motion_history.start(target_id, tick, position);
            let c = collider(
                solver::Target::Creature {
                    id: target_id.get(),
                    revision: target.revision,
                },
                start,
                position,
                definition.body.half_width,
                definition.body.height,
            );
            if intersects(&c, bounds.0, bounds.1) {
                push(&mut result, c)?;
            }
        }
        let min = bounds.0.map(|x| x as f32 - 4.0);
        let max = bounds.1.map(|x| x as f32 + 4.0);
        let center = std::array::from_fn(|i| (min[i] + max[i]) * 0.5);
        let radius = (0..3).map(|i| (max[i] - min[i]) * 0.5).fold(0.0, f32::max);
        reads.entities(
            state
                .entities
                .capture_mobile_dependencies(center, radius)
                .map_err(io::Error::other)?,
        )?;
    }
    if declaration.body.collisions.players {
        reads.players(state)?;
        for (&session, client) in &state.clients {
            let Some(target_id) = EntityId::for_player_session(session) else {
                continue;
            };
            if record.source_ticks > 0 && record.source == Some(target_id.get()) {
                continue;
            }
            let position = client.position();
            let start = state.motion_history.start(target_id, tick, position);
            let body = catalog
                .player_rules()
                .for_stance(client.movement.crouching())
                .body();
            let c = collider(
                solver::Target::Player {
                    id: target_id.get(),
                    revision: client.movement.last_seq(),
                },
                start,
                position,
                body.half_width,
                body.head_height,
            );
            if intersects(&c, bounds.0, bounds.1) {
                push(&mut result, c)?;
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
