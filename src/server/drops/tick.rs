//! The registered drop tick policy invokes the public falling-entity decision
//! over the worker's captured voxel view. Its output still goes through the
//! ordinary revision-checked entity WAL planner, never a separate drop engine.

use super::entity::DropEntityPayload;
use super::{DROP_RADIUS, GRAVITY, TERMINAL_SPEED};
use crate::content::Catalog;
use crate::server::entities::{
    EntityError, EntityLocation, EntitySnapshot, EntityTickPlan, EntityTickPolicy, EntityView,
};
use crate::server::simulation::FIXED_STEP;
use crate::server::voxel_view::VoxelView;
use bloxgloom_host_api::entity::{self as api, FallingWorld};

pub(in crate::server) struct DropTickPlanner;

struct CapturedColumn<'a> {
    view: &'a VoxelView,
    catalog: &'a Catalog,
}

impl FallingWorld for CapturedColumn<'_> {
    fn solid(&self, [x, y, z]: [i32; 3]) -> Result<bool, api::Error> {
        self.view
            .block(x, y, z)
            .map(|block| self.catalog.block_flags(block) & crate::content::SOLID != 0)
            .map_err(|_| api::Error::OutOfRange)
    }
}

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
        let world = CapturedColumn { view, catalog };
        let motion = api::FallingContext {
            position,
            vertical_speed: payload.vertical_speed,
            suspended: snapshot.next_tick.is_none(),
            tick: current_tick,
            step_seconds: FIXED_STEP.as_secs_f32(),
            gravity: GRAVITY,
            terminal_speed: TERMINAL_SPEED,
            radius: DROP_RADIUS,
            world: &world,
        }
        .plan()
        .map_err(|error| match error {
            api::Error::OutOfRange => EntityError::ViewOutOfRange,
            api::Error::Exhausted => EntityError::RevisionExhausted,
            api::Error::InvalidState => EntityError::InvalidPayload,
        })?;
        let next_payload = motion.vertical_speed.map(|speed| {
            payload
                .clone()
                .with_vertical_speed(speed)
                .into_entity_payload()
        });
        Ok(EntityTickPlan {
            lifecycle: Default::default(),
            payload: next_payload,
            next_tick: motion.next_tick,
            anchor_update: None,
            position: motion.position,
            block_states: Vec::new(),
            wakes: Vec::new(),
            transfer: None,
        })
    }

    fn read_radius_chunks(&self) -> u8 {
        1
    }
}

#[cfg(test)]
#[path = "tick/tests.rs"]
mod tests;
