//! A small terrain-only wanderer. All decisions are pure worker plans; the
//! shared entity coordinator owns scheduling, read fences, WAL and publication.
//!
//! Locomotion stays due every two logical ticks; idle AI waits 40..=80
//! ticks (0.8..=1.6 seconds at 50 Hz), with support rechecks every ten ticks
//! even if terrain hints are lost or crowded out. This timer never suspends:
//! the bounded suspended-recheck lane is for dependency sleepers, not an AI
//! clock. Missing terrain retains the ordinary durable due entry and retries
//! through the existing fair admission lane; no catch-up steps are simulated.
use super::*;
use crate::content::Catalog;
use crate::server::voxel_view::VoxelView;
use std::sync::Arc;

mod codec;
mod terrain;
use super::locomotion::Body;
use super::navigation::{self, Route};
use codec::Codec;
pub(in crate::server) use terrain::spawn_clear;

// Due work is admitted at InteractionCommit and executes on the following
// tick. A one-tick admission deadline gives two ticks per fixed physics step.
const STEP_TICKS: u64 = 1;
const BODY: Body = Body {
    half_width: 0.36,
    height: 0.94,
    speed: 1.5625,
};

/// Counter-based choices survive restart and do not depend on receipt latency.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(in crate::server) struct Mossbun {
    pub cycle: u64,
    pub facing: u8,
    pub steps: u8,
    pub goal: Option<[i32; 2]>,
    pub waypoint: Option<[i32; 2]>,
    pub vertical_velocity: f32,
    pub grounded: bool,
    pub think_at: u64,
}

pub(in crate::server) fn register(
    builder: &mut EntityTypeRegistryBuilder<'_>,
    catalog: &Catalog,
) -> Result<(), EntityError> {
    let id = catalog
        .entity_type_id_by_key("bloxgloom:mossbun")
        .ok_or(EntityError::InvalidType)?;
    builder.register(EntityTypeRegistration {
        id,
        ownership: EntityOwnership::Mobile,
        tick_policy: TickPolicy::Interval(STEP_TICKS as u32),
        max_payload_bytes: 41,
        codec: Arc::new(Codec),
    })?;
    builder.register_tick_planner(id, Arc::new(Wander))
}

struct Wander;
impl EntityTickPolicy for Wander {
    fn wakes_on_terrain_change(&self) -> bool {
        true
    }

    fn read_radius_chunks(&self) -> u8 {
        1
    }
    fn reads_neighbours(&self) -> bool {
        false
    }

    fn plan(
        &self,
        snapshot: &EntitySnapshot,
        tick: u64,
        _catalog: &Catalog,
        view: &VoxelView,
        _neighbours: &EntityView,
    ) -> Result<EntityTickPlan, EntityError> {
        let mut plan = EntityTickPlan {
            payload: None,
            next_tick: snapshot.next_tick,
            anchor_update: None,
            position: None,
            block_states: Vec::new(),
            wakes: Vec::new(),
            transfer: None,
        };
        let EntityLocation::Mobile { position } = snapshot.location else {
            return Err(EntityError::InvalidLocation);
        };
        if !terrain::valid_position(position) {
            return Err(EntityError::InvalidLocation);
        }
        // A harmless early hint must not advance AI, but loss of support
        // interrupts idle immediately rather than waiting for the AI deadline.
        let clear = BODY.clear(view, position)?;
        let grounded = BODY.grounded(view, position)?;
        let mut bun = *snapshot
            .private_payload
            .downcast_ref::<Mossbun>()
            .ok_or(EntityError::InvalidPayload)?;
        if snapshot.next_tick.is_some_and(|due| due > tick)
            && (!clear || grounded || bun.vertical_velocity < 0.0)
        {
            return Ok(plan);
        }
        let mut delay = STEP_TICKS;
        // Never tunnel out of a newly placed block; wait for it to be removed.
        if !clear {
            bun.steps = 0;
            bun.goal = None;
            bun.waypoint = None;
            delay = 10;
        } else if !grounded {
            // Physics interrupts idle independently of behavior. A fall changes
            // the route's elevation, so choose a fresh destination after landing.
            bun.goal = None;
            bun.waypoint = None;
            bun.steps = 0;
        } else if bun.goal.is_none() && tick < bun.think_at {
            delay = (bun.think_at - tick).min(10);
        } else if bun.goal.is_none() {
            bun.cycle = bun.cycle.wrapping_add(1);
            let choice = mix(snapshot.id.get() ^ mix(bun.cycle));
            bun.goal = Some([
                position[0].floor() as i32 + (choice % 7) as i32 - 3,
                position[2].floor() as i32 + ((choice >> 8) % 7) as i32 - 3,
            ]);
            bun.steps = 16; // bounded route lifetime; unreachable goals back off
        }
        if clear
            && grounded
            && let Some(goal) = bun.goal
        {
            let point = |cell: [i32; 2]| [cell[0] as f32 + 0.5, position[1], cell[1] as f32 + 0.5];
            if let Some(waypoint) = bun.waypoint {
                let target = point(waypoint);
                if glam::Vec3::from_array(position).distance(glam::Vec3::from_array(target)) < 0.02
                {
                    bun.waypoint = None;
                    bun.steps = bun.steps.saturating_sub(1);
                } else if !BODY.walk_edge(view, position, target)? {
                    bun.waypoint = None;
                }
            }
            if bun.waypoint.is_none() {
                match navigation::route(view, BODY, position, goal)? {
                    Route::Next(target) if bun.steps > 0 => {
                        bun.waypoint = Some([target[0].floor() as i32, target[2].floor() as i32]);
                    }
                    _ => {
                        bun.goal = None;
                        bun.steps = 0;
                        bun.think_at = tick
                            .checked_add(40 + mix(bun.cycle ^ snapshot.id.get()) % 41)
                            .ok_or(EntityError::RevisionExhausted)?;
                        delay = 10;
                    }
                }
            }
        }
        let target = bun
            .waypoint
            .map(|cell| [cell[0] as f32 + 0.5, position[1], cell[1] as f32 + 0.5]);
        let movement = BODY.advance(view, position, bun.vertical_velocity, target)?;
        bun.vertical_velocity = movement.vertical_velocity;
        bun.grounded = movement.grounded;
        let next = movement.position;
        let dx = next[0] - position[0];
        let dz = next[2] - position[2];
        if dx.abs() + dz.abs() > 0.0001 {
            bun.facing = if dx.abs() > dz.abs() {
                if dx > 0.0 { 1 } else { 3 }
            } else if dz > 0.0 {
                0
            } else {
                2
            };
        }
        plan.next_tick = Some(
            tick.checked_add(delay)
                .ok_or(EntityError::RevisionExhausted)?,
        );
        plan.payload = Some(EntityPayload::new(bun));
        plan.position = (next != position).then_some(next);
        Ok(plan)
    }
}

fn mix(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests;
