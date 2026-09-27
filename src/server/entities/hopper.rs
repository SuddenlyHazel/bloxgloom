//! Three-slot gravity-fed inventory machine. Automation uses registered ports.
mod codec;
mod policy;
#[cfg(test)]
mod tests;
use super::transfer::{put, take};
use super::*;
use crate::content::{Catalog, HOPPER_ENTITY_TYPE, HOPPER_STATE};
use crate::inventory::{Inventory, STACK_LIMIT, Stack};
use std::sync::Arc;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::server) struct HopperPayload {
    pub slots: [Option<Stack>; 3],
}

impl HopperPayload {
    pub fn spawn(&self, anchor: CellCoord, tick: u64) -> EntitySpawn {
        EntitySpawn::Anchored {
            entity_type: HOPPER_ENTITY_TYPE,
            anchor,
            anchor_state: HOPPER_STATE,
            footprint: vec![anchor],
            payload: EntityPayload::new(self.clone()),
            spawn_tick: tick,
        }
    }
}

pub(in crate::server) fn register(
    builder: &mut EntityTypeRegistryBuilder<'_>,
    catalog: &Arc<Catalog>,
) -> Result<(), EntityError> {
    builder.register(EntityTypeRegistration {
        id: HOPPER_ENTITY_TYPE,
        ownership: EntityOwnership::anchored(vec![HOPPER_STATE], 1),
        tick_policy: TickPolicy::Interval(20),
        max_payload_bytes: 4096,
        codec: Arc::new(codec::Codec {
            catalog: catalog.clone(),
        }),
    })?;
    builder.register_transfer_policy(HOPPER_ENTITY_TYPE, Arc::new(policy::Port))?;
    builder.register_interaction_policy(HOPPER_ENTITY_TYPE, Arc::new(policy::Planner))?;
    builder.register_tick_planner(HOPPER_ENTITY_TYPE, Arc::new(policy::Planner))
}
