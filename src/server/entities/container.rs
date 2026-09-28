//! Runtime-capacity storage service backing public lifecycle declarations.
use super::storage::{Slots, policy};
use super::*;
use crate::content::Catalog;
use crate::inventory::{Stack, container};
use std::sync::Arc;
#[cfg(test)]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::server) struct ContainerPayload {
    pub slots: Vec<Option<Stack>>,
}
impl Slots for ContainerPayload {
    fn slots_mut(&mut self) -> &mut [Option<Stack>] {
        &mut self.slots
    }
}

struct Codec {
    catalog: Arc<Catalog>,
    slots: usize,
}
impl EntityPayloadCodec for Codec {
    fn encode(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        let payload = payload
            .downcast_ref::<ContainerPayload>()
            .filter(|p| p.slots.len() == self.slots)
            .ok_or(EntityCodecError::InvalidData)?;
        container::encode(&payload.slots, &self.catalog).map_err(|_| EntityCodecError::InvalidData)
    }
    fn decode(&self, bytes: &[u8]) -> Result<EntityPayload, EntityCodecError> {
        let slots = container::decode(bytes, self.slots, &self.catalog)
            .map_err(|_| EntityCodecError::InvalidData)?;
        Ok(EntityPayload::new(ContainerPayload { slots }))
    }
    fn public_view(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        let payload = payload
            .downcast_ref::<ContainerPayload>()
            .filter(|p| p.slots.len() == self.slots)
            .ok_or(EntityCodecError::InvalidData)?;
        Ok(crate::protocol::workstation::WorkstationView {
            slots: payload.slots.clone(),
            status: vec![],
        }
        .encode())
    }
}
pub(in crate::server) fn register(
    builder: &mut EntityTypeRegistryBuilder<'_>,
    catalog: &Arc<Catalog>,
    definition: &crate::server::lifecycle::Resolved,
) -> Result<(), EntityError> {
    builder.register(EntityTypeRegistration {
        id: definition.entity,
        ownership: EntityOwnership::anchored(
            vec![definition.anchor],
            definition.definition.footprint.len(),
        ),
        tick_policy: TickPolicy::Never,
        max_payload_bytes: container::max_bytes(definition.definition.slots),
        codec: Arc::new(Codec {
            catalog: catalog.clone(),
            slots: definition.definition.slots,
        }),
    })?;
    builder.register_transfer_policy(
        definition.entity,
        Arc::new(policy::Port::<ContainerPayload>::for_screen_with_faces(
            catalog
                .inventory_screen(definition.entity)
                .cloned()
                .ok_or(EntityError::InvalidType)?,
            definition.definition.allowed_automation_faces(),
        )),
    )?;
    builder.register_interaction_policy(
        definition.entity,
        Arc::new(policy::Interaction::<ContainerPayload>::new()),
    )
}
