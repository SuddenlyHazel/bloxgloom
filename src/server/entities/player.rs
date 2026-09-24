//! Session-controlled public player entity type.

use super::EntityDelta;
use super::registry::{
    EntityCodecError, EntityPayloadCodec, EntityTypeRegistration, EntityTypeRegistryBuilder,
};
use super::types::{
    EntityError, EntityId, EntityLocation, EntityOwnership, EntityPayload, EntityPublicView,
    TickPolicy,
};
use crate::content::EntityTypeId;
use crate::world::ChunkKey;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub(in crate::server) const PLAYER_ENTITY_TYPE: EntityTypeId = EntityTypeId(2);
pub(in crate::server) const MAX_PLAYER_ENTITY_PAYLOAD_BYTES: usize = 4;
const MAX_SESSION_PLAYER_ENTITIES: usize = 256;

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

/// Runtime-only public player views. This deliberately shares the registered
/// player entity type and public codec, while keeping session motion outside
/// the WAL-owned/checkpointed entity aggregate.
#[derive(Default)]
pub(in crate::server) struct PlayerEntityStore {
    by_session: BTreeMap<u64, EntityPublicView>,
    by_chunk: BTreeMap<ChunkKey, BTreeSet<EntityId>>,
}

impl PlayerEntityStore {
    pub(in crate::server) fn spawn_session(
        &mut self,
        session_id: u64,
        position: [f32; 3],
    ) -> Result<(EntityId, EntityDelta), EntityError> {
        if self.by_session.contains_key(&session_id)
            || self.by_session.len() >= MAX_SESSION_PLAYER_ENTITIES
        {
            return Err(EntityError::TooManyEntities);
        }
        let id = EntityId::for_player_session(session_id).ok_or(EntityError::IdExhausted)?;
        let location = EntityLocation::Mobile { position };
        let owner = location.owner()?;
        let payload = PlayerEntityPayload::new(0, 0, 0, 0);
        let public_view = PlayerPayloadCodec
            .public_view(&EntityPayload::new(payload))
            .map_err(|_| EntityError::CodecRejected)?;
        let view = EntityPublicView {
            id,
            entity_type: PLAYER_ENTITY_TYPE,
            revision: 1,
            motion_revision: 1,
            owner,
            location,
            payload: public_view,
        };
        if self.by_session.insert(session_id, view.clone()).is_some() {
            return Err(EntityError::InvalidTransaction);
        }
        self.by_chunk.entry(owner.chunk()).or_default().insert(id);
        Ok((id, EntityDelta::Spawned(view)))
    }

    pub(in crate::server) fn update_position(
        &mut self,
        session_id: u64,
        position: [f32; 3],
    ) -> Result<Option<EntityDelta>, EntityError> {
        let before =
            self.by_session
                .get(&session_id)
                .cloned()
                .ok_or(EntityError::UnknownEntity(
                    EntityId::for_player_session(session_id).ok_or(EntityError::IdExhausted)?,
                ))?;
        let location = EntityLocation::Mobile { position };
        if location == before.location {
            return Ok(None);
        }
        let owner = location.owner()?;
        let motion_revision = before
            .motion_revision
            .checked_add(1)
            .ok_or(EntityError::RevisionExhausted)?;
        let mut after = before.clone();
        after.location = location;
        after.owner = owner;
        after.motion_revision = motion_revision;
        if owner != before.owner {
            let old_ids = self
                .by_chunk
                .get_mut(&before.owner.chunk())
                .ok_or(EntityError::InvalidTransaction)?;
            if !old_ids.remove(&before.id) {
                return Err(EntityError::InvalidTransaction);
            }
            if old_ids.is_empty() {
                self.by_chunk.remove(&before.owner.chunk());
            }
            self.by_chunk
                .entry(owner.chunk())
                .or_default()
                .insert(after.id);
        }
        self.by_session.insert(session_id, after.clone());
        if owner == before.owner {
            Ok(Some(EntityDelta::Moved(after)))
        } else {
            Ok(Some(EntityDelta::Transferred {
                before_owner: before.owner,
                before_touched_chunks: vec![before.owner.chunk()],
                view: after,
            }))
        }
    }

    pub(in crate::server) fn despawn_session(
        &mut self,
        session_id: u64,
    ) -> Result<Option<EntityDelta>, EntityError> {
        let Some(before) = self.by_session.remove(&session_id) else {
            return Ok(None);
        };
        let revision = before
            .revision
            .checked_add(1)
            .ok_or(EntityError::RevisionExhausted)?;
        let ids = self
            .by_chunk
            .get_mut(&before.owner.chunk())
            .ok_or(EntityError::InvalidTransaction)?;
        if !ids.remove(&before.id) {
            return Err(EntityError::InvalidTransaction);
        }
        if ids.is_empty() {
            self.by_chunk.remove(&before.owner.chunk());
        }
        Ok(Some(EntityDelta::Despawned {
            id: before.id,
            entity_type: before.entity_type,
            revision,
            owner: before.owner,
            touched_chunks: vec![before.owner.chunk()],
        }))
    }

    /// Roll back an unannounced join if its outbound handshake cannot be
    /// queued. No public removal is needed because peers never saw the spawn.
    pub(in crate::server) fn discard_session(&mut self, session_id: u64) {
        let Some(view) = self.by_session.remove(&session_id) else {
            return;
        };
        if let Some(ids) = self.by_chunk.get_mut(&view.owner.chunk()) {
            ids.remove(&view.id);
            if ids.is_empty() {
                self.by_chunk.remove(&view.owner.chunk());
            }
        }
    }

    pub(in crate::server) fn id_for_session(&self, session_id: u64) -> Option<EntityId> {
        self.by_session.get(&session_id).map(|view| view.id)
    }

    pub(in crate::server) fn public_views_for_chunk_bounded(
        &self,
        chunk: ChunkKey,
        limit: usize,
    ) -> Result<Vec<EntityPublicView>, EntityError> {
        let Some(ids) = self.by_chunk.get(&chunk) else {
            return Ok(Vec::new());
        };
        if ids.len() > limit {
            return Err(EntityError::SpatialQueryTooBroad);
        }
        ids.iter()
            .map(|id| {
                self.by_session
                    .values()
                    .find(|view| view.id == *id)
                    .cloned()
                    .ok_or(EntityError::InvalidTransaction)
            })
            .collect()
    }
}

pub(in crate::server) fn register_player_entity_type(
    builder: &mut EntityTypeRegistryBuilder<'_>,
) -> Result<(), EntityError> {
    builder.register(EntityTypeRegistration {
        id: PLAYER_ENTITY_TYPE,
        ownership: EntityOwnership::Mobile,
        tick_policy: TickPolicy::Never,
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
