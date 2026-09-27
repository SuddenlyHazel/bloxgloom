//! Runtime-capacity storage service backing public lifecycle declarations.
use super::storage::{Slots, policy};
use super::*;
use crate::content::Catalog;
use crate::inventory::{Inventory, InventoryStore, Stack};
use crate::protocol::workstation::{WorkstationKind, WorkstationView};
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
    legacy_chest_view: bool,
}
impl EntityPayloadCodec for Codec {
    fn encode(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        let payload = payload
            .downcast_ref::<ContainerPayload>()
            .ok_or(EntityCodecError::InvalidData)?;
        if payload.slots.len() != self.slots {
            return Err(EntityCodecError::InvalidData);
        }
        let mut inventory = Inventory::default();
        inventory.slots[..self.slots].clone_from_slice(&payload.slots);
        InventoryStore::encode_snapshot_with_catalog(&inventory, &self.catalog)
            .map_err(|_| EntityCodecError::InvalidData)
    }
    fn decode(&self, bytes: &[u8]) -> Result<EntityPayload, EntityCodecError> {
        if bytes.len() > self.slots * 1100 + 128 {
            return Err(EntityCodecError::InvalidData);
        }
        let inventory = InventoryStore::decode_snapshot_with_catalog(bytes, &self.catalog)
            .map_err(|_| EntityCodecError::InvalidData)?;
        if inventory.revision != 0 || inventory.slots[self.slots..].iter().any(Option::is_some) {
            return Err(EntityCodecError::InvalidData);
        }
        Ok(EntityPayload::new(ContainerPayload {
            slots: inventory.slots[..self.slots].to_vec(),
        }))
    }
    fn public_view(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        let payload = payload
            .downcast_ref::<ContainerPayload>()
            .filter(|p| p.slots.len() == self.slots)
            .ok_or(EntityCodecError::InvalidData)?;
        if self.legacy_chest_view {
            return Ok(WorkstationView {
                kind: WorkstationKind::Chest,
                slots: payload.slots.clone(),
                ..Default::default()
            }
            .encode());
        }
        // Opaque projection for unconfigured clients. Generic screen discovery
        // is the next slice; authoritative slots still use the shared service.
        let mut bytes = vec![4, self.slots as u8];
        for slot in &payload.slots {
            bytes.extend(slot.as_ref().map_or(0, |s| s.item.0).to_le_bytes());
            bytes.extend(slot.as_ref().map_or(0, |s| s.count).to_le_bytes());
        }
        Ok(bytes)
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
        max_payload_bytes: definition.definition.slots * 1100 + 128,
        codec: Arc::new(Codec {
            catalog: catalog.clone(),
            slots: definition.definition.slots,
            legacy_chest_view: definition.definition.entity == "bloxgloom:chest",
        }),
    })?;
    builder.register_transfer_policy(
        definition.entity,
        Arc::new(policy::Port::<ContainerPayload>::new()),
    )?;
    builder.register_interaction_policy(
        definition.entity,
        Arc::new(policy::Interaction::<ContainerPayload>::new()),
    )
}
