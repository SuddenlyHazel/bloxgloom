//! Passive, durable 27-slot storage using the same inventory ports as Hoppers.
use super::*;
use crate::content::{CHEST_ENTITY_TYPE, CHEST_STATE, Catalog};
use crate::protocol::workstation::WorkstationKind;
use std::sync::Arc;

pub(in crate::server) type ChestPayload = super::storage::StoragePayload<27>;
impl ChestPayload {
    pub fn spawn(&self, anchor: CellCoord, tick: u64) -> EntitySpawn {
        EntitySpawn::Anchored {
            entity_type: CHEST_ENTITY_TYPE,
            anchor,
            anchor_state: CHEST_STATE,
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
    super::storage::register::<27>(
        builder,
        catalog,
        CHEST_ENTITY_TYPE,
        CHEST_STATE,
        WorkstationKind::Chest,
        TickPolicy::Never,
    )
}
