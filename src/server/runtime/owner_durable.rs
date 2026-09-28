//! Durable owner cells with revisioned snapshots and sparse scheduling.
//!
//! Each `(system, owner)` cell holds a decoded [`OwnerData`] value plus its
//! last encoded payload bytes, so a prepared wave can stage exact WAL
//! before/after values without re-encoding live state. Mutation is
//! barrier-owned: workers never touch this store. A wave is prepared against
//! read revisions, staged as `Change`s through the WAL, and committed only
//! with a WAL receipt in hand. Every fallible check runs before the first
//! cell is updated, so a crash mid-wave replays to a consistent state.
//!
//! Scheduling is sparse: commits maintain an active set and a due-tick map,
//! and work is driven from those indexes. Scanning every owner per tick is
//! not supported by this API.
//!
//! Capacity conditions (too many owners, oversized values) report
//! `WouldBlock`: the coordinator defers or rejects one owner and keeps
//! running. Only genuine corruption (bad envelopes, undecodable payloads,
//! keys for unregistered systems) reports `InvalidData`.

use super::super::journal::{Change, StateKey};
use super::super::parallel::{OwnerData, OwnerKey};
use super::super::registry::{OwnerPartition, SystemId};
use super::owner_codec::{
    MAX_OWNER_VALUE_BYTES, OWNER_STATE_DOMAIN, OwnerCodecError, OwnerValueCodec, decode_cell_value,
    decode_owner_state_key, encode_cell_value, owner_state_key,
};
use crate::world::ChunkKey;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, ErrorKind};
use std::sync::Arc;

/// Fail-closed bound for one owner-wave WAL record. Waves that would exceed
/// it defer with `WouldBlock` instead of pressing the journal's record
/// limit; genuine journal validation failures still report `InvalidData`.
pub(in crate::server) const MAX_OWNER_WAVE_BYTES: usize = 512 * 1024;
const FIRST_OWNER: OwnerKey = OwnerKey::Chunk(ChunkKey {
    x: i32::MIN,
    y: i32::MIN,
    z: i32::MIN,
});

/// Startup registration for one system's durable owner state.
pub(in crate::server) struct OwnerSystemConfig {
    pub system: SystemId,
    pub codec: Arc<dyn OwnerValueCodec>,
    pub codec_version: u16,
    pub max_bytes: usize,
    /// The partition this system serves. Wake fan-out stages durable flags
    /// only for systems whose partition accepts the destination owner, so a
    /// wake to an unloaded owner is held exactly where that owner can load.
    pub partition: OwnerPartition,
}

impl OwnerSystemConfig {
    pub fn new(
        system: SystemId,
        codec: Arc<dyn OwnerValueCodec>,
        codec_version: u16,
        max_bytes: usize,
        partition: OwnerPartition,
    ) -> io::Result<Self> {
        if max_bytes == 0 || max_bytes > MAX_OWNER_VALUE_BYTES {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                format!("owner byte bound {max_bytes} is outside 1..={MAX_OWNER_VALUE_BYTES}"),
            ));
        }
        Ok(Self {
            system,
            codec,
            codec_version,
            max_bytes,
            partition,
        })
    }
}

/// One validated owner replacement with its read preconditions. `reads` are
/// same-system owners whose revisions were observed by the worker; the commit
/// rejects the whole wave if any moved.
#[derive(Clone)]
pub(in crate::server) struct OwnerWrite {
    pub owner: OwnerKey,
    pub reads: Vec<(OwnerKey, u64)>,
    pub value: OwnerData,
    pub due_tick: Option<u64>,
}

impl OwnerWrite {
    #[cfg(test)]
    pub fn new(owner: OwnerKey, read_revision: u64, value: OwnerData) -> Self {
        Self {
            owner,
            reads: vec![(owner, read_revision)],
            value,
            due_tick: None,
        }
    }

    #[cfg(test)]
    pub fn scheduled(mut self, due_tick: Option<u64>) -> Self {
        self.due_tick = due_tick;
        self
    }
}

/// Proof that a WAL worker synced this wave. Commits without one are refused:
/// a mutation must not become visible before its WAL receipt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::server) struct OwnerWalReceipt {
    pub sequence: u64,
}

struct DurableCell {
    revision: u64,
    value: OwnerData,
    /// Last encoded codec payload, kept so staged before-values are exact.
    encoded: Vec<u8>,
    due_tick: Option<u64>,
}

struct CellDescriptor {
    codec: Arc<dyn OwnerValueCodec>,
    codec_version: u16,
    max_bytes: usize,
    partition: OwnerPartition,
}

/// Barrier-owned durable owner state for every registered system.
pub(in crate::server) struct DurableOwnerStore {
    descriptors: BTreeMap<SystemId, CellDescriptor>,
    cells: BTreeMap<(SystemId, OwnerKey), DurableCell>,
    /// Owners with committed work waiting, updated at commit time only.
    active: BTreeSet<(SystemId, OwnerKey)>,
    /// Sparse due-tick index, updated at commit time only.
    schedule: BTreeMap<(u64, SystemId, OwnerKey), ()>,
    /// Deadlines not yet fed into ready_due. Feeding removes an entry here,
    /// not from the authoritative schedule. A rejected wave remains ready.
    schedule_by_system: BTreeSet<(SystemId, u64, OwnerKey)>,
    /// Transient eligibility index, bounded by the capped cell population.
    /// Recovery rebuilds unfed deadlines instead; exact feed order is not
    /// durable, but persisted deadlines and the ordinary rotation are.
    ready_due: BTreeSet<(SystemId, OwnerKey)>,
}

impl std::fmt::Debug for DurableOwnerStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DurableOwnerStore")
            .field("cells", &self.cells.len())
            .field("active", &self.active.len())
            .field("scheduled", &self.schedule.len())
            .finish()
    }
}

impl DurableOwnerStore {
    pub fn accepts_intents(&self, system: &SystemId) -> bool {
        self.descriptors
            .get(system)
            .is_some_and(|descriptor| descriptor.codec.accepts_intents())
    }

    pub fn new(configs: Vec<OwnerSystemConfig>) -> io::Result<Self> {
        let mut descriptors = BTreeMap::new();
        for config in configs {
            if descriptors
                .insert(
                    config.system.clone(),
                    CellDescriptor {
                        codec: config.codec,
                        codec_version: config.codec_version,
                        max_bytes: config.max_bytes,
                        partition: config.partition,
                    },
                )
                .is_some()
            {
                return Err(io::Error::other(format!(
                    "duplicate durable owner system {}",
                    config.system.as_str()
                )));
            }
        }
        Ok(Self {
            descriptors,
            cells: BTreeMap::new(),
            active: BTreeSet::new(),
            schedule: BTreeMap::new(),
            schedule_by_system: BTreeSet::new(),
            ready_due: BTreeSet::new(),
        })
    }

    /// Rebuilds durable state from journal latest-values. Unknown systems,
    /// malformed keys, bad envelopes, and undecodable payloads are
    /// `InvalidData`: the save cannot run. Capacity is checked too: a stored
    /// payload over the system's declared bound fails the same way, since a
    /// bound shrink cannot silently drop persisted state.
    pub fn recover(
        configs: Vec<OwnerSystemConfig>,
        latest: &BTreeMap<StateKey, Vec<u8>>,
    ) -> io::Result<Self> {
        let mut store = Self::new(configs)?;
        for (key, value) in latest {
            if key.domain != OWNER_STATE_DOMAIN {
                continue;
            }
            let Some((system_name, owner)) = decode_owner_state_key(key) else {
                return Err(invalid_data("owner state key is malformed"));
            };
            let system = SystemId::new(system_name)
                .map_err(|_| invalid_data("owner state key has a bad system id"))?;
            let descriptor = store
                .descriptors
                .get(&system)
                .ok_or_else(|| invalid_data("owner state key names an unregistered system"))?;
            let (revision, stored_version, due_tick, payload) = decode_cell_value(value)?;
            if payload.len() > descriptor.max_bytes {
                return Err(invalid_data(
                    "stored owner value exceeds its declared bound",
                ));
            }
            let payload = if stored_version == descriptor.codec_version {
                payload
            } else {
                descriptor
                    .codec
                    .migrate(stored_version, descriptor.codec_version, &payload)
                    .map_err(|_| invalid_data("owner value migration failed"))?
            };
            if payload.len() > descriptor.max_bytes {
                return Err(invalid_data(
                    "migrated owner value exceeds its declared bound",
                ));
            }
            let decoded = descriptor
                .codec
                .decode(&payload)
                .map_err(|_| invalid_data("stored owner value does not decode"))?;
            if store
                .cells
                .insert(
                    (system.clone(), owner),
                    DurableCell {
                        revision,
                        value: decoded,
                        encoded: payload,
                        due_tick,
                    },
                )
                .is_some()
            {
                return Err(invalid_data("duplicate owner state key"));
            }
            if due_tick.is_none() {
                store.active.insert((system.clone(), owner));
            }
            if let Some(due) = due_tick {
                store
                    .schedule_by_system
                    .insert((system.clone(), due, owner));
                store.schedule.insert((due, system, owner), ());
            }
        }
        Ok(store)
    }

    pub fn revision(&self, system: &SystemId, owner: OwnerKey) -> Option<u64> {
        self.cells
            .get(&(system.clone(), owner))
            .map(|cell| cell.revision)
    }

    /// Registers one more system's codec after construction. Test harnesses
    /// use this to install codecs without going through `ServerStartup`;
    /// production builds the whole descriptor set up front so recovery sees
    /// every system before the first replayed key.
    #[cfg(test)]
    pub fn register(&mut self, config: OwnerSystemConfig) -> io::Result<()> {
        if self.descriptors.contains_key(&config.system) {
            return Err(io::Error::other(format!(
                "duplicate durable owner system {}",
                config.system.as_str()
            )));
        }
        self.descriptors.insert(
            config.system,
            CellDescriptor {
                codec: config.codec,
                codec_version: config.codec_version,
                max_bytes: config.max_bytes,
                partition: config.partition,
            },
        );
        Ok(())
    }

    pub fn is_registered(&self, system: &SystemId) -> bool {
        self.descriptors.contains_key(system)
    }

    /// Whether the destination owner can ever load in this system. Wake
    /// fan-out holds durable flags only where the owner can load; unknown
    /// systems accept nothing.
    pub fn accepts_owner(&self, system: &SystemId, owner: OwnerKey) -> bool {
        self.descriptors
            .get(system)
            .is_some_and(|descriptor| match descriptor.partition {
                OwnerPartition::Chunk => matches!(owner, OwnerKey::Chunk(_)),
                OwnerPartition::Entity => matches!(owner, OwnerKey::Entity(_)),
                OwnerPartition::Profile => matches!(owner, OwnerKey::Profile(_)),
                OwnerPartition::Global => true,
            })
    }

    /// Registered systems in canonical order, for deterministic cross-system
    /// scans such as effect-destination lookup.
    pub fn systems(&self) -> impl Iterator<Item = &SystemId> + '_ {
        self.descriptors.keys()
    }

    pub fn has_owner(&self, system: &SystemId) -> bool {
        self.cells
            .range((system.clone(), FIRST_OWNER)..)
            .next()
            .is_some_and(|((candidate, _), _)| candidate == system)
    }

    /// Every live owner of one system in stable order. Used only for the
    /// startup partition validation; per-wave admission uses sparse indexes.
    pub fn owners_of(&self, system: &SystemId) -> Vec<OwnerKey> {
        self.cells
            .range((system.clone(), FIRST_OWNER)..)
            .take_while(|((candidate, _), _)| candidate == system)
            .map(|((_, owner), _)| *owner)
            .collect()
    }

    /// Ordinary runnable owners: unscheduled active cells and scheduled cells
    /// whose persisted deadline has arrived. A bounded due feed keeps a
    /// ready-by-owner index; at most two bounded index prefixes are captured
    /// per wave, regardless of owner population or other systems' deadlines.
    pub fn runnable_from(
        &mut self,
        system: &SystemId,
        through_tick: u64,
        cursor: Option<OwnerKey>,
        limit: usize,
    ) -> Vec<OwnerKey> {
        if limit == 0 {
            return Vec::new();
        }
        // Traverse/capture at most one job budget, never repeatedly feeding an
        // already-ready entry. New schedules enter this index only on commit.
        let lower = (system.clone(), 0, FIRST_OWNER);
        let upper = (system.clone(), through_tick, OwnerKey::Profile(u128::MAX));
        let feed: Vec<_> = self
            .schedule_by_system
            .range(lower..=upper)
            .take(limit)
            .cloned()
            .collect();
        for entry in feed {
            let owner = entry.2;
            self.schedule_by_system.remove(&entry);
            self.ready_due.insert((system.clone(), owner));
        }
        let from = cursor.unwrap_or(FIRST_OWNER);
        let active = self
            .active
            .range((system.clone(), from)..)
            .take_while(|(id, _)| id == system)
            .map(|(_, owner)| *owner)
            .take(limit)
            .chain(
                self.active
                    .range((system.clone(), FIRST_OWNER)..(system.clone(), from))
                    .map(|(_, owner)| *owner)
                    .take(limit),
            );
        let due = self
            .ready_due
            .range((system.clone(), from)..)
            .take_while(|(id, _)| id == system)
            .map(|(_, owner)| *owner)
            .take(limit)
            .chain(
                self.ready_due
                    .range((system.clone(), FIRST_OWNER)..(system.clone(), from))
                    .map(|(_, owner)| *owner)
                    .take(limit),
            );
        let mut active = active.take(limit).peekable();
        let mut due = due.take(limit).peekable();
        let mut selected = Vec::with_capacity(limit);
        // Merge in the *same* circular order as the persisted cursor. Lane
        // alternation with a shared cursor lets a recurring high-key deadline
        // reset the active lane to its lowest key forever.
        let rank = |owner: OwnerKey| (owner < from, owner);
        while selected.len() < limit {
            let next = match (active.peek(), due.peek()) {
                (Some(a), Some(d)) if rank(*a) <= rank(*d) => active.next(),
                (_, Some(_)) => due.next(),
                _ => active.next(),
            };
            let Some(owner) = next else { break };
            // Active and scheduled indexes are disjoint by construction.
            selected.push(owner);
        }
        selected
    }

    pub fn successor(&self, system: &SystemId, owner: OwnerKey) -> Option<OwnerKey> {
        self.cells
            .range((
                std::ops::Bound::Excluded((system.clone(), owner)),
                std::ops::Bound::Unbounded,
            ))
            .next()
            .filter(|((id, _), _)| id == system)
            .map(|((_, owner), _)| *owner)
            .or_else(|| {
                self.cells
                    .range((system.clone(), FIRST_OWNER)..)
                    .next()
                    .filter(|((id, _), _)| id == system)
                    .map(|((_, owner), _)| *owner)
            })
    }

    pub fn snapshot(&self, system: &SystemId, owner: OwnerKey) -> Option<(u64, OwnerData)> {
        self.cells
            .get(&(system.clone(), owner))
            .map(|cell| (cell.revision, cell.value.clone()))
    }

    #[cfg(test)]
    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    #[cfg(test)]
    pub fn active_len(&self) -> usize {
        self.active.len()
    }

    /// Stages the exact WAL change for a new cell without inserting it.
    /// Validation (including the byte bound) runs here so a caller can
    /// spend a transaction ID and wait for the receipt before the cell
    /// becomes visible through [`DurableOwnerStore::insert`].
    pub fn stage_insert(
        &self,
        system: &SystemId,
        owner: OwnerKey,
        value: &OwnerData,
    ) -> Result<Change, OwnerDurableError> {
        let descriptor =
            self.descriptors
                .get(system)
                .ok_or_else(|| OwnerDurableError::UnknownSystem {
                    system: system.clone(),
                })?;
        if self.cells.contains_key(&(system.clone(), owner)) {
            return Err(OwnerDurableError::DuplicateOwner {
                system: system.clone(),
                owner,
            });
        }
        if self.cells.len() >= super::systems::MAX_OWNER_VALUES_PER_SYSTEM {
            return Err(OwnerDurableError::TooManyOwners {
                system: system.clone(),
            });
        }
        let encoded = encode_bounded(descriptor, system, owner, value)?;
        Ok(Change::new(
            owner_state_key(system, owner),
            Vec::new(),
            encode_cell_value(0, descriptor.codec_version, None, &encoded),
        ))
    }

    /// Inserts a new owner cell. Oversized values and a full owner count
    /// report `WouldBlock`: capacity defers one owner, never state.
    pub fn insert(
        &mut self,
        system: &SystemId,
        owner: OwnerKey,
        value: OwnerData,
    ) -> Result<(), OwnerDurableError> {
        let descriptor =
            self.descriptors
                .get(system)
                .ok_or_else(|| OwnerDurableError::UnknownSystem {
                    system: system.clone(),
                })?;
        if self.cells.contains_key(&(system.clone(), owner)) {
            return Err(OwnerDurableError::DuplicateOwner {
                system: system.clone(),
                owner,
            });
        }
        if self.cells.len() >= super::systems::MAX_OWNER_VALUES_PER_SYSTEM {
            return Err(OwnerDurableError::TooManyOwners {
                system: system.clone(),
            });
        }
        let encoded = encode_bounded(descriptor, system, owner, &value)?;
        self.cells.insert(
            (system.clone(), owner),
            DurableCell {
                revision: 0,
                value,
                encoded,
                due_tick: None,
            },
        );
        self.active.insert((system.clone(), owner));
        Ok(())
    }

    /// Validates a whole wave against current revisions and byte bounds and
    /// stages its exact WAL changes. Any stale read, oversized replacement,
    /// or exhausted revision rejects the wave before anything is staged.
    pub fn prepare(
        &self,
        system: &SystemId,
        writes: Vec<OwnerWrite>,
    ) -> Result<PreparedOwnerWave, OwnerDurableError> {
        let descriptor =
            self.descriptors
                .get(system)
                .ok_or_else(|| OwnerDurableError::UnknownSystem {
                    system: system.clone(),
                })?;
        if writes.is_empty() {
            return Err(OwnerDurableError::EmptyWave {
                system: system.clone(),
            });
        }
        let mut seen = BTreeSet::new();
        for write in &writes {
            if !seen.insert(write.owner) {
                return Err(OwnerDurableError::DuplicateOwner {
                    system: system.clone(),
                    owner: write.owner,
                });
            }
        }
        let mut staged = Vec::with_capacity(writes.len());
        for write in &writes {
            let cell = self
                .cells
                .get(&(system.clone(), write.owner))
                .ok_or_else(|| OwnerDurableError::UnknownOwner {
                    system: system.clone(),
                    owner: write.owner,
                })?;
            let mut primary_seen = false;
            for (read_owner, read_revision) in &write.reads {
                let actual = self
                    .cells
                    .get(&(system.clone(), *read_owner))
                    .map(|read| read.revision);
                if actual != Some(*read_revision) {
                    return Err(OwnerDurableError::StaleRevision {
                        system: system.clone(),
                        owner: *read_owner,
                        expected: *read_revision,
                        actual,
                    });
                }
                if *read_owner == write.owner {
                    primary_seen = true;
                }
            }
            if !primary_seen {
                return Err(OwnerDurableError::MissingPrimaryRead {
                    system: system.clone(),
                    owner: write.owner,
                });
            }
            let Some(next_revision) = cell.revision.checked_add(1) else {
                return Err(OwnerDurableError::RevisionExhausted {
                    system: system.clone(),
                    owner: write.owner,
                });
            };
            let encoded = encode_bounded(descriptor, system, write.owner, &write.value)?;
            let before = encode_cell_value(
                cell.revision,
                descriptor.codec_version,
                cell.due_tick,
                &cell.encoded,
            );
            let after = encode_cell_value(
                next_revision,
                descriptor.codec_version,
                write.due_tick,
                &encoded,
            );
            staged.push(StagedWrite {
                write: write.clone(),
                next_revision,
                encoded,
                change: Change::new(owner_state_key(system, write.owner), before, after),
            });
        }
        let changes = staged.iter().map(|staged| staged.change.clone()).collect();
        Ok(PreparedOwnerWave {
            system: system.clone(),
            staged,
            changes,
        })
    }

    /// Commits a prepared wave after its WAL receipt. The receipt is the
    /// visibility gate: without one the commit is refused. Before-values are
    /// rechecked against live cells so a wave prepared before a concurrent
    /// commit cannot half-apply; every check runs before the first update.
    pub fn commit(
        &mut self,
        prepared: PreparedOwnerWave,
        receipt: OwnerWalReceipt,
    ) -> Result<usize, OwnerDurableError> {
        let _ = receipt.sequence;
        if prepared.system_is_empty() {
            return Err(OwnerDurableError::EmptyWave {
                system: prepared.system.clone(),
            });
        }
        let descriptor = self.descriptors.get(&prepared.system).ok_or_else(|| {
            OwnerDurableError::UnknownSystem {
                system: prepared.system.clone(),
            }
        })?;
        for staged in &prepared.staged {
            let cell = self
                .cells
                .get(&(prepared.system.clone(), staged.write.owner))
                .ok_or_else(|| OwnerDurableError::UnknownOwner {
                    system: prepared.system.clone(),
                    owner: staged.write.owner,
                })?;
            let expected_before = encode_cell_value(
                cell.revision,
                descriptor.codec_version,
                cell.due_tick,
                &cell.encoded,
            );
            if expected_before != *staged.before() {
                return Err(OwnerDurableError::StaleRevision {
                    system: prepared.system.clone(),
                    owner: staged.write.owner,
                    expected: staged.expected_revision(),
                    actual: Some(cell.revision),
                });
            }
            for (read_owner, read_revision) in &staged.write.reads {
                let actual = self
                    .cells
                    .get(&(prepared.system.clone(), *read_owner))
                    .map(|read| read.revision);
                if actual != Some(*read_revision) {
                    return Err(OwnerDurableError::StaleRevision {
                        system: prepared.system.clone(),
                        owner: *read_owner,
                        expected: *read_revision,
                        actual,
                    });
                }
            }
        }
        let count = prepared.staged.len();
        for staged in prepared.staged {
            let key = (prepared.system.clone(), staged.write.owner);
            if let Some(previous_due) = self.cells.get(&key).and_then(|cell| cell.due_tick) {
                self.schedule.remove(&(previous_due, key.0.clone(), key.1));
                self.schedule_by_system
                    .remove(&(key.0.clone(), previous_due, key.1));
            }
            self.ready_due.remove(&key);
            let cell = self
                .cells
                .get_mut(&key)
                .expect("cells validated before apply");
            cell.revision = staged.next_revision;
            cell.value = staged.write.value.clone();
            cell.encoded = staged.encoded;
            cell.due_tick = staged.write.due_tick;
            if staged.write.due_tick.is_none() {
                self.active.insert(key.clone());
            } else {
                self.active.remove(&key);
            }
            if let Some(due) = staged.write.due_tick {
                self.schedule_by_system.insert((key.0.clone(), due, key.1));
                self.schedule.insert((due, key.0, key.1), ());
            }
        }
        Ok(count)
    }

    /// Applies already-committed owner-domain changes after their WAL receipt,
    /// whether they arrived in a standalone owner wave or piggybacked on an
    /// entity transaction through `add_related_change`. Every before-value is
    /// rechecked against the live cell (absent reads as empty, matching a
    /// fresh insert), so a mismatch is genuine corruption: the WAL committed
    /// but memory disagrees, and the coordinator must stop. Capacity pressure
    /// can never surface here — a committed record already passed its bound —
    /// so every failure reports `InvalidData`.
    ///
    /// Non-owner keys are ignored; the caller filters the transaction's
    /// change set to this domain.
    pub fn apply_replayed(&mut self, changes: &[Change]) -> io::Result<()> {
        for change in changes {
            if change.key.domain != OWNER_STATE_DOMAIN {
                continue;
            }
            let Some((system_name, owner)) = decode_owner_state_key(&change.key) else {
                return Err(invalid_data("owner state key is malformed"));
            };
            let system = SystemId::new(system_name)
                .map_err(|_| invalid_data("owner state key has a bad system id"))?;
            let descriptor = self
                .descriptors
                .get(&system)
                .ok_or_else(|| invalid_data("owner state key names an unregistered system"))?;
            let current = match self.cells.get(&(system.clone(), owner)) {
                Some(cell) => encode_cell_value(
                    cell.revision,
                    descriptor.codec_version,
                    cell.due_tick,
                    &cell.encoded,
                ),
                None => Vec::new(),
            };
            if current != change.before {
                return Err(invalid_data("owner replay precondition mismatch"));
            }
            let (revision, stored_version, due_tick, payload) = decode_cell_value(&change.after)?;
            if payload.len() > descriptor.max_bytes {
                return Err(invalid_data(
                    "replayed owner value exceeds its declared bound",
                ));
            }
            let payload = if stored_version == descriptor.codec_version {
                payload
            } else {
                descriptor
                    .codec
                    .migrate(stored_version, descriptor.codec_version, &payload)
                    .map_err(|_| invalid_data("owner value migration failed"))?
            };
            if payload.len() > descriptor.max_bytes {
                return Err(invalid_data(
                    "migrated owner value exceeds its declared bound",
                ));
            }
            let decoded = descriptor
                .codec
                .decode(&payload)
                .map_err(|_| invalid_data("replayed owner value does not decode"))?;
            let key = (system, owner);
            if let Some(previous_due) = self.cells.get(&key).and_then(|cell| cell.due_tick) {
                self.schedule.remove(&(previous_due, key.0.clone(), key.1));
                self.schedule_by_system
                    .remove(&(key.0.clone(), previous_due, key.1));
            }
            self.ready_due.remove(&key);
            self.cells.insert(
                key.clone(),
                DurableCell {
                    revision,
                    value: decoded,
                    encoded: payload,
                    due_tick,
                },
            );
            if due_tick.is_none() {
                self.active.insert(key.clone());
            } else {
                self.active.remove(&key);
            }
            if let Some(due) = due_tick {
                self.schedule_by_system.insert((key.0.clone(), due, key.1));
                self.schedule.insert((due, key.0, key.1), ());
            }
        }
        Ok(())
    }

    /// Takes up to `limit` active owners in stable order. Driving work from
    /// this set keeps scheduling proportional to active owners, not total
    /// owners.
    #[cfg(test)]
    pub fn take_active(&mut self, limit: usize) -> Vec<(SystemId, OwnerKey)> {
        let count = limit.min(self.active.len());
        let selected: Vec<_> = self.active.iter().take(count).cloned().collect();
        for key in &selected {
            self.active.remove(key);
        }
        selected
    }

    /// Bounded cursor over owners due at or before `through_tick`, mirroring
    /// `due_tick_entries`: entries after `after` come first, then a wrap
    /// prefix, never more than `maximum`.
    #[cfg(test)]
    pub fn due_entries(
        &self,
        through_tick: u64,
        after: Option<(u64, SystemId, OwnerKey)>,
        maximum: usize,
    ) -> Vec<(u64, SystemId, OwnerKey)> {
        use std::ops::Bound::{Excluded, Unbounded};
        if maximum == 0 {
            return Vec::new();
        }
        let in_range = |(tick, _, _): &(u64, SystemId, OwnerKey)| *tick <= through_tick;
        let mut entries = Vec::new();
        match after {
            Some(cursor) => {
                let (cursor_tick, cursor_system, cursor_owner) = cursor;
                entries.extend(
                    self.schedule
                        .range((
                            Excluded((cursor_tick, cursor_system.clone(), cursor_owner)),
                            Unbounded,
                        ))
                        .filter(|(key, _)| in_range(key))
                        .take(maximum)
                        .map(|(key, _)| key.clone()),
                );
                let remaining = maximum - entries.len();
                entries.extend(
                    self.schedule
                        .range(..=(cursor_tick, cursor_system, cursor_owner))
                        .filter(|(key, _)| in_range(key))
                        .take(remaining)
                        .map(|(key, _)| key.clone()),
                );
            }
            None => {
                entries.extend(
                    self.schedule
                        .iter()
                        .filter(|(key, _)| in_range(key))
                        .take(maximum)
                        .map(|(key, _)| key.clone()),
                );
            }
        }
        entries
    }
}

/// A validated wave with exact WAL changes. The changes borrow nothing: the
/// caller submits them, waits for the receipt, then calls `commit`.
pub(in crate::server) struct PreparedOwnerWave {
    system: SystemId,
    staged: Vec<StagedWrite>,
    changes: Vec<Change>,
}

impl std::fmt::Debug for PreparedOwnerWave {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedOwnerWave")
            .field("system", &self.system)
            .field("writes", &self.staged.len())
            .finish()
    }
}

struct StagedWrite {
    write: OwnerWrite,
    next_revision: u64,
    encoded: Vec<u8>,
    change: Change,
}

impl StagedWrite {
    fn before(&self) -> &Vec<u8> {
        &self.change.before
    }

    fn expected_revision(&self) -> u64 {
        self.next_revision
            .checked_sub(1)
            .expect("staged revision is nonzero")
    }
}

impl PreparedOwnerWave {
    /// Carry every captured owner read into admission, including owners not
    /// replaced by this wave. Writes dominate shared reads in the common gate.
    pub fn read_keys(&self) -> Vec<StateKey> {
        self.staged
            .iter()
            .flat_map(|staged| staged.write.reads.iter())
            .map(|(owner, _)| owner_state_key(&self.system, *owner))
            .collect()
    }

    pub fn changes(&self) -> &[Change] {
        &self.changes
    }

    fn system_is_empty(&self) -> bool {
        self.staged.is_empty()
    }
}

fn encode_bounded(
    descriptor: &CellDescriptor,
    system: &SystemId,
    owner: OwnerKey,
    value: &OwnerData,
) -> Result<Vec<u8>, OwnerDurableError> {
    let encoded =
        descriptor
            .codec
            .encode(value)
            .map_err(|error| OwnerDurableError::CodecRejected {
                system: system.clone(),
                owner,
                error,
            })?;
    if encoded.len() > descriptor.max_bytes {
        return Err(OwnerDurableError::ValueTooLarge {
            system: system.clone(),
            owner,
            actual: encoded.len(),
            limit: descriptor.max_bytes,
        });
    }
    Ok(encoded)
}

fn invalid_data(message: &'static str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}

/// Typed durable-owner failure. Capacity conditions map to `WouldBlock` so
/// the coordinator defers one owner and keeps running; only `Corrupt*`
/// variants and recovery failures map to `InvalidData`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::server) enum OwnerDurableError {
    UnknownSystem {
        system: SystemId,
    },
    UnknownOwner {
        system: SystemId,
        owner: OwnerKey,
    },
    DuplicateOwner {
        system: SystemId,
        owner: OwnerKey,
    },
    StaleRevision {
        system: SystemId,
        owner: OwnerKey,
        expected: u64,
        actual: Option<u64>,
    },
    MissingPrimaryRead {
        system: SystemId,
        owner: OwnerKey,
    },
    ValueTooLarge {
        system: SystemId,
        owner: OwnerKey,
        actual: usize,
        limit: usize,
    },
    TooManyOwners {
        system: SystemId,
    },
    CodecRejected {
        system: SystemId,
        owner: OwnerKey,
        error: OwnerCodecError,
    },
    RevisionExhausted {
        system: SystemId,
        owner: OwnerKey,
    },
    EmptyWave {
        system: SystemId,
    },
}

impl OwnerDurableError {
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::ValueTooLarge { .. } | Self::TooManyOwners { .. } => ErrorKind::WouldBlock,
            Self::CodecRejected { .. } => ErrorKind::Other,
            _ => ErrorKind::Other,
        }
    }

    pub fn io(self) -> io::Error {
        let kind = self.kind();
        io::Error::new(kind, format!("{self:?}"))
    }
}

#[cfg(test)]
#[path = "owner_durable/tests.rs"]
mod tests;
