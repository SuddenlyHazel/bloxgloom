//! Data-only counterparts of server runtime declarations. These never populate
//! gameplay dispatch, entity codecs, owner schedulers or generation contributors.
//! In particular, owner seeds/private state are represented only by their existing
//! compatibility fingerprint, not copied to clients.
use super::{Catalog, EntityTypeDef, EntityTypeId, hash_bytes};
use bloxgloom_host_api::RegistrationError;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub(crate) struct Entity {
    pub key: String,
    pub schema_version: u16,
    // Includes the opaque server codec/public-projection contract. We do not
    // reconstruct or call that codec, nor infer access to private state bytes.
    pub schema_fingerprint: u64,
    pub max_state_bytes: u16,
    pub initial_delay_ticks: Option<u32>,
}

#[derive(Clone, Debug)]
pub(crate) struct Identity {
    pub kind: u8,
    pub key: String,
    pub fingerprint: u64,
}

impl Identity {
    pub(crate) fn new(kind: u8, key: String, bytes: &[u8]) -> Self {
        let mut fingerprint = 0xcbf2_9ce4_8422_2325;
        hash_bytes(&mut fingerprint, bytes);
        Self {
            kind,
            key,
            fingerprint,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct Metadata {
    pub identities: BTreeMap<(u8, u32), (String, u64)>,
    pub entities: BTreeMap<String, Entity>,
}

impl Catalog {
    pub(crate) fn has_public_script_entity(&self, key: &str) -> bool {
        self.client_metadata.entities.contains_key(key) || self.gameplay_entities.contains_key(key)
    }
    pub(crate) fn client_entity(&mut self, entity: Entity) -> Result<(), RegistrationError> {
        if entity.max_state_bytes == 0
            || entity
                .initial_delay_ticks
                .is_some_and(|d| !(1..=100_000).contains(&d))
            || self.client_metadata.entities.len() + self.gameplay_entities.len() >= 256
        {
            return Err(RegistrationError("invalid client entity schema".into()));
        }
        self.register_entity_type(EntityTypeDef {
            id: EntityTypeId(self.entities.len() as u32),
            key: entity.key.clone().into(),
            schema_version: entity.schema_version,
            schema_fingerprint: entity.schema_fingerprint,
        })
        .map_err(|e| RegistrationError(format!("invalid client entity: {e:?}")))?;
        self.client_metadata
            .entities
            .insert(entity.key.clone(), entity);
        Ok(())
    }

    pub(crate) fn client_runtime_identity(
        &mut self,
        identity: Identity,
    ) -> Result<(), RegistrationError> {
        let (max, runtime) = match identity.kind {
            b'G' => (
                4096,
                self.gameplay_handlers
                    .iter()
                    .map(|(id, h)| (*id, h.key.as_str()))
                    .collect::<Vec<_>>(),
            ),
            b'O' => (
                128,
                self.gameplay_observers
                    .iter()
                    .map(|(id, p)| (*id, p.key.as_str()))
                    .collect::<Vec<_>>(),
            ),
            b'Q' => (
                128,
                self.player_lifecycles
                    .iter()
                    .map(|(id, p)| (*id, p.key.as_str()))
                    .collect::<Vec<_>>(),
            ),
            b'Y' => (
                128,
                self.owner_systems
                    .iter()
                    .map(|(id, s)| (*id, s.key.as_str()))
                    .collect::<Vec<_>>(),
            ),
            _ => {
                return Err(RegistrationError(
                    "unsupported client runtime identity".into(),
                ));
            }
        };
        let existing = runtime
            .into_iter()
            .chain(
                self.client_metadata
                    .identities
                    .iter()
                    .filter(|((kind, _), _)| *kind == identity.kind)
                    .map(|((_, id), (key, _))| (*id, key.as_str())),
            )
            .collect::<Vec<_>>();
        if !super::valid_key(&identity.key)
            || existing.len() >= max
            || existing.iter().any(|(_, key)| *key == identity.key)
        {
            return Err(RegistrationError(
                "duplicate or invalid client runtime identity".into(),
            ));
        }
        let id = existing
            .iter()
            .map(|(id, _)| *id)
            .max()
            .map_or(0, |id| id + 1);
        self.client_metadata
            .identities
            .insert((identity.kind, id), (identity.key, identity.fingerprint));
        Ok(())
    }
}
