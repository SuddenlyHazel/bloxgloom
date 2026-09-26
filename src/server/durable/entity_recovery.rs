//! Validate and recover the sparse entity store before accepting any clients.
//!
//! Entity identity, payload, indexes, and allocator are WAL-owned. Mobile
//! motion is checkpoint-owned between WAL-backed spawn/transfer fences, so its
//! current value is checked by revision rather than exact WAL-byte equality.

use crate::server::entities::{
    ENTITY_ALLOCATOR_DOMAIN, ENTITY_CELL_DOMAIN, ENTITY_CHUNK_DOMAIN, ENTITY_MOTION_DOMAIN,
    ENTITY_RECORD_DOMAIN, ENTITY_REVISION_DOMAIN, EntityCheckpointStore, EntityId, EntityStore,
    EntityTypeRegistry, decode_checkpoint, decode_motion_value, encode_checkpoint,
};
use crate::server::journal::{Journal, StateKey};
use std::collections::BTreeMap;
use std::io::{self, ErrorKind};
use std::path::Path;
use std::sync::Arc;

pub(super) struct PreparedEntityRecovery {
    pub(super) entities: EntityStore,
    pub(super) checkpoint_store: EntityCheckpointStore,
    replay: Option<Vec<u8>>,
}

impl PreparedEntityRecovery {
    /// Publish only after every journal participant has validated its current
    /// checkpoint and candidate after-value. A rejection must not leave some
    /// save files replayed while the server remains closed.
    pub(super) fn publish_replay(&self) -> io::Result<()> {
        if let Some(bytes) = &self.replay {
            self.checkpoint_store.write(bytes)?;
        }
        Ok(())
    }
}

pub(super) fn prepare(
    root: &Path,
    journal: &Journal,
    latest: &BTreeMap<StateKey, Vec<u8>>,
    types: Arc<EntityTypeRegistry>,
) -> io::Result<PreparedEntityRecovery> {
    let checkpoint_store = EntityCheckpointStore::new(root)?;
    checkpoint_store.recover_unpublished()?;
    let checkpoint = checkpoint_store.read()?;
    let mut entities = match checkpoint.as_deref() {
        Some(bytes) => decode_checkpoint(bytes, types).map_err(invalid_entity)?,
        None => EntityStore::new(types),
    };
    let mut overlay = BTreeMap::new();
    for (key, value) in latest {
        let is_entity = match key.domain.as_str() {
            ENTITY_RECORD_DOMAIN
            | ENTITY_MOTION_DOMAIN
            | ENTITY_ALLOCATOR_DOMAIN
            | ENTITY_CHUNK_DOMAIN
            | ENTITY_CELL_DOMAIN
            | ENTITY_REVISION_DOMAIN => true,
            domain if domain.starts_with("bloxgloom:entity") => {
                return Err(invalid_data("unknown required entity journal domain"));
            }
            _ => false,
        };
        if !is_entity {
            continue;
        }
        let current = entities
            .checkpoint_value_for_key(key)
            .map_err(invalid_entity)?
            .ok_or_else(|| invalid_data("unhandled entity journal key"))?;
        if key.domain == ENTITY_MOTION_DOMAIN {
            validate_checkpointed_motion(journal, key, &current, value)?;
        } else {
            journal.validate_snapshot(key, &current)?;
        }
        overlay.insert(key.clone(), value.clone());
    }
    let changed = entities
        .apply_journal_overlay(&overlay)
        .map_err(invalid_entity)?;
    let replay = changed
        .then(|| encode_checkpoint(&entities).map_err(invalid_entity))
        .transpose()?;
    Ok(PreparedEntityRecovery {
        entities,
        checkpoint_store,
        replay,
    })
}

fn validate_checkpointed_motion(
    journal: &Journal,
    key: &StateKey,
    checkpoint: &[u8],
    latest: &[u8],
) -> io::Result<()> {
    // An absent checkpoint must still be a valid WAL starting point. A live
    // checkpoint position may have advanced after the last WAL motion fence,
    // or may lag a newer WAL transfer. Either is recoverable; equal revisions
    // must identify exactly the same motion value.
    if checkpoint.is_empty() {
        return journal.validate_snapshot(key, checkpoint);
    }
    let id = decode_motion_id(key)?;
    let current = decode_motion_value(id, checkpoint).map_err(invalid_entity)?;
    if latest.is_empty() {
        return Ok(());
    }
    let durable = decode_motion_value(id, latest).map_err(invalid_entity)?;
    if current.revision == durable.revision
        && current.position.map(f32::to_bits) != durable.position.map(f32::to_bits)
    {
        return Err(invalid_data(
            "entity checkpoint motion conflicts with its WAL fence",
        ));
    }
    Ok(())
}

fn decode_motion_id(key: &StateKey) -> io::Result<EntityId> {
    let bytes: [u8; 8] = key
        .bytes
        .as_slice()
        .try_into()
        .map_err(|_| invalid_data("invalid entity motion journal key"))?;
    EntityId::new(u64::from_le_bytes(bytes))
        .ok_or_else(|| invalid_data("zero entity motion journal key"))
}

fn invalid_entity(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, error)
}

fn invalid_data(message: &'static str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}

#[cfg(test)]
#[path = "entity_recovery/tests.rs"]
mod tests;
