//! Drop entity type registration and its bounded durable payload codec.

use crate::content::{Catalog, EntityTypeId};
use crate::inventory::{ComponentPayload, MAX_COMPONENT_BYTES, STACK_LIMIT, Stack};
use crate::items::ItemId;
use crate::server::entities::{
    EntityCodecError, EntityError, EntityOwnership, EntityPayload, EntityPayloadCodec,
    EntityTypeRegistration, EntityTypeRegistryBuilder, TickPolicy,
};
use std::sync::Arc;
use std::time::Duration;

pub(in crate::server) const DROP_ENTITY_TYPE: EntityTypeId = EntityTypeId(1);
const DROP_PAYLOAD_FIXED_BYTES: usize = 4 + 2 + 8 + 8 + 2 + 2;
pub(in crate::server) const MAX_DROP_ENTITY_PAYLOAD_BYTES: usize =
    DROP_PAYLOAD_FIXED_BYTES + MAX_COMPONENT_BYTES;

/// WAL-owned drop state. Position and velocity remain in the generic entity's
/// checkpointed mobile motion field and are intentionally not duplicated here.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::server) struct DropEntityPayload {
    pub(in crate::server) stack: Stack,
    pub(in crate::server) created_unix_ms: u64,
    pub(in crate::server) pickup_delay: Duration,
}

impl DropEntityPayload {
    pub(in crate::server) fn new(
        stack: Stack,
        created_unix_ms: u64,
        pickup_delay: Duration,
    ) -> Self {
        Self {
            stack,
            created_unix_ms,
            pickup_delay,
        }
    }

    pub(in crate::server) fn into_entity_payload(self) -> EntityPayload {
        EntityPayload::new(self)
    }
}

pub(in crate::server) fn register_entity_type(
    builder: &mut EntityTypeRegistryBuilder<'_>,
    catalog: Arc<Catalog>,
) -> Result<(), EntityError> {
    builder.register(EntityTypeRegistration {
        id: DROP_ENTITY_TYPE,
        ownership: EntityOwnership::Mobile,
        tick_policy: TickPolicy::EveryTick,
        max_payload_bytes: MAX_DROP_ENTITY_PAYLOAD_BYTES,
        codec: Arc::new(DropPayloadCodec { catalog }),
    })
}

struct DropPayloadCodec {
    catalog: Arc<Catalog>,
}

impl EntityPayloadCodec for DropPayloadCodec {
    fn decode(&self, bytes: &[u8]) -> Result<EntityPayload, EntityCodecError> {
        if bytes.len() < DROP_PAYLOAD_FIXED_BYTES || bytes.len() > MAX_DROP_ENTITY_PAYLOAD_BYTES {
            return Err(EntityCodecError::InvalidData);
        }
        let item = ItemId::new(u32::from_le_bytes(
            bytes[0..4]
                .try_into()
                .map_err(|_| EntityCodecError::InvalidData)?,
        ));
        let count = u16::from_le_bytes(
            bytes[4..6]
                .try_into()
                .map_err(|_| EntityCodecError::InvalidData)?,
        );
        let created_unix_ms = u64::from_le_bytes(
            bytes[6..14]
                .try_into()
                .map_err(|_| EntityCodecError::InvalidData)?,
        );
        let pickup_delay_ms = u64::from_le_bytes(
            bytes[14..22]
                .try_into()
                .map_err(|_| EntityCodecError::InvalidData)?,
        );
        let component_version = u16::from_le_bytes(
            bytes[22..24]
                .try_into()
                .map_err(|_| EntityCodecError::InvalidData)?,
        );
        let component_len = usize::from(u16::from_le_bytes(
            bytes[24..26]
                .try_into()
                .map_err(|_| EntityCodecError::InvalidData)?,
        ));
        if component_len > MAX_COMPONENT_BYTES
            || bytes.len() != DROP_PAYLOAD_FIXED_BYTES + component_len
        {
            return Err(EntityCodecError::InvalidData);
        }
        let components = if component_len == 0 {
            if component_version != 0 {
                return Err(EntityCodecError::InvalidData);
            }
            None
        } else {
            let payload = ComponentPayload::new(
                component_version,
                bytes[DROP_PAYLOAD_FIXED_BYTES..].to_vec(),
            )
            .ok_or(EntityCodecError::InvalidData)?;
            Some(Arc::new(payload))
        };
        let stack = Stack {
            item,
            count,
            components,
        };
        if !stack.valid_in(&self.catalog) || !(1..=STACK_LIMIT).contains(&count) {
            return Err(EntityCodecError::InvalidData);
        }
        Ok(DropEntityPayload::new(
            stack,
            created_unix_ms,
            Duration::from_millis(pickup_delay_ms),
        )
        .into_entity_payload())
    }

    fn encode(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        let payload = payload
            .downcast_ref::<DropEntityPayload>()
            .ok_or(EntityCodecError::InvalidData)?;
        if !payload.stack.valid_in(&self.catalog)
            || !(1..=STACK_LIMIT).contains(&payload.stack.count)
        {
            return Err(EntityCodecError::InvalidData);
        }
        let component_len = payload
            .stack
            .components
            .as_ref()
            .map_or(0, |component| component.bytes.len());
        let mut bytes = Vec::with_capacity(DROP_PAYLOAD_FIXED_BYTES + component_len);
        bytes.extend(payload.stack.item.get().to_le_bytes());
        bytes.extend(payload.stack.count.to_le_bytes());
        bytes.extend(payload.created_unix_ms.to_le_bytes());
        let pickup_delay_ms = u64::try_from(payload.pickup_delay.as_millis())
            .map_err(|_| EntityCodecError::InvalidData)?;
        bytes.extend(pickup_delay_ms.to_le_bytes());
        if let Some(component) = &payload.stack.components {
            bytes.extend(component.version.to_le_bytes());
            bytes.extend(
                u16::try_from(component.bytes.len())
                    .map_err(|_| EntityCodecError::InvalidData)?
                    .to_le_bytes(),
            );
            bytes.extend(&component.bytes);
        } else {
            bytes.extend(0u16.to_le_bytes());
            bytes.extend(0u16.to_le_bytes());
        }
        if bytes.len() > MAX_DROP_ENTITY_PAYLOAD_BYTES {
            return Err(EntityCodecError::InvalidData);
        }
        Ok(bytes)
    }

    fn public_view(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        let payload = payload
            .downcast_ref::<DropEntityPayload>()
            .ok_or(EntityCodecError::InvalidData)?;
        if !payload.stack.valid_in(&self.catalog) {
            return Err(EntityCodecError::InvalidData);
        }
        let mut bytes = Vec::with_capacity(6);
        bytes.extend(payload.stack.item.get().to_le_bytes());
        bytes.extend(payload.stack.count.to_le_bytes());
        Ok(bytes)
    }
}

#[cfg(test)]
#[path = "entity/tests.rs"]
mod tests;
