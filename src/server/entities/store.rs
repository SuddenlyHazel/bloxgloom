use super::persistence::{encode_allocator_value, encode_record_value};
use super::registry::{EntityTypeDescriptor, EntityTypeRegistry};
use super::spatial::{
    EntityIndexes, decode_cell_key, decode_chunk_key, encode_cell_key, encode_cell_owner,
    encode_chunk_key, validate_location_owner, validate_ownership_mode,
};
use super::types::{
    AnchorUpdate, CellCoord, EntityError, EntityId, EntityLocation, EntityOwner, EntityOwnership,
    EntityPayload, EntityPublicView, position_to_cell,
};
use crate::content::{BlockStateId, EntityTypeId};
use crate::server::journal::{Change, StateKey};
use crate::world::ChunkKey;
use std::collections::BTreeMap;
use std::sync::Arc;

pub const ENTITY_RECORD_DOMAIN: &str = "bloxgloom:entity";
pub const ENTITY_ALLOCATOR_DOMAIN: &str = "bloxgloom:entity_allocator";
pub const ENTITY_CHUNK_DOMAIN: &str = "bloxgloom:entity_chunk";
pub const ENTITY_CELL_DOMAIN: &str = "bloxgloom:entity_cell";
pub const MAX_ENTITY_RECORDS: usize = 1_048_576;
pub const MAX_ENTITY_TRANSACTION_CHANGES: usize = 16_384;

#[derive(Clone, Debug)]
pub struct EntityRecord {
    pub id: EntityId,
    pub entity_type: EntityTypeId,
    pub schema_version: u16,
    pub schema_fingerprint: u64,
    pub owner: EntityOwner,
    pub revision: u64,
    pub location: EntityLocation,
    pub payload: EntityPayload,
    pub payload_size: usize,
    pub public_view: Vec<u8>,
    pub next_tick: Option<u64>,
}

impl PartialEq for EntityRecord {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.entity_type == other.entity_type
            && self.schema_version == other.schema_version
            && self.schema_fingerprint == other.schema_fingerprint
            && self.owner == other.owner
            && self.revision == other.revision
            && self.location == other.location
            && self.payload.same_instance(&other.payload)
            && self.payload_size == other.payload_size
            && self.public_view == other.public_view
            && self.next_tick == other.next_tick
    }
}

impl EntityRecord {
    pub fn public_view(&self) -> EntityPublicView {
        EntityPublicView {
            id: self.id,
            entity_type: self.entity_type,
            revision: self.revision,
            owner: self.owner,
            location: self.location.clone(),
            payload: self.public_view.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum EntitySpawn {
    Mobile {
        entity_type: EntityTypeId,
        position: [f32; 3],
        payload: EntityPayload,
        spawn_tick: u64,
    },
    Anchored {
        entity_type: EntityTypeId,
        anchor: CellCoord,
        anchor_state: BlockStateId,
        footprint: Vec<CellCoord>,
        payload: EntityPayload,
        spawn_tick: u64,
    },
}

#[derive(Clone, Debug, Default)]
pub struct EntityPatch {
    /// Current-schema private payload replacement.
    pub payload: Option<EntityPayload>,
    /// `Some(None)` suspends ticking; `Some(Some(tick))` sets its next due tick.
    pub next_tick: Option<Option<u64>>,
    /// Same-owner mobile position change. Cross-chunk movement uses transfer.
    pub position: Option<[f32; 3]>,
}

#[derive(Clone, Debug)]
pub struct EntitySnapshot {
    pub id: EntityId,
    pub entity_type: EntityTypeId,
    pub revision: u64,
    pub owner: EntityOwner,
    pub location: EntityLocation,
    pub private_payload: EntityPayload,
    pub next_tick: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EntityDelta {
    Spawned(EntityPublicView),
    Updated(EntityPublicView),
    Transferred {
        before_owner: EntityOwner,
        view: EntityPublicView,
    },
    Despawned {
        id: EntityId,
        entity_type: EntityTypeId,
        revision: u64,
        owner: EntityOwner,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct EntityCommit {
    pub registry_revision: u64,
    pub deltas: Vec<EntityDelta>,
}

#[derive(Clone, Debug)]
enum Operation {
    Spawn {
        after: EntityRecord,
        expected_allocator: u64,
    },
    Replace {
        before: EntityRecord,
        after: EntityRecord,
        transferred: bool,
    },
    Despawn {
        before: EntityRecord,
        removal_revision: u64,
    },
}

/// Full-key prepared mutation. The caller combines `changes()` with linked
/// block, inventory, or item-ownership changes before submitting one WAL record.
#[derive(Clone, Debug)]
pub struct PreparedEntityTransaction {
    operation: Operation,
    changes: Vec<Change>,
}

impl PreparedEntityTransaction {
    pub fn changes(&self) -> &[Change] {
        &self.changes
    }

    pub fn read_keys(&self) -> impl Iterator<Item = &StateKey> {
        self.changes.iter().map(|change| &change.key)
    }

    pub fn entity_id(&self) -> EntityId {
        match &self.operation {
            Operation::Spawn { after, .. } => after.id,
            Operation::Replace { after, .. } => after.id,
            Operation::Despawn { before, .. } => before.id,
        }
    }

    /// Add another subsystem's exact before/after key to the same atomic WAL
    /// transaction, for example the two block cells occupied by an anchored
    /// machine. Duplicate keys are rejected so conflict checks stay complete.
    pub fn add_related_change(&mut self, change: Change) -> Result<(), EntityError> {
        if self
            .changes
            .iter()
            .any(|existing| existing.key == change.key)
        {
            return Err(EntityError::ConflictingTransactionKey);
        }
        if self.changes.len() >= MAX_ENTITY_TRANSACTION_CHANGES {
            return Err(EntityError::TooManyTransactionChanges);
        }
        self.changes.push(change);
        self.changes.sort_by(|left, right| left.key.cmp(&right.key));
        Ok(())
    }
}

/// Authoritative sparse entity state. All indexes are derived from records and
/// updated only after a caller reports a successful durable receipt.
pub struct EntityStore {
    types: Arc<EntityTypeRegistry>,
    records: BTreeMap<EntityId, EntityRecord>,
    indexes: EntityIndexes,
    next_id: u64,
    revision: u64,
}

impl EntityStore {
    pub fn new(types: Arc<EntityTypeRegistry>) -> Self {
        Self {
            types,
            records: BTreeMap::new(),
            indexes: EntityIndexes::default(),
            next_id: 1,
            revision: 0,
        }
    }

    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub const fn next_id(&self) -> u64 {
        self.next_id
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn snapshot(&self, id: EntityId) -> Option<EntitySnapshot> {
        self.records.get(&id).map(|record| EntitySnapshot {
            id: record.id,
            entity_type: record.entity_type,
            revision: record.revision,
            owner: record.owner,
            location: record.location.clone(),
            private_payload: record.payload.clone(),
            next_tick: record.next_tick,
        })
    }

    pub fn public_view(&self, id: EntityId) -> Option<EntityPublicView> {
        self.records.get(&id).map(EntityRecord::public_view)
    }

    pub fn public_views_for_chunk(&self, chunk: ChunkKey) -> Vec<EntityPublicView> {
        self.indexes
            .chunks
            .get(&chunk)
            .into_iter()
            .flat_map(|page| page.entity_ids.iter())
            .filter_map(|id| self.public_view(*id))
            .collect()
    }

    pub fn due_entities(&self, through_tick: u64, maximum: usize) -> Vec<EntityId> {
        self.indexes.due(through_tick, maximum)
    }

    pub fn query_mobile_aabb(
        &self,
        min: [f32; 3],
        max: [f32; 3],
    ) -> Result<Vec<EntityId>, EntityError> {
        self.indexes.mobile_query(min, max)
    }

    pub fn owner(&self, id: EntityId) -> Option<EntityOwner> {
        self.records.get(&id).map(|record| record.owner)
    }

    pub fn prepare_spawn(
        &self,
        spawn: EntitySpawn,
    ) -> Result<PreparedEntityTransaction, EntityError> {
        self.ensure_revision_room()?;
        if self.records.len() >= MAX_ENTITY_RECORDS {
            return Err(EntityError::TooManyEntities);
        }
        let id = EntityId::new(self.next_id).ok_or(EntityError::IdExhausted)?;
        let next_id = self
            .next_id
            .checked_add(1)
            .ok_or(EntityError::IdExhausted)?;
        let (entity_type, location, payload, spawn_tick) = match spawn {
            EntitySpawn::Mobile {
                entity_type,
                position,
                payload,
                spawn_tick,
            } => (
                entity_type,
                EntityLocation::Mobile { position },
                payload,
                spawn_tick,
            ),
            EntitySpawn::Anchored {
                entity_type,
                anchor,
                anchor_state,
                footprint,
                payload,
                spawn_tick,
            } => (
                entity_type,
                EntityLocation::Anchored {
                    anchor,
                    anchor_state,
                    footprint,
                },
                payload,
                spawn_tick,
            ),
        };
        let descriptor = self.types.descriptor(entity_type)?;
        let location = canonical_location(location, descriptor)?;
        let owner = location.owner()?;
        let next_tick = descriptor.tick_policy().first_tick(spawn_tick)?;
        let payload_size = descriptor.encode_payload(&payload)?.len();
        let public_view = descriptor.public_view(&payload)?;
        let after = EntityRecord {
            id,
            entity_type,
            schema_version: descriptor.schema_version(),
            schema_fingerprint: descriptor.schema_fingerprint(),
            owner,
            revision: 1,
            location,
            payload,
            payload_size,
            public_view,
            next_tick,
        };
        self.validate_record(&after, descriptor)?;
        self.indexes.preview_change(None, Some(&after))?;
        let mut transaction = self.prepare_change(Operation::Spawn {
            after,
            expected_allocator: self.next_id,
        })?;
        transaction.add_related_change(Change::new(
            allocator_state_key(),
            encode_allocator_value(self.next_id)?,
            encode_allocator_value(next_id)?,
        ))?;
        Ok(transaction)
    }

    pub fn prepare_update(
        &self,
        id: EntityId,
        expected_revision: u64,
        patch: EntityPatch,
    ) -> Result<PreparedEntityTransaction, EntityError> {
        self.ensure_revision_room()?;
        let before = self.expected(id, expected_revision)?;
        let descriptor = self.types.descriptor(before.entity_type)?;
        let mut after = before.clone();
        if let Some(position) = patch.position {
            let EntityLocation::Mobile { .. } = before.location else {
                return Err(EntityError::WrongOwnership);
            };
            let location = EntityLocation::Mobile { position };
            let owner = location.owner()?;
            if owner != before.owner {
                return Err(EntityError::TransferRequired);
            }
            after.location = location;
            after.owner = owner;
        }
        if let Some(payload) = patch.payload {
            let encoded = descriptor.encode_payload(&payload)?;
            let previous = descriptor.encode_payload(&after.payload)?;
            if encoded != previous {
                after.payload_size = encoded.len();
                after.public_view = descriptor.public_view(&payload)?;
                after.payload = payload;
            }
        }
        if let Some(next_tick) = patch.next_tick {
            if !descriptor.tick_policy().validates(next_tick) {
                return Err(EntityError::InvalidType);
            }
            after.next_tick = next_tick;
        }
        if after.location == before.location
            && after.payload.same_instance(&before.payload)
            && after.next_tick == before.next_tick
        {
            return Err(EntityError::NoChanges);
        }
        after.revision = before
            .revision
            .checked_add(1)
            .ok_or(EntityError::RevisionExhausted)?;
        self.validate_record(&after, descriptor)?;
        self.indexes.preview_change(Some(&before), Some(&after))?;
        self.prepare_change(Operation::Replace {
            before,
            after,
            transferred: false,
        })
    }

    /// Prepare a barrier transfer of a mobile entity across owner chunks.
    pub fn prepare_transfer(
        &self,
        id: EntityId,
        expected_revision: u64,
        position: [f32; 3],
    ) -> Result<PreparedEntityTransaction, EntityError> {
        self.ensure_revision_room()?;
        let before = self.expected(id, expected_revision)?;
        let descriptor = self.types.descriptor(before.entity_type)?;
        if !matches!(descriptor.ownership(), EntityOwnership::Mobile) {
            return Err(EntityError::WrongOwnership);
        }
        let location = EntityLocation::Mobile { position };
        let owner = location.owner()?;
        if owner == before.owner {
            return Err(EntityError::NotTransfer);
        }
        let mut after = before.clone();
        after.location = location;
        after.owner = owner;
        after.revision = before
            .revision
            .checked_add(1)
            .ok_or(EntityError::RevisionExhausted)?;
        self.validate_record(&after, descriptor)?;
        self.indexes.preview_change(Some(&before), Some(&after))?;
        self.prepare_change(Operation::Replace {
            before,
            after,
            transferred: true,
        })
    }

    pub fn prepare_anchor_update(
        &self,
        id: EntityId,
        expected_revision: u64,
        update: AnchorUpdate,
        patch: EntityPatch,
    ) -> Result<PreparedEntityTransaction, EntityError> {
        self.ensure_revision_room()?;
        let before = self.expected(id, expected_revision)?;
        let descriptor = self.types.descriptor(before.entity_type)?;
        if !matches!(descriptor.ownership(), EntityOwnership::Anchored { .. }) {
            return Err(EntityError::WrongOwnership);
        }
        if patch.position.is_some() {
            return Err(EntityError::WrongOwnership);
        }
        let mut after = before.clone();
        after.location = canonical_location(
            EntityLocation::Anchored {
                anchor: update.anchor,
                anchor_state: update.anchor_state,
                footprint: update.footprint,
            },
            descriptor,
        )?;
        after.owner = after.location.owner()?;
        if let Some(payload) = patch.payload {
            let encoded = descriptor.encode_payload(&payload)?;
            let previous = descriptor.encode_payload(&after.payload)?;
            if encoded != previous {
                after.payload_size = encoded.len();
                after.public_view = descriptor.public_view(&payload)?;
                after.payload = payload;
            }
        }
        if let Some(next_tick) = patch.next_tick {
            if !descriptor.tick_policy().validates(next_tick) {
                return Err(EntityError::InvalidType);
            }
            after.next_tick = next_tick;
        }
        if after.location == before.location
            && after.payload.same_instance(&before.payload)
            && after.next_tick == before.next_tick
        {
            return Err(EntityError::NoChanges);
        }
        after.revision = before
            .revision
            .checked_add(1)
            .ok_or(EntityError::RevisionExhausted)?;
        self.validate_record(&after, descriptor)?;
        self.indexes.preview_change(Some(&before), Some(&after))?;
        self.prepare_change(Operation::Replace {
            before,
            after,
            transferred: false,
        })
    }

    pub fn prepare_despawn(
        &self,
        id: EntityId,
        expected_revision: u64,
    ) -> Result<PreparedEntityTransaction, EntityError> {
        self.ensure_revision_room()?;
        let before = self.expected(id, expected_revision)?;
        let removal_revision = before
            .revision
            .checked_add(1)
            .ok_or(EntityError::RevisionExhausted)?;
        self.indexes.preview_change(Some(&before), None)?;
        self.prepare_change(Operation::Despawn {
            before,
            removal_revision,
        })
    }

    /// Checks every entity-owned WAL precondition before a transaction is
    /// submitted. The runtime repeats this check after receipt before applying.
    pub fn validate_prepared(
        &self,
        transaction: &PreparedEntityTransaction,
    ) -> Result<(), EntityError> {
        self.ensure_revision_room()?;
        for change in &transaction.changes {
            let Some(current) = self.value_for_key(&change.key)? else {
                continue;
            };
            if current != change.before {
                return Err(EntityError::InvalidTransaction);
            }
        }
        match &transaction.operation {
            Operation::Spawn {
                after,
                expected_allocator,
            } => {
                if self.records.contains_key(&after.id) || self.next_id != *expected_allocator {
                    return Err(EntityError::InvalidTransaction);
                }
                let descriptor = self.types.descriptor(after.entity_type)?;
                self.validate_record(after, descriptor)?;
                self.indexes.preview_change(None, Some(after))?;
            }
            Operation::Replace { before, after, .. } => {
                let current = self.records.get(&before.id);
                if current != Some(before) {
                    return Err(EntityError::StaleRevision {
                        id: before.id,
                        expected: before.revision,
                        actual: current.map(|record| record.revision),
                    });
                }
                let descriptor = self.types.descriptor(after.entity_type)?;
                self.validate_record(after, descriptor)?;
                self.indexes.preview_change(Some(before), Some(after))?;
            }
            Operation::Despawn { before, .. } => {
                let current = self.records.get(&before.id);
                if current != Some(before) {
                    return Err(EntityError::StaleRevision {
                        id: before.id,
                        expected: before.revision,
                        actual: current.map(|record| record.revision),
                    });
                }
                self.indexes.preview_change(Some(before), None)?;
            }
        }
        Ok(())
    }

    /// Applies an already synced WAL transaction. Internal inconsistency is
    /// returned to the runtime, which must stop before publication.
    pub fn apply_committed(
        &mut self,
        transaction: PreparedEntityTransaction,
    ) -> Result<EntityCommit, EntityError> {
        self.validate_prepared(&transaction)?;
        let registry_revision = self
            .revision
            .checked_add(1)
            .ok_or(EntityError::RevisionExhausted)?;
        let delta = match transaction.operation {
            Operation::Spawn { after, .. } => {
                self.indexes.replace(None, Some(&after))?;
                self.next_id = after
                    .id
                    .get()
                    .checked_add(1)
                    .ok_or(EntityError::IdExhausted)?;
                self.records.insert(after.id, after.clone());
                EntityDelta::Spawned(after.public_view())
            }
            Operation::Replace {
                before,
                after,
                transferred,
            } => {
                self.indexes.replace(Some(&before), Some(&after))?;
                self.records.insert(after.id, after.clone());
                if transferred || before.owner != after.owner {
                    EntityDelta::Transferred {
                        before_owner: before.owner,
                        view: after.public_view(),
                    }
                } else {
                    EntityDelta::Updated(after.public_view())
                }
            }
            Operation::Despawn {
                before,
                removal_revision,
            } => {
                self.indexes.replace(Some(&before), None)?;
                self.records.remove(&before.id);
                EntityDelta::Despawned {
                    id: before.id,
                    entity_type: before.entity_type,
                    revision: removal_revision,
                    owner: before.owner,
                }
            }
        };
        self.revision = registry_revision;
        Ok(EntityCommit {
            registry_revision,
            deltas: vec![delta],
        })
    }

    pub fn ids_for_chunk(&self, chunk: ChunkKey) -> Vec<EntityId> {
        self.indexes
            .chunks
            .get(&chunk)
            .map(|page| page.entity_ids.iter().copied().collect())
            .unwrap_or_default()
    }

    pub fn record_values(&self) -> impl Iterator<Item = &EntityRecord> {
        self.records.values()
    }

    pub fn chunk_pages(&self) -> &BTreeMap<ChunkKey, super::spatial::ChunkPage> {
        &self.indexes.chunks
    }

    pub fn indexes(&self) -> &EntityIndexes {
        &self.indexes
    }

    pub fn types(&self) -> &EntityTypeRegistry {
        &self.types
    }

    pub fn from_parts(
        types: Arc<EntityTypeRegistry>,
        next_id: u64,
        revision: u64,
        records: BTreeMap<EntityId, EntityRecord>,
    ) -> Result<Self, EntityError> {
        if next_id == 0 || records.len() > MAX_ENTITY_RECORDS {
            return Err(EntityError::CorruptCheckpoint);
        }
        let max_id = records.keys().next_back().map_or(0, |id| id.get());
        if next_id <= max_id {
            return Err(EntityError::CorruptCheckpoint);
        }
        if records.values().any(|record| record.revision > revision) {
            return Err(EntityError::CorruptCheckpoint);
        }
        let mut store = Self::new(types);
        store.next_id = next_id;
        store.revision = revision;
        store.records = records;
        for record in store.records.values() {
            let descriptor = store
                .types
                .descriptor(record.entity_type)
                .map_err(|_| EntityError::UnknownRequiredType(record.entity_type))?;
            store.validate_record(record, descriptor)?;
            store.indexes.insert(record)?;
        }
        store.indexes.validate_against_records(&store.records)?;
        Ok(store)
    }

    pub fn apply_journal_overlay(
        &mut self,
        values: &BTreeMap<StateKey, Vec<u8>>,
    ) -> Result<bool, EntityError> {
        let mut owner_values = BTreeMap::new();
        let mut chunk_values = BTreeMap::new();
        let mut cell_values = BTreeMap::new();
        let mut allocator = None;
        for (key, value) in values {
            match key.domain.as_str() {
                ENTITY_RECORD_DOMAIN => {
                    if key.bytes.len() != 8 {
                        return Err(EntityError::CorruptCheckpoint);
                    }
                    let id = EntityId::new(u64::from_le_bytes(
                        key.bytes
                            .as_slice()
                            .try_into()
                            .map_err(|_| EntityError::CorruptCheckpoint)?,
                    ))
                    .ok_or(EntityError::CorruptCheckpoint)?;
                    owner_values.insert(id, value.clone());
                }
                ENTITY_ALLOCATOR_DOMAIN => {
                    if !key.bytes.is_empty() {
                        return Err(EntityError::CorruptCheckpoint);
                    }
                    allocator = Some(super::persistence::decode_allocator_value(value)?);
                }
                ENTITY_CHUNK_DOMAIN => {
                    chunk_values.insert(decode_chunk_key(&key.bytes)?, value.clone());
                }
                ENTITY_CELL_DOMAIN => {
                    cell_values.insert(decode_cell_key(&key.bytes)?, value.clone());
                }
                domain if domain.starts_with("bloxgloom:entity") => {
                    return Err(EntityError::CorruptCheckpoint);
                }
                _ => {}
            }
        }
        if owner_values.is_empty()
            && chunk_values.is_empty()
            && cell_values.is_empty()
            && allocator.is_none()
        {
            return Ok(false);
        }
        let mut records = self.records.clone();
        for (id, value) in owner_values {
            if value.is_empty() {
                records.remove(&id);
            } else {
                let record = super::persistence::decode_record_value(id, &value, &self.types)?;
                records.insert(id, record);
            }
        }
        let next_id = allocator.unwrap_or(self.next_id);
        // An overlay is materialized as a final map rather than replayed as a
        // commit stream, so its exact global revision count is unavailable.
        // Preserve the checkpoint revision and raise it to at least every
        // live entity revision, which keeps subsequent public revisions
        // monotonic after recovery.
        let revision = records
            .values()
            .map(|record| record.revision)
            .max()
            .unwrap_or(self.revision)
            .max(self.revision);
        let recovered = Self::from_parts(self.types.clone(), next_id, revision, records)?;
        for (chunk, bytes) in chunk_values {
            let expected = recovered
                .indexes
                .chunks
                .get(&chunk)
                .cloned()
                .unwrap_or_default()
                .encode_value()?;
            if expected != bytes {
                return Err(EntityError::CorruptCheckpoint);
            }
        }
        for (cell, bytes) in cell_values {
            let expected = encode_cell_owner(recovered.indexes.anchored_cells.get(&cell).copied())?;
            if expected != bytes {
                return Err(EntityError::CorruptCheckpoint);
            }
        }
        let changed = !same_persisted_records(&self.records, &recovered.records, &self.types)?
            || self.next_id != recovered.next_id
            || self.revision != recovered.revision;
        *self = recovered;
        Ok(changed)
    }

    fn prepare_change(
        &self,
        operation: Operation,
    ) -> Result<PreparedEntityTransaction, EntityError> {
        let (before, after) = match &operation {
            Operation::Spawn { after, .. } => (None, Some(after)),
            Operation::Replace { before, after, .. } => (Some(before), Some(after)),
            Operation::Despawn { before, .. } => (Some(before), None),
        };
        let id = before
            .map(|record| record.id)
            .or_else(|| after.map(|record| record.id))
            .ok_or(EntityError::InvalidTransaction)?;
        let before_bytes = before
            .map(|record| encode_record_value(record, &self.types))
            .transpose()?
            .unwrap_or_default();
        let after_bytes = after
            .map(|record| encode_record_value(record, &self.types))
            .transpose()?
            .unwrap_or_default();
        let mut changes = vec![Change::new(entity_state_key(id), before_bytes, after_bytes)];
        let (chunk_pages, cells) = self.indexes.preview_change(before, after)?;
        for (chunk, before, after) in chunk_pages {
            changes.push(Change::new(chunk_state_key(chunk), before, after));
        }
        for (cell, before, after) in cells {
            changes.push(Change::new(cell_state_key(cell), before, after));
        }
        changes.sort_by(|left, right| left.key.cmp(&right.key));
        if changes.len() > MAX_ENTITY_TRANSACTION_CHANGES {
            return Err(EntityError::TooManyTransactionChanges);
        }
        Ok(PreparedEntityTransaction { operation, changes })
    }

    fn expected(&self, id: EntityId, expected_revision: u64) -> Result<EntityRecord, EntityError> {
        let record = self
            .records
            .get(&id)
            .ok_or(EntityError::UnknownEntity(id))?;
        if record.revision != expected_revision {
            return Err(EntityError::StaleRevision {
                id,
                expected: expected_revision,
                actual: Some(record.revision),
            });
        }
        Ok(record.clone())
    }

    fn validate_record(
        &self,
        record: &EntityRecord,
        descriptor: &EntityTypeDescriptor,
    ) -> Result<(), EntityError> {
        if record.id.get() == 0
            || record.entity_type != descriptor.id()
            || record.schema_version != descriptor.schema_version()
            || record.schema_fingerprint != descriptor.schema_fingerprint()
            || record.revision == 0
            || !descriptor.tick_policy().validates(record.next_tick)
        {
            return Err(EntityError::InvalidType);
        }
        validate_ownership_mode(&record.location, descriptor.ownership())?;
        validate_location_owner(&record.location, record.owner)?;
        if descriptor.encode_payload(&record.payload)?.len() != record.payload_size
            || descriptor.public_view(&record.payload)? != record.public_view
        {
            return Err(EntityError::InvalidPayload);
        }
        Ok(())
    }

    fn value_for_key(&self, key: &StateKey) -> Result<Option<Vec<u8>>, EntityError> {
        match key.domain.as_str() {
            ENTITY_RECORD_DOMAIN => {
                if key.bytes.len() != 8 {
                    return Err(EntityError::InvalidTransaction);
                }
                let id = EntityId::new(u64::from_le_bytes(
                    key.bytes
                        .as_slice()
                        .try_into()
                        .map_err(|_| EntityError::InvalidTransaction)?,
                ))
                .ok_or(EntityError::InvalidTransaction)?;
                Ok(Some(
                    self.records
                        .get(&id)
                        .map(|record| encode_record_value(record, &self.types))
                        .transpose()?
                        .unwrap_or_default(),
                ))
            }
            ENTITY_ALLOCATOR_DOMAIN => {
                if !key.bytes.is_empty() {
                    return Err(EntityError::InvalidTransaction);
                }
                Ok(Some(encode_allocator_value(self.next_id)?))
            }
            ENTITY_CHUNK_DOMAIN => {
                let chunk = decode_chunk_key(&key.bytes)?;
                self.indexes
                    .chunks
                    .get(&chunk)
                    .cloned()
                    .unwrap_or_default()
                    .encode_value()
                    .map(Some)
            }
            ENTITY_CELL_DOMAIN => {
                let cell = decode_cell_key(&key.bytes)?;
                Ok(Some(encode_cell_owner(
                    self.indexes.anchored_cells.get(&cell).copied(),
                )?))
            }
            _ => Ok(None),
        }
    }

    fn ensure_revision_room(&self) -> Result<(), EntityError> {
        self.revision
            .checked_add(1)
            .map(|_| ())
            .ok_or(EntityError::RevisionExhausted)
    }
}

fn same_persisted_records(
    left: &BTreeMap<EntityId, EntityRecord>,
    right: &BTreeMap<EntityId, EntityRecord>,
    types: &EntityTypeRegistry,
) -> Result<bool, EntityError> {
    if left.len() != right.len() {
        return Ok(false);
    }
    for (id, left_record) in left {
        let Some(right_record) = right.get(id) else {
            return Ok(false);
        };
        if encode_record_value(left_record, types)? != encode_record_value(right_record, types)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn canonical_location(
    mut location: EntityLocation,
    descriptor: &EntityTypeDescriptor,
) -> Result<EntityLocation, EntityError> {
    if let EntityLocation::Anchored { footprint, .. } = &mut location {
        footprint.sort_unstable();
        if footprint.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(EntityError::InvalidLocation);
        }
    }
    validate_ownership_mode(&location, descriptor.ownership())?;
    match &location {
        EntityLocation::Mobile { position } => {
            position_to_cell(*position)?;
        }
        EntityLocation::Anchored { anchor, .. } => {
            let _ = anchor.chunk();
        }
    }
    Ok(location)
}

fn entity_state_key(id: EntityId) -> StateKey {
    StateKey::new(ENTITY_RECORD_DOMAIN, id.get().to_le_bytes().to_vec())
}

fn allocator_state_key() -> StateKey {
    StateKey::new(ENTITY_ALLOCATOR_DOMAIN, Vec::new())
}

fn chunk_state_key(chunk: ChunkKey) -> StateKey {
    StateKey::new(ENTITY_CHUNK_DOMAIN, encode_chunk_key(chunk))
}

fn cell_state_key(cell: CellCoord) -> StateKey {
    StateKey::new(ENTITY_CELL_DOMAIN, encode_cell_key(cell))
}
