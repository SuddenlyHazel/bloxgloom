//! Three-slot gravity-fed inventory machine. Automation uses registered ports.
#[cfg(test)]
use super::storage::codec;
mod policy;
#[cfg(test)]
mod tests;
use super::*;
use crate::content::{Catalog, HOPPER_ENTITY_TYPE, HOPPER_STATE};
#[cfg(test)]
use crate::inventory::{Inventory, Stack};
use std::sync::Arc;

pub(in crate::server) type HopperPayload = super::storage::StoragePayload<3>;

pub(in crate::server) fn register(
    builder: &mut EntityTypeRegistryBuilder<'_>,
    catalog: &Arc<Catalog>,
) -> Result<(), EntityError> {
    super::storage::register::<3>(
        builder,
        catalog,
        HOPPER_ENTITY_TYPE,
        HOPPER_STATE,
        TickPolicy::Interval(20),
    )?;
    builder.register_tick_planner(HOPPER_ENTITY_TYPE, Arc::new(policy::Planner))
}
