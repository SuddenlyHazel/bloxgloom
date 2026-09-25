use super::persistence::{
    encode_allocator_value, encode_durable_record_value, encode_motion_value, encode_record_value,
    encode_revision_value,
};
use super::registry::{EntityTypeDescriptor, EntityTypeRegistry};
use super::spatial::{
    EntityIndexes, decode_cell_key, decode_chunk_key, encode_cell_key, encode_cell_owner,
    encode_chunk_key, validate_location_owner, validate_ownership_mode,
};
use super::types::{
    AnchorUpdate, CellCoord, EntityError, EntityId, EntityLocation, EntityMotionSnapshot,
    EntityOwner, EntityOwnership, EntityPayload, EntityPublicView, TRANSIENT_ENTITY_ID_BIT,
    position_to_cell,
};
use crate::content::{BlockStateId, EntityTypeId};
use crate::server::journal::{Change, StateKey};
use crate::world::ChunkKey;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub const ENTITY_RECORD_DOMAIN: &str = "bloxgloom:entity";
pub const ENTITY_MOTION_DOMAIN: &str = "bloxgloom:entity_motion";
pub const ENTITY_ALLOCATOR_DOMAIN: &str = "bloxgloom:entity_allocator";
pub const ENTITY_REVISION_DOMAIN: &str = "bloxgloom:entity_revision";
pub const ENTITY_CHUNK_DOMAIN: &str = "bloxgloom:entity_chunk";
pub const ENTITY_CELL_DOMAIN: &str = "bloxgloom:entity_cell";
pub const MAX_ENTITY_RECORDS: usize = 1_048_576;
pub const MAX_ENTITY_TRANSACTION_CHANGES: usize = 16_384;
pub const MAX_ENTITY_TRANSACTION_BYTES: usize = 1_000_000;
pub const MAX_ENTITY_SPAWN_BATCH: usize = 4_096;

#[derive(Clone, Debug)]
pub struct EntityRecord {
    pub id: EntityId,
    pub entity_type: EntityTypeId,
    pub schema_version: u16,
    pub schema_fingerprint: u64,
    pub owner: EntityOwner,
    pub revision: u64,
    pub motion_revision: u64,
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
            && self.motion_revision == other.motion_revision
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
            motion_revision: self.motion_revision,
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
    pub motion_revision: u64,
    pub owner: EntityOwner,
    pub location: EntityLocation,
    pub private_payload: EntityPayload,
    pub next_tick: Option<u64>,
}

impl EntitySnapshot {
    pub fn anchor(&self) -> Option<CellCoord> {
        match &self.location {
            EntityLocation::Anchored { anchor, .. } => Some(*anchor),
            EntityLocation::Mobile { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum EntityDelta {
    Spawned(EntityPublicView),
    Updated {
        before_touched_chunks: Vec<ChunkKey>,
        view: EntityPublicView,
    },
    Transferred {
        before_owner: EntityOwner,
        before_touched_chunks: Vec<ChunkKey>,
        view: EntityPublicView,
    },
    Moved(EntityPublicView),
    Despawned {
        id: EntityId,
        entity_type: EntityTypeId,
        revision: u64,
        owner: EntityOwner,
        touched_chunks: Vec<ChunkKey>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct EntityCommit {
    pub registry_revision: u64,
    pub deltas: Vec<EntityDelta>,
}

#[derive(Clone, Debug)]
enum Operation {
    SpawnBatch {
        after: Vec<EntityRecord>,
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
    Batch {
        operations: Vec<Operation>,
    },
}

/// Full-key prepared mutation. The caller combines `changes()` with linked
/// block, inventory, or item-ownership changes before submitting one WAL record.
#[derive(Clone, Debug)]
pub struct PreparedEntityTransaction {
    operation: Operation,
    changes: Vec<Change>,
    additional_read_keys: Vec<StateKey>,
}

impl PreparedEntityTransaction {
    pub fn changes(&self) -> &[Change] {
        &self.changes
    }

    pub fn read_keys(&self) -> impl Iterator<Item = &StateKey> {
        self.changes
            .iter()
            .map(|change| &change.key)
            .chain(self.additional_read_keys.iter())
    }

    pub fn entity_id(&self) -> EntityId {
        self.entity_ids()[0]
    }

    pub fn entity_ids(&self) -> Vec<EntityId> {
        operation_entity_ids(&self.operation)
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
        let mut candidate = self.changes.clone();
        candidate.push(change);
        candidate.sort_by(|left, right| left.key.cmp(&right.key));
        validate_transaction_size(&candidate)?;
        self.changes = candidate;
        Ok(())
    }

    /// Reserve a read-only precondition key while the prepared change is
    /// pending. These keys are admission fences, not WAL state transitions.
    pub fn add_read_key(&mut self, key: StateKey) {
        if !self.changes.iter().any(|change| change.key == key)
            && !self.additional_read_keys.contains(&key)
        {
            self.additional_read_keys.push(key);
            self.additional_read_keys.sort();
        }
    }
}

/// A set of entity changes prepared against one store revision and committed
/// atomically with one coalesced full-key WAL change set.
pub type PreparedEntityBatch = PreparedEntityTransaction;

/// Authoritative sparse entity state. All indexes are derived from records and
/// updated only after a caller reports a successful durable receipt.
pub struct EntityStore {
    types: Arc<EntityTypeRegistry>,
    records: BTreeMap<EntityId, EntityRecord>,
    indexes: EntityIndexes,
    next_id: u64,
    revision: u64,
    durable_sequence: u64,
    durable_global_revision: u64,
    motion_fences: BTreeMap<EntityId, u64>,
}

impl EntityStore {
    pub fn new(types: Arc<EntityTypeRegistry>) -> Self {
        let indexes = EntityIndexes::with_tick_types(types.tickable_types());
        Self {
            types,
            records: BTreeMap::new(),
            indexes,
            next_id: 1,
            revision: 0,
            durable_sequence: 0,
            durable_global_revision: 0,
            motion_fences: BTreeMap::new(),
        }
    }

    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub const fn durable_sequence(&self) -> u64 {
        self.durable_sequence
    }

    pub const fn durable_global_revision(&self) -> u64 {
        self.durable_global_revision
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
            motion_revision: record.motion_revision,
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

    /// Clone at most `limit` public views. A page with more references fails
    /// closed before any unbounded response allocation can occur.
    pub fn public_views_for_chunk_bounded(
        &self,
        chunk: ChunkKey,
        limit: usize,
    ) -> Result<Vec<EntityPublicView>, EntityError> {
        let Some(page) = self.indexes.chunks.get(&chunk) else {
            return Ok(Vec::new());
        };
        if page.entity_ids.len() > limit {
            return Err(EntityError::SpatialQueryTooBroad);
        }
        let mut views = Vec::with_capacity(page.entity_ids.len());
        for id in &page.entity_ids {
            let record = self
                .records
                .get(id)
                .ok_or(EntityError::InvalidTransaction)?;
            views.push(record.public_view());
        }
        Ok(views)
    }

    pub fn due_entities(&self, through_tick: u64, maximum: usize) -> Vec<EntityId> {
        self.indexes.due(through_tick, maximum)
    }

    pub fn due_tick_entries(
        &self,
        through_tick: u64,
        after: Option<(u64, EntityId)>,
        maximum: usize,
    ) -> Vec<(u64, EntityId)> {
        self.indexes.due_tick_entries(through_tick, after, maximum)
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

    pub fn anchored_at(&self, cell: CellCoord) -> Option<EntityId> {
        self.indexes.anchored_cells.get(&cell).copied()
    }

    pub fn mobile_motion_snapshot(&self, id: EntityId) -> Option<EntityMotionSnapshot> {
        let record = self.records.get(&id)?;
        let EntityLocation::Mobile { position } = record.location else {
            return None;
        };
        Some(EntityMotionSnapshot {
            id,
            revision: record.motion_revision,
            position,
        })
    }

    /// Applies a checkpoint-owned same-chunk mobile movement. Cross-chunk
    /// movement must be represented by `prepare_transfer`, which fences this
    /// entity until the WAL receipt commits or the caller cancels it.
    pub fn update_mobile_motion(
        &mut self,
        id: EntityId,
        expected_motion_revision: u64,
        position: [f32; 3],
    ) -> Result<EntityCommit, EntityError> {
        if self.motion_fences.contains_key(&id) {
            return Err(EntityError::MotionFenced);
        }
        let before = self
            .records
            .get(&id)
            .cloned()
            .ok_or(EntityError::UnknownEntity(id))?;
        if before.motion_revision != expected_motion_revision {
            return Err(EntityError::StaleMotionRevision {
                id,
                expected: expected_motion_revision,
                actual: Some(before.motion_revision),
            });
        }
        if !matches!(&before.location, EntityLocation::Mobile { .. }) {
            return Err(EntityError::WrongOwnership);
        }
        let location = EntityLocation::Mobile { position };
        let owner = location.owner()?;
        if owner != before.owner {
            return Err(EntityError::TransferRequired);
        }
        if location == before.location {
            return Err(EntityError::NoChanges);
        }
        let mut after = before.clone();
        after.location = location;
        after.motion_revision = before
            .motion_revision
            .checked_add(1)
            .ok_or(EntityError::RevisionExhausted)?;
        let descriptor = self.types.descriptor(before.entity_type)?;
        self.validate_record(&after, descriptor)?;
        self.indexes.preview_change(Some(&before), Some(&after))?;
        self.indexes.replace(Some(&before), Some(&after))?;
        self.records.insert(id, after.clone());
        Ok(EntityCommit {
            registry_revision: self.revision,
            deltas: vec![EntityDelta::Moved(after.public_view())],
        })
    }

    pub fn prepare_spawn(
        &self,
        spawn: EntitySpawn,
    ) -> Result<PreparedEntityTransaction, EntityError> {
        self.prepare_spawn_batch(vec![spawn])
    }

    /// Prepare a deterministic group of spawns under one allocator and one
    /// coalesced set of sparse index page writes. All records become visible
    /// together after the enclosing WAL receipt.
    pub fn prepare_spawn_batch(
        &self,
        spawns: Vec<EntitySpawn>,
    ) -> Result<PreparedEntityTransaction, EntityError> {
        self.ensure_revision_room()?;
        if spawns.is_empty() || spawns.len() > MAX_ENTITY_SPAWN_BATCH {
            return Err(EntityError::TooManyEntities);
        }
        if self.records.len().saturating_add(spawns.len()) > MAX_ENTITY_RECORDS {
            return Err(EntityError::TooManyEntities);
        }
        let next_id = self
            .next_id
            .checked_add(u64::try_from(spawns.len()).map_err(|_| EntityError::IdExhausted)?)
            .filter(|next| *next != 0 && *next <= TRANSIENT_ENTITY_ID_BIT)
            .ok_or(EntityError::IdExhausted)?;
        let mut records = Vec::with_capacity(spawns.len());
        let mut projected_indexes = self.indexes.clone();
        let mut changes = Vec::with_capacity(spawns.len() * 2 + 1);
        for (offset, spawn) in spawns.into_iter().enumerate() {
            let id_value = self
                .next_id
                .checked_add(u64::try_from(offset).map_err(|_| EntityError::IdExhausted)?)
                .ok_or(EntityError::IdExhausted)?;
            let id = EntityId::new(id_value).ok_or(EntityError::IdExhausted)?;
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
                motion_revision: if matches!(&location, EntityLocation::Mobile { .. }) {
                    1
                } else {
                    0
                },
                location,
                payload,
                payload_size,
                public_view,
                next_tick,
            };
            self.validate_record(&after, descriptor)?;
            projected_indexes.insert(&after)?;
            changes.push(Change::new(
                entity_state_key(id),
                Vec::new(),
                encode_durable_record_value(&after, &self.types)?,
            ));
            if matches!(&after.location, EntityLocation::Mobile { .. }) {
                changes.push(Change::new(
                    motion_state_key(id),
                    Vec::new(),
                    encode_motion_value(&after)?,
                ));
            }
            records.push(after);
        }

        changes.push(Change::new(
            allocator_state_key(),
            encode_allocator_value(self.next_id)?,
            encode_allocator_value(next_id)?,
        ));
        changes.push(self.durable_revision_change()?);
        let mut touched_chunks = BTreeSet::new();
        for record in &records {
            touched_chunks.extend(record.location.touched_chunks()?);
        }
        for chunk in touched_chunks {
            let before = self
                .indexes
                .chunks
                .get(&chunk)
                .cloned()
                .unwrap_or_default()
                .encode_value()?;
            let after = projected_indexes
                .chunks
                .get(&chunk)
                .cloned()
                .unwrap_or_default()
                .encode_value()?;
            if before != after {
                changes.push(Change::new(chunk_state_key(chunk), before, after));
            }
        }
        let touched_cells: BTreeSet<_> = records
            .iter()
            .filter_map(|record| match &record.location {
                EntityLocation::Anchored { footprint, .. } => Some(footprint.iter().copied()),
                EntityLocation::Mobile { .. } => None,
            })
            .flatten()
            .collect();
        for cell in touched_cells {
            let before = encode_cell_owner(self.indexes.anchored_cells.get(&cell).copied())?;
            let after = encode_cell_owner(projected_indexes.anchored_cells.get(&cell).copied())?;
            if before != after {
                changes.push(Change::new(cell_state_key(cell), before, after));
            }
        }
        changes.sort_by(|left, right| left.key.cmp(&right.key));
        if changes.len() > MAX_ENTITY_TRANSACTION_CHANGES {
            return Err(EntityError::TooManyTransactionChanges);
        }
        validate_transaction_size(&changes)?;
        let transaction = PreparedEntityTransaction {
            operation: Operation::SpawnBatch {
                after: records,
                expected_allocator: self.next_id,
            },
            changes,
            additional_read_keys: Vec::new(),
        };
        self.validate_prepared(&transaction)?;
        Ok(transaction)
    }

    /// Coalesces independently prepared, disjoint entity transactions into a
    /// single atomic batch. Shared chunk/cell page writes are recomputed from
    /// the initial and final sparse indexes, so two entities in one chunk do
    /// not claim conflicting transitions for the same WAL key.
    pub fn combine_prepared(
        &self,
        transactions: Vec<PreparedEntityTransaction>,
    ) -> Result<PreparedEntityBatch, EntityError> {
        if transactions.is_empty() {
            return Err(EntityError::NoChanges);
        }
        if transactions.len() > MAX_ENTITY_TRANSACTION_CHANGES {
            return Err(EntityError::TooManyTransactionChanges);
        }
        let mut operations = Vec::new();
        let mut related_changes = BTreeMap::new();
        let mut additional_read_keys = BTreeSet::new();
        for transaction in transactions {
            self.validate_prepared(&transaction)?;
            additional_read_keys.extend(transaction.additional_read_keys);
            collect_operations(transaction.operation, &mut operations);
            for change in transaction.changes {
                if is_entity_state_domain(&change.key.domain) {
                    continue;
                }
                if change.key.domain.starts_with("bloxgloom:entity") {
                    return Err(EntityError::InvalidTransaction);
                }
                if related_changes.insert(change.key.clone(), change).is_some() {
                    return Err(EntityError::ConflictingTransactionKey);
                }
            }
        }
        if operations.is_empty() || operations.len() > MAX_ENTITY_TRANSACTION_CHANGES {
            return Err(EntityError::TooManyTransactionChanges);
        }
        if additional_read_keys.len() > MAX_ENTITY_TRANSACTION_CHANGES {
            return Err(EntityError::TooManyTransactionChanges);
        }
        let mut ids = BTreeSet::new();
        let mut spawn_batches = 0usize;
        for operation in &operations {
            for id in operation_entity_ids(operation) {
                if !ids.insert(id) {
                    return Err(EntityError::InvalidTransaction);
                }
            }
            spawn_batches += usize::from(matches!(operation, Operation::SpawnBatch { .. }));
        }
        if spawn_batches > 1 {
            return Err(EntityError::InvalidTransaction);
        }
        operations.sort_by_key(|operation| operation_entity_ids(operation)[0]);

        let mut projected_records = self.records.clone();
        let mut projected_indexes = self.indexes.clone();
        let mut projected_next_id = self.next_id;
        for operation in &operations {
            apply_operation_to_projection(
                &self.types,
                &self.motion_fences,
                operation,
                &mut projected_records,
                &mut projected_indexes,
                &mut projected_next_id,
            )?;
        }
        projected_indexes.validate_against_records(&projected_records)?;

        let mut changes: Vec<_> = related_changes.into_values().collect();
        let mut touched_chunks = BTreeSet::new();
        let mut touched_cells = BTreeSet::new();
        for id in &ids {
            let before = self.records.get(id);
            let after = projected_records.get(id);
            changes.push(Change::new(
                entity_state_key(*id),
                before
                    .map(|record| encode_durable_record_value(record, &self.types))
                    .transpose()?
                    .unwrap_or_default(),
                after
                    .map(|record| encode_durable_record_value(record, &self.types))
                    .transpose()?
                    .unwrap_or_default(),
            ));
            if let Some(record) = before {
                touched_chunks.extend(record.location.touched_chunks()?);
                if let EntityLocation::Anchored { footprint, .. } = &record.location {
                    touched_cells.extend(footprint.iter().copied());
                }
            }
            if let Some(record) = after {
                touched_chunks.extend(record.location.touched_chunks()?);
                if let EntityLocation::Anchored { footprint, .. } = &record.location {
                    touched_cells.extend(footprint.iter().copied());
                }
            }

            let before_motion =
                before.filter(|record| matches!(&record.location, EntityLocation::Mobile { .. }));
            let after_motion =
                after.filter(|record| matches!(&record.location, EntityLocation::Mobile { .. }));
            let motion_changed = match (before_motion, after_motion) {
                (None, Some(_)) => true,
                (Some(before), Some(after)) => before.motion_revision != after.motion_revision,
                _ => false,
            };
            if motion_changed {
                let before_bytes = before_motion
                    .map(encode_motion_value)
                    .transpose()?
                    .unwrap_or_default();
                let after_bytes = after_motion
                    .map(encode_motion_value)
                    .transpose()?
                    .unwrap_or_default();
                changes.push(Change::new(
                    motion_state_key(*id),
                    before_bytes,
                    after_bytes,
                ));
            }
        }
        if projected_next_id != self.next_id {
            changes.push(Change::new(
                allocator_state_key(),
                encode_allocator_value(self.next_id)?,
                encode_allocator_value(projected_next_id)?,
            ));
        }
        for chunk in touched_chunks {
            let before = self
                .indexes
                .chunks
                .get(&chunk)
                .cloned()
                .unwrap_or_default()
                .encode_value()?;
            let after = projected_indexes
                .chunks
                .get(&chunk)
                .cloned()
                .unwrap_or_default()
                .encode_value()?;
            if before != after {
                changes.push(Change::new(chunk_state_key(chunk), before, after));
            }
        }
        for cell in touched_cells {
            let before = encode_cell_owner(self.indexes.anchored_cells.get(&cell).copied())?;
            let after = encode_cell_owner(projected_indexes.anchored_cells.get(&cell).copied())?;
            if before != after {
                changes.push(Change::new(cell_state_key(cell), before, after));
            }
        }
        changes.push(self.durable_revision_change()?);
        changes.sort_by(|left, right| left.key.cmp(&right.key));
        if changes.len() > MAX_ENTITY_TRANSACTION_CHANGES {
            return Err(EntityError::TooManyTransactionChanges);
        }
        validate_transaction_size(&changes)?;
        let batch = PreparedEntityBatch {
            operation: Operation::Batch { operations },
            changes,
            additional_read_keys: additional_read_keys.into_iter().collect(),
        };
        self.validate_prepared(&batch)?;
        Ok(batch)
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
            // A WAL-planned move advances the motion revision exactly like a
            // transfer does, so the motion-domain key tracks live motion and
            // a later barrier transfer stages an exact preimage. Without
            // this the record would move while its motion key stayed at the
            // spawn value, and the first crossing would fail closed at the
            // journal worker.
            after.motion_revision = before
                .motion_revision
                .checked_add(1)
                .ok_or(EntityError::RevisionExhausted)?;
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
        let mut transaction = self.prepare_change(Operation::Replace {
            before: before.clone(),
            after: after.clone(),
            transferred: false,
        })?;
        if after.location != before.location {
            transaction.add_related_change(Change::new(
                motion_state_key(id),
                encode_motion_value(&before)?,
                encode_motion_value(&after)?,
            ))?;
        }
        Ok(transaction)
    }

    /// Prepare a barrier transfer of a mobile entity across owner chunks.
    /// The optional patch carries the tick's payload and schedule update in
    /// the same atomic record; its own `position` must stay empty because the
    /// transfer destination arrives in `position`.
    pub fn prepare_transfer(
        &mut self,
        id: EntityId,
        expected_revision: u64,
        position: [f32; 3],
        patch: EntityPatch,
    ) -> Result<PreparedEntityTransaction, EntityError> {
        self.ensure_revision_room()?;
        let before = self.expected(id, expected_revision)?;
        let descriptor = self.types.descriptor(before.entity_type)?;
        if !matches!(descriptor.ownership(), EntityOwnership::Mobile) {
            return Err(EntityError::WrongOwnership);
        }
        if patch.position.is_some() {
            return Err(EntityError::InvalidTransaction);
        }
        if self.motion_fences.contains_key(&id) {
            return Err(EntityError::MotionFenced);
        }
        let location = EntityLocation::Mobile { position };
        let owner = location.owner()?;
        if owner == before.owner {
            return Err(EntityError::NotTransfer);
        }
        let mut after = before.clone();
        after.location = location;
        after.owner = owner;
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
        after.motion_revision = before
            .motion_revision
            .checked_add(1)
            .ok_or(EntityError::RevisionExhausted)?;
        after.revision = before
            .revision
            .checked_add(1)
            .ok_or(EntityError::RevisionExhausted)?;
        self.validate_record(&after, descriptor)?;
        self.indexes.preview_change(Some(&before), Some(&after))?;
        let mut transaction = self.prepare_change(Operation::Replace {
            before,
            after,
            transferred: true,
        })?;
        let (motion_after, fence_revision) = match &transaction.operation {
            Operation::Replace { after, .. } => {
                (encode_motion_value(after)?, after.motion_revision - 1)
            }
            _ => return Err(EntityError::InvalidTransaction),
        };
        let before_motion = encode_motion_value(
            self.records
                .get(&id)
                .ok_or(EntityError::UnknownEntity(id))?,
        )?;
        transaction.add_related_change(Change::new(
            motion_state_key(id),
            before_motion,
            motion_after,
        ))?;
        self.motion_fences.insert(id, fence_revision);
        Ok(transaction)
    }

    /// Releases the checkpoint-motion fence if a prepared transfer is not
    /// submitted or its WAL submission is rejected before a receipt exists.
    pub fn cancel_prepared(&mut self, transaction: &PreparedEntityTransaction) {
        let mut operations = Vec::new();
        collect_operations(transaction.operation.clone(), &mut operations);
        for operation in operations {
            if let Operation::Replace {
                before,
                transferred: true,
                ..
            } = operation
                && self.motion_fences.get(&before.id) == Some(&before.motion_revision)
            {
                self.motion_fences.remove(&before.id);
            }
        }
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
        let revision_changes: Vec<_> = transaction
            .changes
            .iter()
            .filter(|change| change.key.domain == ENTITY_REVISION_DOMAIN)
            .collect();
        if revision_changes.len() != 1
            || revision_changes[0].key.bytes.len() != 0
            || revision_changes[0].after
                != encode_revision_value(
                    self.durable_sequence
                        .checked_add(1)
                        .ok_or(EntityError::RevisionExhausted)?,
                    self.revision
                        .checked_add(1)
                        .ok_or(EntityError::RevisionExhausted)?,
                )?
        {
            return Err(EntityError::InvalidTransaction);
        }
        for change in &transaction.changes {
            let Some(current) = self.value_for_key(&change.key)? else {
                continue;
            };
            if current != change.before {
                return Err(EntityError::InvalidTransaction);
            }
        }
        if let Operation::Batch { operations } = &transaction.operation {
            let mut records = self.records.clone();
            let mut indexes = self.indexes.clone();
            let mut next_id = self.next_id;
            let mut seen = BTreeSet::new();
            for operation in operations {
                for id in operation_entity_ids(operation) {
                    if !seen.insert(id) {
                        return Err(EntityError::InvalidTransaction);
                    }
                }
                apply_operation_to_projection(
                    &self.types,
                    &self.motion_fences,
                    operation,
                    &mut records,
                    &mut indexes,
                    &mut next_id,
                )?;
            }
            indexes.validate_against_records(&records)?;
            return Ok(());
        }
        match &transaction.operation {
            Operation::SpawnBatch {
                after,
                expected_allocator,
            } => {
                if after.is_empty()
                    || after.len() > MAX_ENTITY_SPAWN_BATCH
                    || self.next_id != *expected_allocator
                    || self.records.len().saturating_add(after.len()) > MAX_ENTITY_RECORDS
                    || after.first().map(|record| record.id.get()) != Some(*expected_allocator)
                {
                    return Err(EntityError::InvalidTransaction);
                }
                let mut projected = self.indexes.clone();
                for (offset, record) in after.iter().enumerate() {
                    let expected_id = expected_allocator
                        .checked_add(u64::try_from(offset).map_err(|_| EntityError::IdExhausted)?)
                        .ok_or(EntityError::IdExhausted)?;
                    if record.id.get() != expected_id || self.records.contains_key(&record.id) {
                        return Err(EntityError::InvalidTransaction);
                    }
                    let descriptor = self.types.descriptor(record.entity_type)?;
                    self.validate_record(record, descriptor)?;
                    projected.insert(record)?;
                }
            }
            Operation::Replace { before, after, .. } => {
                let current = self.records.get(&before.id);
                let matches_before = current
                    .map(|current| same_durable_fields(current, before, &self.types))
                    .transpose()?
                    .unwrap_or(false);
                if !matches_before {
                    return Err(EntityError::StaleRevision {
                        id: before.id,
                        expected: before.revision,
                        actual: current.map(|record| record.revision),
                    });
                }
                let current = current.expect("checked above");
                if !transaction_is_transfer(&transaction.operation)
                    && matches!(&after.location, EntityLocation::Mobile { .. })
                    && after.owner != current.owner
                {
                    return Err(EntityError::InvalidTransaction);
                }
                if transaction_is_transfer(&transaction.operation)
                    && (current.motion_revision != before.motion_revision
                        || self.motion_fences.get(&before.id) != Some(&before.motion_revision))
                {
                    return Err(EntityError::StaleMotionRevision {
                        id: before.id,
                        expected: before.motion_revision,
                        actual: Some(current.motion_revision),
                    });
                }
                let descriptor = self.types.descriptor(after.entity_type)?;
                let indexed_after = if transaction_is_transfer(&transaction.operation) {
                    after.clone()
                } else if after.location != before.location {
                    // Planned WAL move: preview the moved record, matching
                    // what application installs instead of merging away.
                    after.clone()
                } else {
                    merge_durable_fields(current, after)
                };
                self.validate_record(&indexed_after, descriptor)?;
                self.indexes
                    .preview_change(Some(current), Some(&indexed_after))?;
            }
            Operation::Despawn { before, .. } => {
                let current = self.records.get(&before.id);
                let matches_before = current
                    .map(|current| same_durable_fields(current, before, &self.types))
                    .transpose()?
                    .unwrap_or(false);
                if !matches_before {
                    return Err(EntityError::StaleRevision {
                        id: before.id,
                        expected: before.revision,
                        actual: current.map(|record| record.revision),
                    });
                }
                self.indexes.preview_change(current, None)?;
            }
            Operation::Batch { .. } => return Err(EntityError::InvalidTransaction),
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
        let durable_sequence = self
            .durable_sequence
            .checked_add(1)
            .ok_or(EntityError::RevisionExhausted)?;
        let operations = match transaction.operation {
            Operation::Batch { operations } => operations,
            operation => vec![operation],
        };
        let mut deltas = Vec::new();
        for operation in operations {
            deltas.extend(self.apply_operation(operation)?);
        }
        self.revision = registry_revision;
        self.durable_sequence = durable_sequence;
        self.durable_global_revision = registry_revision;
        Ok(EntityCommit {
            registry_revision,
            deltas,
        })
    }

    /// Replays an already-synced transaction into an ordered checkpoint
    /// mirror. Unlike the live path, the mirror did not create transfer
    /// fences at preparation time; it reconstructs each fence only after
    /// proving the transaction's expected durable fields and motion revision
    /// match the mirror frontier. Checkpoint-motion events and these prepared
    /// batches must be applied in their coordinator sequence order.
    pub fn apply_committed_mirror(
        &mut self,
        transaction: PreparedEntityTransaction,
    ) -> Result<EntityCommit, EntityError> {
        let mut operations = Vec::new();
        collect_operations(transaction.operation.clone(), &mut operations);
        let mut transfer_fences = BTreeMap::new();
        for operation in &operations {
            let Operation::Replace {
                before,
                after,
                transferred: true,
            } = operation
            else {
                continue;
            };
            let current = self.records.get(&before.id);
            let matches_before = match current {
                Some(current) => {
                    same_durable_fields(current, before, &self.types)?
                        && current.motion_revision == before.motion_revision
                        && !self.motion_fences.contains_key(&before.id)
                        && before.owner != after.owner
                }
                None => false,
            };
            if !matches_before
                || transfer_fences
                    .insert(before.id, before.motion_revision)
                    .is_some()
            {
                return Err(EntityError::StaleMotionRevision {
                    id: before.id,
                    expected: before.motion_revision,
                    actual: current.map(|record| record.motion_revision),
                });
            }
        }
        self.motion_fences.extend(
            transfer_fences
                .iter()
                .map(|(id, revision)| (*id, *revision)),
        );
        let result = self.apply_committed(transaction);
        for id in transfer_fences.keys() {
            self.motion_fences.remove(id);
        }
        result
    }

    fn apply_operation(&mut self, operation: Operation) -> Result<Vec<EntityDelta>, EntityError> {
        match operation {
            Operation::SpawnBatch {
                after,
                expected_allocator,
            } => {
                let mut deltas = Vec::with_capacity(after.len());
                for record in after {
                    self.indexes.insert(&record)?;
                    self.records.insert(record.id, record.clone());
                    deltas.push(EntityDelta::Spawned(record.public_view()));
                }
                self.next_id = expected_allocator
                    .checked_add(u64::try_from(deltas.len()).map_err(|_| EntityError::IdExhausted)?)
                    .ok_or(EntityError::IdExhausted)?;
                Ok(deltas)
            }
            Operation::Replace {
                before,
                after,
                transferred,
            } => {
                let current = self
                    .records
                    .get(&before.id)
                    .cloned()
                    .ok_or(EntityError::UnknownEntity(before.id))?;
                let transferred_owner = transferred || before.owner != after.owner;
                // A WAL transaction whose after-location differs from its own
                // before-location is a planned move: merging the current
                // location over it would silently discard the committed
                // decision. WAL-motion types never run checkpoint motion, so
                // `current` still matches `before` here; the preview below
                // was validated against this same resolution.
                let planned_move = !transferred && after.location != before.location;
                let applied = if transferred || planned_move {
                    after
                } else {
                    merge_durable_fields(&current, &after)
                };
                let before_touched_chunks =
                    current.location.touched_chunks()?.into_iter().collect();
                self.indexes.replace(Some(&current), Some(&applied))?;
                self.records.insert(applied.id, applied.clone());
                let delta = if transferred_owner {
                    self.motion_fences.remove(&before.id);
                    EntityDelta::Transferred {
                        before_owner: before.owner,
                        before_touched_chunks: before
                            .location
                            .touched_chunks()?
                            .into_iter()
                            .collect(),
                        view: applied.public_view(),
                    }
                } else if planned_move {
                    EntityDelta::Moved(applied.public_view())
                } else {
                    EntityDelta::Updated {
                        before_touched_chunks,
                        view: applied.public_view(),
                    }
                };
                Ok(vec![delta])
            }
            Operation::Despawn {
                before,
                removal_revision,
            } => {
                let current = self
                    .records
                    .get(&before.id)
                    .cloned()
                    .ok_or(EntityError::UnknownEntity(before.id))?;
                self.indexes.replace(Some(&current), None)?;
                self.records.remove(&before.id);
                Ok(vec![EntityDelta::Despawned {
                    id: before.id,
                    entity_type: before.entity_type,
                    revision: removal_revision,
                    owner: before.owner,
                    touched_chunks: before.location.touched_chunks()?.into_iter().collect(),
                }])
            }
            Operation::Batch { .. } => Err(EntityError::InvalidTransaction),
        }
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
        durable_sequence: u64,
        durable_global_revision: u64,
        records: BTreeMap<EntityId, EntityRecord>,
    ) -> Result<Self, EntityError> {
        if next_id == 0
            || next_id > TRANSIENT_ENTITY_ID_BIT
            || records.len() > MAX_ENTITY_RECORDS
            || durable_global_revision > revision
            || durable_global_revision < durable_sequence
        {
            return Err(EntityError::CorruptCheckpoint);
        }
        let max_id = records.keys().next_back().map_or(0, |id| id.get());
        if next_id <= max_id || max_id >= TRANSIENT_ENTITY_ID_BIT {
            return Err(EntityError::CorruptCheckpoint);
        }
        if records.values().any(|record| record.revision > revision) {
            return Err(EntityError::CorruptCheckpoint);
        }
        let mut store = Self::new(types);
        store.next_id = next_id;
        store.revision = revision;
        store.durable_sequence = durable_sequence;
        store.durable_global_revision = durable_global_revision;
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
        let mut motion_values = BTreeMap::new();
        let mut chunk_values = BTreeMap::new();
        let mut cell_values = BTreeMap::new();
        let mut allocator = None;
        let mut revision_watermark = None;
        for (key, value) in values {
            match key.domain.as_str() {
                ENTITY_RECORD_DOMAIN => {
                    let id = decode_entity_id(&key.bytes)?;
                    owner_values.insert(id, value.clone());
                }
                ENTITY_MOTION_DOMAIN => {
                    let id = decode_entity_id(&key.bytes)?;
                    if !value.is_empty() {
                        let motion = super::persistence::decode_motion_value(id, value)?;
                        motion_values.insert(id, motion);
                    }
                }
                ENTITY_ALLOCATOR_DOMAIN => {
                    if !key.bytes.is_empty() {
                        return Err(EntityError::CorruptCheckpoint);
                    }
                    allocator = Some(super::persistence::decode_allocator_value(value)?);
                }
                ENTITY_REVISION_DOMAIN => {
                    if !key.bytes.is_empty() {
                        return Err(EntityError::CorruptCheckpoint);
                    }
                    if revision_watermark
                        .replace(super::persistence::decode_revision_value(value)?)
                        .is_some()
                    {
                        return Err(EntityError::CorruptCheckpoint);
                    }
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
            && motion_values.is_empty()
            && chunk_values.is_empty()
            && cell_values.is_empty()
            && allocator.is_none()
            && revision_watermark.is_none()
        {
            return Ok(false);
        }
        let mut records = self.records.clone();
        for (id, value) in owner_values {
            if value.is_empty() {
                records.remove(&id);
            } else {
                let wal_motion = motion_values.get(&id).copied();
                let record = super::persistence::decode_durable_record_value(
                    id,
                    &value,
                    &self.types,
                    records.get(&id),
                    wal_motion,
                )?;
                records.insert(id, record);
            }
        }
        for (id, motion) in &motion_values {
            let Some(record) = records.get_mut(id) else {
                // A motion value can outlive a later entity tombstone in the
                // compacted latest-value map. The durable entity record is
                // authoritative for existence; never resurrect from motion.
                continue;
            };
            if !matches!(&record.location, EntityLocation::Mobile { .. }) {
                return Err(EntityError::CorruptCheckpoint);
            }
            if motion.revision > record.motion_revision {
                record.location = EntityLocation::Mobile {
                    position: motion.position,
                };
                record.motion_revision = motion.revision;
            } else if motion.revision == record.motion_revision {
                let EntityLocation::Mobile { position } = &record.location else {
                    return Err(EntityError::CorruptCheckpoint);
                };
                if position.map(|value| value.to_bits()) != motion.position.map(f32::to_bits) {
                    return Err(EntityError::CorruptCheckpoint);
                }
            }
        }
        let next_id = allocator.unwrap_or(self.next_id);
        let (durable_sequence, durable_global_revision) =
            revision_watermark.unwrap_or((self.durable_sequence, self.durable_global_revision));
        if durable_sequence < self.durable_sequence
            || durable_global_revision < self.durable_global_revision
            || (durable_sequence == self.durable_sequence
                && durable_global_revision != self.durable_global_revision)
        {
            return Err(EntityError::CorruptCheckpoint);
        }
        // Latest-value replay cannot reconstruct the number of commits, but
        // the WAL-owned watermark preserves it even if the highest-revision
        // entity was despawned. Mobile motion has its own per-entity revision
        // and is checkpoint-owned; it must not advance this WAL-owned registry
        // frontier.
        let revision = records
            .values()
            .map(|record| record.revision)
            .max()
            .unwrap_or(self.revision)
            .max(self.revision)
            .max(durable_global_revision);
        let recovered = Self::from_parts(
            self.types.clone(),
            next_id,
            revision,
            durable_sequence,
            durable_global_revision,
            records,
        )?;
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
            || self.revision != recovered.revision
            || self.durable_sequence != recovered.durable_sequence
            || self.durable_global_revision != recovered.durable_global_revision;
        *self = recovered;
        Ok(changed)
    }

    fn prepare_change(
        &self,
        operation: Operation,
    ) -> Result<PreparedEntityTransaction, EntityError> {
        let (before, after) = match &operation {
            Operation::SpawnBatch { .. } | Operation::Batch { .. } => {
                return Err(EntityError::InvalidTransaction);
            }
            Operation::Replace { before, after, .. } => (Some(before), Some(after)),
            Operation::Despawn { before, .. } => (Some(before), None),
        };
        let id = before
            .map(|record| record.id)
            .or_else(|| after.map(|record| record.id))
            .ok_or(EntityError::InvalidTransaction)?;
        let before_bytes = before
            .map(|record| encode_durable_record_value(record, &self.types))
            .transpose()?
            .unwrap_or_default();
        let after_bytes = after
            .map(|record| encode_durable_record_value(record, &self.types))
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
        changes.push(self.durable_revision_change()?);
        changes.sort_by(|left, right| left.key.cmp(&right.key));
        if changes.len() > MAX_ENTITY_TRANSACTION_CHANGES {
            return Err(EntityError::TooManyTransactionChanges);
        }
        validate_transaction_size(&changes)?;
        Ok(PreparedEntityTransaction {
            operation,
            changes,
            additional_read_keys: Vec::new(),
        })
    }

    fn expected(&self, id: EntityId, expected_revision: u64) -> Result<EntityRecord, EntityError> {
        if self.motion_fences.contains_key(&id) {
            return Err(EntityError::MotionFenced);
        }
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
        match (&record.location, record.motion_revision) {
            (EntityLocation::Mobile { .. }, 0) | (EntityLocation::Anchored { .. }, 1..) => {
                return Err(EntityError::InvalidLocation);
            }
            _ => {}
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
                        .map(|record| encode_durable_record_value(record, &self.types))
                        .transpose()?
                        .unwrap_or_default(),
                ))
            }
            ENTITY_MOTION_DOMAIN => {
                let id = decode_entity_id(&key.bytes)?;
                Ok(Some(
                    self.records
                        .get(&id)
                        .map(encode_motion_value)
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
            ENTITY_REVISION_DOMAIN => {
                if !key.bytes.is_empty() {
                    return Err(EntityError::InvalidTransaction);
                }
                Ok(Some(encode_revision_value(
                    self.durable_sequence,
                    self.durable_global_revision,
                )?))
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

    /// Returns the checkpoint's current value for a journal-owned entity key.
    ///
    /// Recovery uses this to prove that an existing entity checkpoint value
    /// is reachable from the journal before applying its latest overlay. The
    /// mobile motion domain is intentionally checkpoint-owned between spawn
    /// and transfer receipts, so callers must compare BGEM revisions using the
    /// motion merge rule instead of `Journal::validate_snapshot`'s exact WAL
    /// history rule.
    pub fn checkpoint_value_for_key(&self, key: &StateKey) -> Result<Option<Vec<u8>>, EntityError> {
        self.value_for_key(key)
    }

    fn ensure_revision_room(&self) -> Result<(), EntityError> {
        self.revision
            .checked_add(1)
            .map(|_| ())
            .ok_or(EntityError::RevisionExhausted)
    }

    fn durable_revision_change(&self) -> Result<Change, EntityError> {
        let sequence = self
            .durable_sequence
            .checked_add(1)
            .ok_or(EntityError::RevisionExhausted)?;
        let global_revision = self
            .revision
            .checked_add(1)
            .ok_or(EntityError::RevisionExhausted)?;
        Ok(Change::new(
            revision_state_key(),
            encode_revision_value(self.durable_sequence, self.durable_global_revision)?,
            encode_revision_value(sequence, global_revision)?,
        ))
    }
}

fn transaction_is_transfer(operation: &Operation) -> bool {
    matches!(
        operation,
        Operation::Replace {
            transferred: true,
            ..
        }
    )
}

fn operation_entity_ids(operation: &Operation) -> Vec<EntityId> {
    match operation {
        Operation::SpawnBatch { after, .. } => after.iter().map(|record| record.id).collect(),
        Operation::Replace { after, .. } => vec![after.id],
        Operation::Despawn { before, .. } => vec![before.id],
        Operation::Batch { operations } => {
            operations.iter().flat_map(operation_entity_ids).collect()
        }
    }
}

fn collect_operations(operation: Operation, output: &mut Vec<Operation>) {
    match operation {
        Operation::Batch { operations } => {
            for operation in operations {
                collect_operations(operation, output);
            }
        }
        operation => output.push(operation),
    }
}

fn is_entity_state_domain(domain: &str) -> bool {
    matches!(
        domain,
        ENTITY_RECORD_DOMAIN
            | ENTITY_MOTION_DOMAIN
            | ENTITY_ALLOCATOR_DOMAIN
            | ENTITY_REVISION_DOMAIN
            | ENTITY_CHUNK_DOMAIN
            | ENTITY_CELL_DOMAIN
    )
}

fn apply_operation_to_projection(
    types: &EntityTypeRegistry,
    motion_fences: &BTreeMap<EntityId, u64>,
    operation: &Operation,
    records: &mut BTreeMap<EntityId, EntityRecord>,
    indexes: &mut EntityIndexes,
    next_id: &mut u64,
) -> Result<(), EntityError> {
    match operation {
        Operation::SpawnBatch {
            after,
            expected_allocator,
        } => {
            if after.is_empty()
                || after.len() > MAX_ENTITY_SPAWN_BATCH
                || *next_id != *expected_allocator
                || records.len().saturating_add(after.len()) > MAX_ENTITY_RECORDS
                || after.first().map(|record| record.id.get()) != Some(*expected_allocator)
            {
                return Err(EntityError::InvalidTransaction);
            }
            for (offset, record) in after.iter().enumerate() {
                let expected = expected_allocator
                    .checked_add(u64::try_from(offset).map_err(|_| EntityError::IdExhausted)?)
                    .ok_or(EntityError::IdExhausted)?;
                if record.id.get() != expected || records.contains_key(&record.id) {
                    return Err(EntityError::InvalidTransaction);
                }
                let descriptor = types.descriptor(record.entity_type)?;
                validate_record_with_types(record, descriptor)?;
                indexes.insert(record)?;
                records.insert(record.id, record.clone());
            }
            *next_id = expected_allocator
                .checked_add(u64::try_from(after.len()).map_err(|_| EntityError::IdExhausted)?)
                .filter(|next| *next != 0)
                .ok_or(EntityError::IdExhausted)?;
        }
        Operation::Replace {
            before,
            after,
            transferred,
        } => {
            let current = records
                .get(&before.id)
                .cloned()
                .ok_or(EntityError::UnknownEntity(before.id))?;
            if !same_durable_fields(&current, before, types)? {
                return Err(EntityError::StaleRevision {
                    id: before.id,
                    expected: before.revision,
                    actual: Some(current.revision),
                });
            }
            if *transferred
                && (current.motion_revision != before.motion_revision
                    || motion_fences.get(&before.id) != Some(&before.motion_revision))
            {
                return Err(EntityError::StaleMotionRevision {
                    id: before.id,
                    expected: before.motion_revision,
                    actual: Some(current.motion_revision),
                });
            }
            if !*transferred
                && matches!(&after.location, EntityLocation::Mobile { .. })
                && after.owner != current.owner
            {
                return Err(EntityError::InvalidTransaction);
            }
            // Same planned-move resolution as live application: a WAL
            // transaction that moves its own preimage applies the move.
            let planned_move = !transferred && after.location != before.location;
            let applied = if *transferred || planned_move {
                after.clone()
            } else {
                merge_durable_fields(&current, after)
            };
            let descriptor = types.descriptor(applied.entity_type)?;
            validate_record_with_types(&applied, descriptor)?;
            indexes.replace(Some(&current), Some(&applied))?;
            records.insert(applied.id, applied);
        }
        Operation::Despawn { before, .. } => {
            let current = records
                .get(&before.id)
                .cloned()
                .ok_or(EntityError::UnknownEntity(before.id))?;
            if !same_durable_fields(&current, before, types)? {
                return Err(EntityError::StaleRevision {
                    id: before.id,
                    expected: before.revision,
                    actual: Some(current.revision),
                });
            }
            indexes.replace(Some(&current), None)?;
            records.remove(&before.id);
        }
        Operation::Batch { .. } => return Err(EntityError::InvalidTransaction),
    }
    Ok(())
}

fn validate_record_with_types(
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
    match (&record.location, record.motion_revision) {
        (EntityLocation::Mobile { .. }, 0) | (EntityLocation::Anchored { .. }, 1..) => {
            return Err(EntityError::InvalidLocation);
        }
        _ => {}
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

fn validate_transaction_size(changes: &[Change]) -> Result<(), EntityError> {
    // Include the journal transaction header and exact per-change framing.
    let size = changes.iter().try_fold(30usize, |total, change| {
        total
            .checked_add(17 + change.key.domain.len() + change.key.bytes.len())
            .and_then(|value| value.checked_add(change.before.len()))
            .and_then(|value| value.checked_add(change.after.len()))
    });
    if size.is_none_or(|size| size > MAX_ENTITY_TRANSACTION_BYTES) {
        Err(EntityError::TransactionTooLarge)
    } else {
        Ok(())
    }
}

/// Compares only WAL-owned fields. Mobile motion is intentionally excluded:
/// a delayed count/payload receipt must merge over the newer checkpointed
/// position rather than reject or rewind it.
fn same_durable_fields(
    left: &EntityRecord,
    right: &EntityRecord,
    types: &EntityTypeRegistry,
) -> Result<bool, EntityError> {
    let same_fields = left.id == right.id
        && left.entity_type == right.entity_type
        && left.schema_version == right.schema_version
        && left.schema_fingerprint == right.schema_fingerprint
        && left.owner == right.owner
        && left.revision == right.revision
        && match (&left.location, &right.location) {
            (EntityLocation::Mobile { .. }, EntityLocation::Mobile { .. }) => true,
            (EntityLocation::Anchored { .. }, EntityLocation::Anchored { .. }) => {
                left.location == right.location
            }
            _ => false,
        }
        && left.payload_size == right.payload_size
        && left.public_view == right.public_view
        && left.next_tick == right.next_tick;
    if !same_fields {
        return Ok(false);
    }
    let descriptor = types.descriptor(left.entity_type)?;
    Ok(descriptor.encode_payload(&left.payload)? == descriptor.encode_payload(&right.payload)?)
}

fn merge_durable_fields(current: &EntityRecord, durable_after: &EntityRecord) -> EntityRecord {
    let mut merged = durable_after.clone();
    if matches!(&durable_after.location, EntityLocation::Mobile { .. }) {
        merged.location = current.location.clone();
        merged.motion_revision = current.motion_revision;
    }
    merged
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

fn revision_state_key() -> StateKey {
    StateKey::new(ENTITY_REVISION_DOMAIN, Vec::new())
}

fn motion_state_key(id: EntityId) -> StateKey {
    StateKey::new(ENTITY_MOTION_DOMAIN, id.get().to_le_bytes().to_vec())
}

fn decode_entity_id(bytes: &[u8]) -> Result<EntityId, EntityError> {
    if bytes.len() != 8 {
        return Err(EntityError::InvalidTransaction);
    }
    let id = EntityId::new(u64::from_le_bytes(
        bytes
            .try_into()
            .map_err(|_| EntityError::InvalidTransaction)?,
    ))
    .ok_or(EntityError::InvalidTransaction)?;
    if id.get() >= TRANSIENT_ENTITY_ID_BIT {
        return Err(EntityError::InvalidTransaction);
    }
    Ok(id)
}

fn chunk_state_key(chunk: ChunkKey) -> StateKey {
    StateKey::new(ENTITY_CHUNK_DOMAIN, encode_chunk_key(chunk))
}

fn cell_state_key(cell: CellCoord) -> StateKey {
    StateKey::new(ENTITY_CELL_DOMAIN, encode_cell_key(cell))
}
