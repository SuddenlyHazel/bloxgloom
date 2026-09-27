//! Bounded byte-state entities independent of creature/machine contracts.
use super::*;
use bloxgloom_host_api::gameplay::EntityDefinition;
use std::sync::Arc;

pub(in crate::server) struct Codec {
    pub definition: Arc<EntityDefinition>,
}
impl EntityPayloadCodec for Codec {
    fn decode(&self, bytes: &[u8]) -> Result<EntityPayload, EntityCodecError> {
        self.check(bytes)?;
        Ok(EntityPayload::new(bytes.to_vec()))
    }
    fn encode(&self, state: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        let bytes = state
            .downcast_ref::<Vec<u8>>()
            .ok_or(EntityCodecError::InvalidData)?;
        self.check(bytes)?;
        Ok(bytes.clone())
    }
    fn public_view(&self, state: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        let bytes = self.encode(state)?;
        let public = self
            .definition
            .state
            .public(&bytes)
            .map_err(|_| EntityCodecError::InvalidData)?;
        if public.len() > super::types::MAX_ENTITY_PUBLIC_VIEW_BYTES {
            return Err(EntityCodecError::InvalidData);
        }
        Ok(public)
    }
}
impl Codec {
    fn check(&self, bytes: &[u8]) -> Result<(), EntityCodecError> {
        if bytes.len() > self.definition.max_state_bytes as usize {
            return Err(EntityCodecError::InvalidData);
        }
        self.definition
            .state
            .validate(bytes)
            .map_err(|_| EntityCodecError::InvalidData)
    }
}
