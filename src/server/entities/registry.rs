use super::types::{
    EntityError, EntityOwnership, EntityPayload, MAX_ENTITY_FOOTPRINT_CELLS,
    MAX_ENTITY_PAYLOAD_BYTES, MAX_ENTITY_PUBLIC_VIEW_BYTES, TickPolicy,
};
use crate::content::{Catalog, EntityTypeId};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntityCodecError {
    InvalidData,
    UnsupportedVersion,
}

/// Type-specific payload decoding, migration, persistence encoding, and public projection.
///
/// Payloads stay decoded in the live store; codecs serialize only for WAL and
/// checkpoint values or the bounded public view.
pub trait EntityPayloadCodec: Send + Sync + 'static {
    fn decode(&self, payload: &[u8]) -> Result<EntityPayload, EntityCodecError>;

    fn encode(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityCodecError>;

    fn migrate(
        &self,
        from_version: u16,
        to_version: u16,
        payload: &[u8],
    ) -> Result<Vec<u8>, EntityCodecError> {
        if from_version == to_version {
            Ok(payload.to_vec())
        } else {
            Err(EntityCodecError::UnsupportedVersion)
        }
    }

    fn public_view(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityCodecError>;
}

#[derive(Clone)]
pub struct EntityTypeDescriptor {
    id: EntityTypeId,
    key: String,
    schema_version: u16,
    schema_fingerprint: u64,
    ownership: EntityOwnership,
    tick_policy: TickPolicy,
    max_payload_bytes: usize,
    codec: Arc<dyn EntityPayloadCodec>,
}

impl EntityTypeDescriptor {
    pub const fn id(&self) -> EntityTypeId {
        self.id
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub const fn schema_version(&self) -> u16 {
        self.schema_version
    }

    pub const fn schema_fingerprint(&self) -> u64 {
        self.schema_fingerprint
    }

    pub fn ownership(&self) -> &EntityOwnership {
        &self.ownership
    }

    pub const fn tick_policy(&self) -> TickPolicy {
        self.tick_policy
    }

    pub const fn max_payload_bytes(&self) -> usize {
        self.max_payload_bytes
    }

    pub fn encode_payload(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityError> {
        let encoded = self
            .codec
            .encode(payload)
            .map_err(|_| EntityError::CodecRejected)?;
        if encoded.len() > self.max_payload_bytes {
            return Err(EntityError::PayloadTooLarge);
        }
        Ok(encoded)
    }

    pub fn public_view(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityError> {
        self.encode_payload(payload)?;
        let view = self
            .codec
            .public_view(payload)
            .map_err(|_| EntityError::CodecRejected)?;
        if view.len() > MAX_ENTITY_PUBLIC_VIEW_BYTES {
            return Err(EntityError::PublicViewTooLarge);
        }
        Ok(view)
    }

    pub fn decode_payload(
        &self,
        stored_version: u16,
        payload: &[u8],
    ) -> Result<EntityPayload, EntityError> {
        if payload.len() > self.max_payload_bytes {
            return Err(EntityError::PayloadTooLarge);
        }
        if stored_version == 0 || stored_version > self.schema_version {
            return Err(EntityError::CodecRejected);
        }
        let migrated = if stored_version == self.schema_version {
            payload.to_vec()
        } else {
            self.codec
                .migrate(stored_version, self.schema_version, payload)
                .map_err(|_| EntityError::CodecRejected)?
        };
        if migrated.len() > self.max_payload_bytes {
            return Err(EntityError::PayloadTooLarge);
        }
        let decoded = self
            .codec
            .decode(&migrated)
            .map_err(|_| EntityError::CodecRejected)?;
        if self.encode_payload(&decoded)? != migrated {
            return Err(EntityError::InvalidPayload);
        }
        Ok(decoded)
    }
}

/// Registration parameters paired with an already registered content entity.
pub struct EntityTypeRegistration {
    pub id: EntityTypeId,
    pub ownership: EntityOwnership,
    pub tick_policy: TickPolicy,
    pub max_payload_bytes: usize,
    pub codec: Arc<dyn EntityPayloadCodec>,
}

/// Startup-only type registry builder. Freezing requires one lifecycle
/// implementation for every entity type in the resolved content catalog.
pub struct EntityTypeRegistryBuilder<'a> {
    catalog: &'a Catalog,
    descriptors: BTreeMap<EntityTypeId, EntityTypeDescriptor>,
}

impl<'a> EntityTypeRegistryBuilder<'a> {
    pub fn new(catalog: &'a Catalog) -> Self {
        Self {
            catalog,
            descriptors: BTreeMap::new(),
        }
    }

    pub fn register(&mut self, registration: EntityTypeRegistration) -> Result<(), EntityError> {
        if self.descriptors.contains_key(&registration.id) {
            return Err(EntityError::DuplicateType(registration.id));
        }
        let content_type = self
            .catalog
            .entity_type(registration.id)
            .ok_or(EntityError::UnknownType(registration.id))?;
        if content_type.schema_version == 0
            || content_type.key.is_empty()
            || registration.max_payload_bytes > MAX_ENTITY_PAYLOAD_BYTES
            || matches!(registration.tick_policy, TickPolicy::Interval(0))
        {
            return Err(EntityError::InvalidType);
        }
        match &registration.ownership {
            EntityOwnership::Mobile => {}
            EntityOwnership::Anchored {
                compatible_anchor_states,
                max_footprint_cells,
            } => {
                if compatible_anchor_states.is_empty()
                    || *max_footprint_cells == 0
                    || *max_footprint_cells > MAX_ENTITY_FOOTPRINT_CELLS
                    || compatible_anchor_states
                        .iter()
                        .any(|state| state.0 == 0 || self.catalog.state(*state).is_none())
                {
                    return Err(EntityError::InvalidType);
                }
            }
        }
        let descriptor = EntityTypeDescriptor {
            id: registration.id,
            key: content_type.key.to_string(),
            schema_version: content_type.schema_version,
            schema_fingerprint: content_type.schema_fingerprint,
            ownership: registration.ownership,
            tick_policy: registration.tick_policy,
            max_payload_bytes: registration.max_payload_bytes,
            codec: registration.codec,
        };
        self.descriptors.insert(registration.id, descriptor);
        Ok(())
    }

    pub fn freeze(self) -> Result<EntityTypeRegistry, EntityError> {
        for (kind, id, _, _) in self.catalog.identities() {
            if kind == b'E' {
                let id = EntityTypeId(id);
                if !self.descriptors.contains_key(&id) {
                    return Err(EntityError::MissingTypeRegistration(id));
                }
            }
        }
        if self
            .descriptors
            .keys()
            .any(|id| self.catalog.entity_type(*id).is_none())
        {
            return Err(EntityError::InvalidType);
        }
        Ok(EntityTypeRegistry {
            descriptors: self.descriptors,
        })
    }
}

#[derive(Clone)]
pub struct EntityTypeRegistry {
    descriptors: BTreeMap<EntityTypeId, EntityTypeDescriptor>,
}

impl EntityTypeRegistry {
    pub fn descriptor(&self, id: EntityTypeId) -> Result<&EntityTypeDescriptor, EntityError> {
        self.descriptors
            .get(&id)
            .ok_or(EntityError::UnknownRequiredType(id))
    }

    pub fn descriptors(&self) -> impl Iterator<Item = &EntityTypeDescriptor> {
        self.descriptors.values()
    }
}
