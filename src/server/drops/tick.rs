//! Deterministic drop physics as a registered entity tick policy.
//!
//! The planner is a pure function of the entity snapshot and its captured
//! voxel view: fixed-step gravity integration over the column below the drop,
//! with no wall clock, RNG, or I/O. The trusted durable layer stages the
//! returned motion as one WAL record (same-owner update or fenced barrier
//! transfer), so crash recovery resumes from the last staged tick.

use super::entity::DropEntityPayload;
use super::{DROP_RADIUS, GRAVITY, TERMINAL_SPEED};
use crate::content::Catalog;
use crate::server::entities::{
    EntityError, EntityLocation, EntitySnapshot, EntityTickPlan, EntityTickPolicy, EntityView,
};
use crate::server::simulation::FIXED_STEP;
use crate::server::voxel_view::VoxelView;
use crate::world::{ChunkKey, world_to_chunk};

pub(in crate::server) struct DropTickPlanner;

impl EntityTickPolicy for DropTickPlanner {
    fn reads_neighbours(&self) -> bool {
        false
    }

    fn plan(
        &self,
        snapshot: &EntitySnapshot,
        current_tick: u64,
        catalog: &Catalog,
        view: &VoxelView,
        _neighbours: &EntityView,
    ) -> Result<EntityTickPlan, EntityError> {
        let EntityLocation::Mobile { position } = snapshot.location else {
            return Err(EntityError::WrongOwnership);
        };
        let payload = snapshot
            .private_payload
            .downcast_ref::<DropEntityPayload>()
            .ok_or(EntityError::InvalidPayload)?;
        // Suspension was committed at an integer block top + DROP_RADIUS.
        // Check that supporting layer, not the active sweep's inclusive start
        // layer: a drop spawned inside a column must not climb one voxel on
        // each harmless recheck. Only absent support resumes active physics.
        if snapshot.next_tick.is_none() {
            let support = position[1].floor() - 1.0;
            match first_solid_top(view, catalog, position, support, support, &mut Vec::new())? {
                TerrainCheck::Missing => return Err(EntityError::ViewOutOfRange),
                TerrainCheck::Hit(_) => {
                    return Ok(EntityTickPlan {
                        payload: None,
                        next_tick: None,
                        anchor_update: None,
                        position: None,
                        block_states: Vec::new(),
                        wakes: Vec::new(),
                        transfer: None,
                    });
                }
                TerrainCheck::Clear => {}
            }
        }
        // Fixed-step integration, matching the historic coordinator step:
        // every scheduled tick advances exactly one step, so the trajectory
        // is a pure function of the staged state, never of scheduling order.
        let dt = FIXED_STEP.as_secs_f32().min(0.1);
        let speed = (payload.vertical_speed - GRAVITY * dt).max(-TERMINAL_SPEED);
        let start = position[1] - DROP_RADIUS;
        let end = start + speed * dt;
        let mut missing = Vec::new();
        match first_solid_top(view, catalog, position, start, end, &mut missing) {
            Ok(TerrainCheck::Missing) => Err(EntityError::ViewOutOfRange),
            Ok(TerrainCheck::Hit(top)) => {
                let rest = top + DROP_RADIUS;
                let position_changed = position[1].to_bits() != rest.to_bits();
                let speed_changed = payload.vertical_speed.to_bits() != 0.0_f32.to_bits();
                let mut after = payload.clone();
                after.vertical_speed = 0.0;
                Ok(EntityTickPlan {
                    // A settled tick persists rest speed and suspends the
                    // schedule. A woken settled drop plans all-`None` here,
                    // which reaffirms without staging a WAL record.
                    payload: speed_changed.then(|| after.into_entity_payload()),
                    next_tick: None,
                    anchor_update: None,
                    position: position_changed.then_some([position[0], rest, position[2]]),
                    block_states: Vec::new(),
                    wakes: Vec::new(),
                    transfer: None,
                })
            }
            Ok(TerrainCheck::Clear) => {
                let next = current_tick
                    .checked_add(1)
                    .ok_or(EntityError::RevisionExhausted)?;
                let mut after = payload.clone();
                after.vertical_speed = speed;
                Ok(EntityTickPlan {
                    payload: (after.vertical_speed.to_bits() != payload.vertical_speed.to_bits())
                        .then(|| after.into_entity_payload()),
                    next_tick: Some(next),
                    anchor_update: None,
                    position: Some([position[0], end + DROP_RADIUS, position[2]]),
                    block_states: Vec::new(),
                    wakes: Vec::new(),
                    transfer: None,
                })
            }
            Err(error) => Err(error),
        }
    }

    /// The physics samples the drop's footprint corners, which can straddle
    /// a chunk seam, plus the column traversed this tick. Capturing the full
    /// neighbourhood keeps every sample inside the declared view; a chunk
    /// that has not loaded yet defers the tick instead of failing it.
    fn read_radius_chunks(&self) -> u8 {
        1
    }
}

enum TerrainCheck {
    Hit(f32),
    Clear,
    Missing,
}

const MAX_MISSING_CHUNKS_PER_STEP: usize = 64;

fn first_solid_top(
    view: &VoxelView,
    catalog: &Catalog,
    position: [f32; 3],
    start_bottom: f32,
    end_bottom: f32,
    missing_chunks: &mut Vec<ChunkKey>,
) -> Result<TerrainCheck, EntityError> {
    let mut hit: Option<f32> = None;
    let bottom = end_bottom.floor() as i32;
    let top = start_bottom.floor() as i32;
    for y in (bottom..=top).rev() {
        let block_top = y as f32 + 1.0;
        if block_top < end_bottom {
            continue;
        }
        for x in [position[0] - DROP_RADIUS, position[0] + DROP_RADIUS] {
            for z in [position[2] - DROP_RADIUS, position[2] + DROP_RADIUS] {
                let x = x.floor() as i32;
                let z = z.floor() as i32;
                match view.block(x, y, z) {
                    Ok(block) if catalog.block_flags(block) & crate::content::SOLID != 0 => {
                        hit = Some(hit.map_or(block_top, |previous| previous.max(block_top)));
                    }
                    Ok(_) => {}
                    Err(_) => {
                        let key = world_to_chunk(x, y, z).0;
                        if missing_chunks.len() < MAX_MISSING_CHUNKS_PER_STEP
                            && !missing_chunks.contains(&key)
                        {
                            missing_chunks.push(key);
                        }
                        return Ok(TerrainCheck::Missing);
                    }
                }
            }
        }
    }
    Ok(hit.map_or(TerrainCheck::Clear, TerrainCheck::Hit))
}

#[cfg(test)]
#[path = "tick/tests.rs"]
mod tests;
