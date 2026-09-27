//! Shared fixed-slot storage codecs, interactions, and automation ports.
pub(super) mod codec;
pub(super) mod policy;
#[cfg(test)]
mod tests;
use super::*;
use crate::content::{BlockStateId, Catalog, EntityTypeId};
use crate::inventory::{Inventory, STACK_LIMIT, Stack};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::server) struct StoragePayload<const N: usize> {
    pub slots: [Option<Stack>; N],
}

pub(in crate::server) trait Slots: Clone + Send + Sync + 'static {
    fn slots_mut(&mut self) -> &mut [Option<Stack>];
}
impl<const N: usize> Slots for StoragePayload<N> {
    fn slots_mut(&mut self) -> &mut [Option<Stack>] {
        &mut self.slots
    }
}
impl<const N: usize> Default for StoragePayload<N> {
    fn default() -> Self {
        Self {
            slots: std::array::from_fn(|_| None),
        }
    }
}

pub(super) fn register<const N: usize>(
    builder: &mut EntityTypeRegistryBuilder<'_>,
    catalog: &Arc<Catalog>,
    entity_type: EntityTypeId,
    state: BlockStateId,
    tick: TickPolicy,
) -> Result<(), EntityError> {
    if N == 0 || N > bloxgloom_host_api::inventory::MAX_SLOTS {
        return Err(EntityError::InvalidType);
    }
    builder.register(EntityTypeRegistration {
        id: entity_type,
        ownership: EntityOwnership::anchored(vec![state], 1),
        tick_policy: tick,
        max_payload_bytes: crate::inventory::container::max_bytes(N),
        codec: Arc::new(codec::Codec::<N> {
            catalog: catalog.clone(),
        }),
    })?;
    builder.register_transfer_policy(
        entity_type,
        Arc::new(policy::Port::<StoragePayload<N>>::for_screen(
            catalog
                .inventory_screen(entity_type)
                .cloned()
                .ok_or(EntityError::InvalidType)?,
        )),
    )?;
    builder.register_interaction_policy(
        entity_type,
        Arc::new(policy::Interaction::<StoragePayload<N>>::new()),
    )
}
