//! Session-controlled public player entity type.

use super::registry::{
    EntityCodecError, EntityPayloadCodec, EntityTypeRegistration, EntityTypeRegistryBuilder,
};
use super::types::{EntityError, EntityOwnership, EntityPayload, TickPolicy};
use crate::content::EntityTypeId;
use std::sync::Arc;

pub(in crate::server) const PLAYER_ENTITY_TYPE: EntityTypeId = EntityTypeId(2);
pub(in crate::server) const MAX_PLAYER_ENTITY_PAYLOAD_BYTES: usize = 4;

/// Public cosmetic state only. Profile identity and movement authority remain
/// in the authenticated session, never in this network-visible payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::server) struct PlayerEntityPayload {
    pub(in crate::server) skin: u8,
    pub(in crate::server) shirt: u8,
    pub(in crate::server) pants: u8,
    pub(in crate::server) flags: u8,
}

impl PlayerEntityPayload {
    pub(in crate::server) const fn new(skin: u8, shirt: u8, pants: u8, flags: u8) -> Self {
        Self {
            skin,
            shirt,
            pants,
            flags,
        }
    }

    const fn encode(self) -> [u8; MAX_PLAYER_ENTITY_PAYLOAD_BYTES] {
        [self.skin, self.shirt, self.pants, self.flags]
    }
}

pub(in crate::server) fn register_player_entity_type(
    builder: &mut EntityTypeRegistryBuilder<'_>,
) -> Result<(), EntityError> {
    builder.register(EntityTypeRegistration {
        id: PLAYER_ENTITY_TYPE,
        ownership: EntityOwnership::Mobile,
        tick_policy: TickPolicy::EveryTick,
        max_payload_bytes: MAX_PLAYER_ENTITY_PAYLOAD_BYTES,
        codec: Arc::new(PlayerPayloadCodec),
    })
}

struct PlayerPayloadCodec;

impl EntityPayloadCodec for PlayerPayloadCodec {
    fn decode(&self, payload: &[u8]) -> Result<EntityPayload, EntityCodecError> {
        let value: [u8; MAX_PLAYER_ENTITY_PAYLOAD_BYTES] = payload
            .try_into()
            .map_err(|_| EntityCodecError::InvalidData)?;
        let [skin, shirt, pants, flags] = value;
        Ok(EntityPayload::new(PlayerEntityPayload::new(
            skin, shirt, pants, flags,
        )))
    }

    fn encode(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        payload
            .downcast_ref::<PlayerEntityPayload>()
            .copied()
            .map(PlayerEntityPayload::encode)
            .map(Vec::from)
            .ok_or(EntityCodecError::InvalidData)
    }

    fn public_view(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        self.encode(payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::Catalog;

    #[test]
    fn player_public_payload_codec_is_fixed_size() {
        let codec = PlayerPayloadCodec;
        let payload = EntityPayload::new(PlayerEntityPayload::new(1, 4, 7, 0));
        assert_eq!(codec.encode(&payload).unwrap(), [1, 4, 7, 0]);
        assert_eq!(codec.public_view(&payload).unwrap(), [1, 4, 7, 0]);
        assert_eq!(
            codec
                .decode(&[1, 4, 7, 0])
                .unwrap()
                .downcast_ref::<PlayerEntityPayload>(),
            Some(&PlayerEntityPayload::new(1, 4, 7, 0))
        );
        assert!(matches!(
            codec.decode(&[1, 4, 7]),
            Err(EntityCodecError::InvalidData)
        ));
    }

    #[test]
    fn player_type_registers_only_against_catalogued_identity() {
        let catalog = Catalog::builtins();
        let mut builder = EntityTypeRegistryBuilder::new(&catalog);
        register_player_entity_type(&mut builder).unwrap();
        assert_eq!(
            register_player_entity_type(&mut builder),
            Err(EntityError::DuplicateType(PLAYER_ENTITY_TYPE))
        );

        let mut unknown = EntityTypeRegistryBuilder::new(&catalog);
        assert_eq!(
            unknown.register(EntityTypeRegistration {
                id: EntityTypeId(70_000),
                ownership: EntityOwnership::Mobile,
                tick_policy: TickPolicy::EveryTick,
                max_payload_bytes: MAX_PLAYER_ENTITY_PAYLOAD_BYTES,
                codec: Arc::new(PlayerPayloadCodec),
            }),
            Err(EntityError::UnknownType(EntityTypeId(70_000)))
        );
    }
}
