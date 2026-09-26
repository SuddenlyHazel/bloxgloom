//! Fair chunk-owner admission and WAL transaction construction.

use super::codec::{checksum, finish, invalid, key_bytes, read_key};
use super::frontier::MAX_FRONTIER_CELLS;
use super::handler::{
    FireDeliveryInput, FireDeliveryPatch, FireOwnerInput, FireOwnerPatch, MAX_DUE_CELLS_PER_OWNER,
    neighbor,
};
use super::pending::{decode_pending_key, pending_key};
use super::{FireFrontier, FireIgnition, FireIgnitionId, FirePending};
use crate::server::effects::CellCoord;
use crate::server::journal::{Change, StateKey};
use crate::server::parallel::{
    BatchId, JobKey, OwnerJob, OwnerKey, OwnerPatch, OwnerSnapshot, PhaseExecutor,
    ValidatedOwnerWave,
};
use crate::server::registry::{PhasePlan, SystemHandlerError, SystemId};
use crate::server::simulation::{Phase, TickId};
use crate::world::{BlockId, ChunkKey, GLOWSTONE, OwnerApplyReceipt, PreparedEdit, World};
use std::array;
use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[path = "scheduler/encode.rs"]
mod encode;
#[path = "scheduler/fixture.rs"]
mod fixture;

pub(super) const FIRE_LANES: usize = 32;
const TOTAL_CURSOR_LANES: usize = FIRE_LANES * 2;
pub(super) const FIRE_SYSTEM_ID: &str = "bloxgloom:fire_propagate";
pub(super) const FIRE_DELIVERY_SYSTEM_ID: &str = "bloxgloom:fire_deliver";
const MAX_SOURCE_OWNERS_PER_WAVE: usize = FIRE_LANES;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct FireCursor {
    pub(super) last_owner: Option<ChunkKey>,
    pub(super) last_source: Option<ChunkKey>,
    pub(super) last_tick: u64,
}

impl FireCursor {
    pub(super) fn encode(self) -> Vec<u8> {
        if self == Self::default() {
            return Vec::new();
        }
        let mut bytes = Vec::with_capacity(42);
        bytes.extend(b"BGFC");
        bytes.push(1);
        bytes.push(
            u8::from(self.last_owner.is_some()) | (u8::from(self.last_source.is_some()) << 1),
        );
        bytes.extend(key_bytes(self.last_owner.unwrap_or(ChunkKey {
            x: 0,
            y: 0,
            z: 0,
        })));
        bytes.extend(key_bytes(self.last_source.unwrap_or(ChunkKey {
            x: 0,
            y: 0,
            z: 0,
        })));
        bytes.extend(self.last_tick.to_le_bytes());
        finish(bytes)
    }

    pub(super) fn decode(bytes: &[u8]) -> io::Result<Self> {
        if bytes.is_empty() {
            return Ok(Self::default());
        }
        if bytes.len() != 42 || &bytes[..4] != b"BGFC" || bytes[4] != 1 {
            return Err(invalid("invalid fire cursor record"));
        }
        if checksum(&bytes[..38]) != u32::from_le_bytes(bytes[38..].try_into().unwrap()) {
            return Err(invalid("fire cursor checksum mismatch"));
        }
        let owner = read_key(&bytes[6..18])?;
        let source = read_key(&bytes[18..30])?;
        let marker = bytes[5];
        let last_owner = match marker & 1 {
            0 if owner == (ChunkKey { x: 0, y: 0, z: 0 }) => None,
            1 => Some(owner),
            _ => return Err(invalid("invalid fire cursor owner marker")),
        };
        let last_source = match marker & 2 {
            0 if source == (ChunkKey { x: 0, y: 0, z: 0 }) => None,
            2 => Some(source),
            _ => return Err(invalid("invalid fire cursor source marker")),
        };
        let last_tick = u64::from_le_bytes(bytes[30..38].try_into().unwrap());
        if marker & !3 != 0
            || (last_owner.is_some() && last_tick == 0)
            || (last_source.is_some() && last_owner.is_none())
        {
            return Err(invalid("fire cursor has owner without tick"));
        }
        Ok(Self {
            last_owner,
            last_source,
            last_tick,
        })
    }
}

#[derive(Clone, Debug)]
pub(in crate::server) struct FireRecovered {
    frontiers: BTreeMap<ChunkKey, FireFrontier>,
    pending: BTreeMap<(ChunkKey, ChunkKey), FirePending>,
    cursors: [FireCursor; TOTAL_CURSOR_LANES],
}

impl Default for FireRecovered {
    fn default() -> Self {
        Self {
            frontiers: BTreeMap::new(),
            pending: BTreeMap::new(),
            cursors: [FireCursor::default(); TOTAL_CURSOR_LANES],
        }
    }
}

impl FireRecovered {
    pub(in crate::server) fn apply_value(
        &mut self,
        key: &StateKey,
        value: &[u8],
    ) -> io::Result<bool> {
        match key.domain.as_str() {
            "bloxgloom:fire_frontier" => {
                let owner = read_key(&key.bytes)?;
                let frontier = FireFrontier::decode(value)?;
                if frontier.is_empty() {
                    self.frontiers.remove(&owner);
                } else {
                    self.frontiers.insert(owner, frontier);
                }
            }
            "bloxgloom:fire_pending" => {
                let mailbox = decode_pending_key(&key.bytes)?;
                let pending = FirePending::decode(value)?;
                if pending.is_empty() {
                    self.pending.remove(&mailbox);
                } else {
                    self.pending.insert(mailbox, pending);
                }
            }
            "bloxgloom:fire_cursor" => {
                let [lane] = key.bytes.as_slice() else {
                    return Err(invalid("invalid fire cursor key"));
                };
                let lane = usize::from(*lane);
                if lane >= TOTAL_CURSOR_LANES {
                    return Err(invalid("invalid fire cursor lane"));
                }
                let cursor = FireCursor::decode(value)?;
                if cursor
                    .last_owner
                    .is_some_and(|owner| cursor_lane(owner, lane) != lane)
                {
                    return Err(invalid("fire cursor belongs to wrong lane"));
                }
                if (lane < FIRE_LANES && cursor.last_source.is_some())
                    || (lane >= FIRE_LANES
                        && cursor.last_owner.is_some()
                        && cursor.last_source.is_none())
                {
                    return Err(invalid("fire cursor has wrong lane shape"));
                }
                self.cursors[lane] = cursor;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    pub(in crate::server) fn last_tick(&self) -> u64 {
        let cursor = self
            .cursors
            .iter()
            .map(|cursor| cursor.last_tick)
            .max()
            .unwrap_or_default();
        let frontier = self
            .frontiers
            .values()
            .map(FireFrontier::latest_tick)
            .max()
            .unwrap_or(0);
        let pending = self
            .pending
            .values()
            .map(FirePending::latest_tick)
            .max()
            .unwrap_or(0);
        cursor.max(frontier).max(pending)
    }

    #[cfg(test)]
    pub(in crate::server) fn checkpoint_values(&self) -> Vec<(StateKey, Vec<u8>)> {
        let mut values = Vec::new();
        for (&key, frontier) in &self.frontiers {
            values.push((frontier_key(key), frontier.encode()));
        }
        for (&(destination, source), mailbox) in &self.pending {
            values.push((mailbox_key(destination, source), mailbox.encode()));
        }
        for (lane, cursor) in self.cursors.iter().enumerate() {
            if *cursor != FireCursor::default() {
                values.push((cursor_key(lane), cursor.encode()));
            }
        }
        values
    }
}

#[derive(Clone, Debug)]
pub(super) struct MailboxUpdate {
    pub(super) destination: ChunkKey,
    pub(super) source: ChunkKey,
    pub(super) after: FirePending,
}

/// Ignitions produced by a validated player edit. These mailbox transitions
/// must be included in that edit's same WAL transaction before visibility.
#[derive(Clone, Debug)]
pub(in crate::server) struct FireSeed {
    pub(in crate::server) changes: Vec<Change>,
    mailboxes: Vec<MailboxUpdate>,
}

impl FireSeed {
    pub(in crate::server) fn changes(&self) -> &[Change] {
        &self.changes
    }
}

#[derive(Clone, Debug)]
pub(in crate::server) struct FireTransaction {
    pub(in crate::server) owner: ChunkKey,
    pub(in crate::server) burns: Vec<u16>,
    pub(in crate::server) changed_cells: Vec<CellCoord>,
    pub(in crate::server) world_edit: Option<PreparedEdit>,
    pub(in crate::server) changes: Vec<Change>,
    pub(super) emitted_effects: usize,
    pub(super) delivered_effects: usize,
    pub(super) frontier_after: FireFrontier,
    pub(super) mailboxes: Vec<MailboxUpdate>,
    pub(super) cursor_lane: usize,
    pub(super) cursor_after: FireCursor,
}

impl FireTransaction {
    pub(in crate::server) fn changes(&self) -> &[Change] {
        &self.changes
    }

    #[cfg(test)]
    pub(in crate::server) fn burns(&self) -> &[u16] {
        &self.burns
    }

    pub(in crate::server) fn owner(&self) -> ChunkKey {
        self.owner
    }
}

/// Wall-clock stages of one production owner wave. The worker barrier includes
/// handler execution and waiting for the slowest owner, but never a WAL wait or
/// post-receipt world apply.
#[derive(Clone, Copy, Debug, Default)]
pub(in crate::server) struct FireWaveTimings {
    pub(in crate::server) capture: Duration,
    pub(in crate::server) worker_barrier: Duration,
    pub(in crate::server) validate: Duration,
    pub(in crate::server) route_and_encode: Duration,
}

pub(in crate::server) struct FireWave {
    pub(in crate::server) transactions: Vec<FireTransaction>,
    pub(in crate::server) missing_chunks: Vec<ChunkKey>,
    pub(in crate::server) deferred_owners: usize,
    pub(in crate::server) timings: FireWaveTimings,
}

/// Cumulative admission counters and a point-in-time authoritative fire load.
/// Benchmark sampling is outside the coordinator's measured phase timing.
#[derive(Clone, Copy, Debug, Default)]
pub(in crate::server) struct FireLoadMetrics {
    pub(in crate::server) frontier_cells: usize,
    pub(in crate::server) pending_ignitions: usize,
    pub(in crate::server) admitted_transactions: u64,
    pub(in crate::server) deferred_transactions: u64,
    pub(in crate::server) conflict_deferred_transactions: u64,
    pub(in crate::server) full_deferred_transactions: u64,
    pub(in crate::server) deferred_owners: u64,
    pub(in crate::server) missing_chunks: u64,
    pub(in crate::server) burned_cells: u64,
}

pub(in crate::server) struct FireRuntime {
    frontiers: BTreeMap<ChunkKey, Arc<FireFrontier>>,
    pending: BTreeMap<(ChunkKey, ChunkKey), Arc<FirePending>>,
    cursors: [FireCursor; TOTAL_CURSOR_LANES],
    inflight_frontiers: BTreeSet<ChunkKey>,
    inflight_mailboxes: BTreeSet<(ChunkKey, ChunkKey)>,
    inflight_lanes: BTreeSet<usize>,
    executor: PhaseExecutor<OwnerPatch, SystemHandlerError>,
    encode_executor: PhaseExecutor<Option<FireTransaction>, io::Error>,
    pub(super) apply_executor: PhaseExecutor<Vec<OwnerApplyReceipt>, io::Error>,
    pub(super) apply_sequence: u64,
    admission: FireLoadMetrics,
}

impl FireRuntime {
    pub(in crate::server) fn new(recovered: FireRecovered, workers: usize) -> io::Result<Self> {
        let executor = PhaseExecutor::new(workers, FIRE_LANES * 2, FIRE_LANES * 2)
            .map_err(|error| io::Error::other(format!("fire worker pool: {error:?}")))?;
        let apply_executor = PhaseExecutor::new(workers, FIRE_LANES * 2, FIRE_LANES * 2)
            .map_err(|error| io::Error::other(format!("fire apply worker pool: {error:?}")))?;
        let encode_executor = PhaseExecutor::new(workers, FIRE_LANES * 2, FIRE_LANES * 2)
            .map_err(|error| io::Error::other(format!("fire encode worker pool: {error:?}")))?;
        Ok(Self {
            frontiers: recovered
                .frontiers
                .into_iter()
                .map(|(key, frontier)| (key, Arc::new(frontier)))
                .collect(),
            pending: recovered
                .pending
                .into_iter()
                .map(|(key, mailbox)| (key, Arc::new(mailbox)))
                .collect(),
            cursors: recovered.cursors,
            inflight_frontiers: BTreeSet::new(),
            inflight_mailboxes: BTreeSet::new(),
            inflight_lanes: BTreeSet::new(),
            executor,
            encode_executor,
            apply_executor,
            apply_sequence: 0,
            admission: FireLoadMetrics::default(),
        })
    }

    pub(in crate::server) fn load_metrics(&self) -> FireLoadMetrics {
        FireLoadMetrics {
            frontier_cells: self.frontiers.values().map(|frontier| frontier.len()).sum(),
            pending_ignitions: self.pending.values().map(|mailbox| mailbox.len()).sum(),
            ..self.admission
        }
    }

    pub(in crate::server) fn note_admitted(&mut self, count: usize) {
        self.admission.admitted_transactions = self
            .admission
            .admitted_transactions
            .saturating_add(count as u64);
    }

    pub(in crate::server) fn note_deferred(&mut self, count: usize) {
        self.admission.deferred_transactions = self
            .admission
            .deferred_transactions
            .saturating_add(count as u64);
    }

    pub(in crate::server) fn note_conflict(&mut self, count: usize) {
        self.note_deferred(count);
        self.admission.conflict_deferred_transactions = self
            .admission
            .conflict_deferred_transactions
            .saturating_add(count as u64);
    }

    pub(in crate::server) fn note_full(&mut self, count: usize) {
        self.note_deferred(count);
        self.admission.full_deferred_transactions = self
            .admission
            .full_deferred_transactions
            .saturating_add(count as u64);
    }

    /// Prefer lanes that have gone longest without a committed owner update.
    /// Lane cursors are WAL-durable, so a smaller admitted prefix cannot keep
    /// choosing the same low-key owners while other active lanes starve.
    pub(in crate::server) fn prioritize_transactions(&self, transactions: &mut [FireTransaction]) {
        transactions.sort_by_key(|transaction| {
            (
                self.cursors[transaction.cursor_lane].last_tick,
                transaction.owner,
            )
        });
    }

    /// Called by the edit planner for an actual glowstone placement. The
    /// caller joins these `changes` to the BGED/inventory/receipt action, and
    /// calls `mark_seed_submitted` only after complete WAL admission.
    pub(in crate::server) fn prepare_seed_from_edit(
        &self,
        tick: TickId,
        source: ChunkKey,
        cell: u16,
        placed: BlockId,
    ) -> io::Result<Option<FireSeed>> {
        if placed != GLOWSTONE {
            return Ok(None);
        }
        if cell >= 4_096 {
            return Err(invalid("fire seed cell is outside chunk"));
        }
        let activation = tick
            .get()
            .checked_add(1)
            .ok_or_else(|| invalid("fire seed tick exhausted"))?;
        let mut updates = BTreeMap::<ChunkKey, FirePending>::new();
        for direction in 0..6u8 {
            let (destination, target_cell) = neighbor(source, cell, direction)
                .ok_or_else(|| invalid("fire seed crossed coordinate limit"))?;
            if self.inflight_mailboxes.contains(&(destination, source)) {
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "fire seed mailbox has an in-flight update",
                ));
            }
            updates
                .entry(destination)
                .or_insert_with(|| {
                    self.pending
                        .get(&(destination, source))
                        .map(|mailbox| (**mailbox).clone())
                        .unwrap_or_default()
                })
                .insert(FireIgnition {
                    id: FireIgnitionId {
                        source_tick: tick.get(),
                        source_cell: cell,
                        // Distinct from a fire-frontier emission at the same cell.
                        direction: direction + 8,
                    },
                    target_cell,
                    activate_at: activation,
                })
                .map_err(|error| {
                    if error.kind() == io::ErrorKind::Other {
                        io::Error::new(io::ErrorKind::WouldBlock, "fire seed mailbox full")
                    } else {
                        error
                    }
                })?;
        }
        let mut changes = Vec::with_capacity(updates.len());
        let mut mailboxes = Vec::with_capacity(updates.len());
        for (destination, after) in updates {
            let prior = self.pending.get(&(destination, source));
            let before = prior.map_or_else(Vec::new, |mailbox| mailbox.encode());
            let encoded_after = after.encode();
            if before == encoded_after {
                continue;
            }
            changes.push(Change::new(
                mailbox_key(destination, source),
                before,
                encoded_after,
            ));
            mailboxes.push(MailboxUpdate {
                destination,
                source,
                after,
            });
        }
        Ok(Some(FireSeed { changes, mailboxes }))
    }

    pub(in crate::server) fn mark_seed_submitted(&mut self, seed: &FireSeed) -> io::Result<()> {
        if seed.mailboxes.iter().any(|mailbox| {
            self.inflight_mailboxes
                .contains(&(mailbox.destination, mailbox.source))
        }) {
            return Err(invalid("overlapping fire seed admitted"));
        }
        for mailbox in &seed.mailboxes {
            self.inflight_mailboxes
                .insert((mailbox.destination, mailbox.source));
        }
        Ok(())
    }

    pub(in crate::server) fn install_seed_synced(&mut self, seed: FireSeed) -> io::Result<()> {
        if seed.mailboxes.iter().any(|mailbox| {
            !self
                .inflight_mailboxes
                .contains(&(mailbox.destination, mailbox.source))
        }) {
            return Err(invalid("fire seed receipt has no admitted mailbox"));
        }
        for mailbox in seed.mailboxes {
            self.inflight_mailboxes
                .remove(&(mailbox.destination, mailbox.source));
            self.pending.insert(
                (mailbox.destination, mailbox.source),
                Arc::new(mailbox.after),
            );
        }
        Ok(())
    }

    /// Worker-prepared burn/effect wave. This call performs no durable or live
    /// mutation; a caller may stage each returned transaction or defer it.
    pub(in crate::server) fn prepare_source_wave(
        &mut self,
        world: &mut World,
        plan: &PhasePlan,
        tick: TickId,
    ) -> io::Result<FireWave> {
        let started = Instant::now();
        let system_id = SystemId::new(FIRE_SYSTEM_ID).expect("static fire system ID");
        let system = plan
            .system(&system_id)
            .ok_or_else(|| io::Error::other("fire system not registered"))?
            .clone();
        if !system.has_executable_handler() || system.phase() != Phase::Simulation {
            return Err(io::Error::other(
                "fire system lacks executable simulation handler",
            ));
        }
        let batch = BatchId::new(tick, Phase::Simulation, system.wave_index());
        let selected = self.select_due_owners(tick.get());
        let mut expected = Vec::with_capacity(selected.len());
        let mut captured_versions = BTreeMap::new();
        let mut missing_chunks = Vec::new();
        for owner in selected {
            let Some(basis) = world.cached_edit_basis(owner) else {
                missing_chunks.push(owner);
                continue;
            };
            let frontier = Arc::clone(&self.frontiers[&owner]);
            let mut mailboxes = BTreeMap::new();
            for destination in source_destinations(owner) {
                if let Some(mailbox) = self.pending.get(&(destination, owner)) {
                    mailboxes.insert(destination, Arc::clone(mailbox));
                }
            }
            let revision = basis.chunk().version;
            let key = JobKey::new(batch, owner, 0, revision);
            let input = FireOwnerInput {
                basis,
                frontier,
                mailboxes,
                tick: tick.get(),
                max_due: MAX_DUE_CELLS_PER_OWNER,
            };
            let snapshot = OwnerSnapshot::new(OwnerKey::Chunk(owner), revision, Arc::new(input));
            let job = OwnerJob::new(system_id.clone(), key, vec![snapshot])
                .map_err(|error| io::Error::other(format!("fire owner job: {error:?}")))?;
            let worker_system = system.clone();
            self.executor
                .try_submit(key, move |_| worker_system.prepare(&job))
                .map_err(|error| io::Error::other(format!("fire worker admission: {error:?}")))?;
            captured_versions.insert(owner, revision);
            expected.push(key);
        }
        let captured = Instant::now();
        if expected.is_empty() {
            self.admission.missing_chunks = self
                .admission
                .missing_chunks
                .saturating_add(missing_chunks.len() as u64);
            return Ok(FireWave {
                transactions: Vec::new(),
                missing_chunks,
                deferred_owners: 0,
                timings: FireWaveTimings {
                    capture: captured.duration_since(started),
                    ..FireWaveTimings::default()
                },
            });
        }
        let results = self
            .executor
            .barrier_with(batch, |key| {
                key.owner
                    .as_chunk()
                    .is_some_and(|owner| world.cached_version(owner) == Some(key.snapshot_revision))
            })
            .map_err(|error| io::Error::other(format!("fire worker barrier: {error:?}")))?;
        let barrier_complete = Instant::now();
        let limits = plan
            .owner_wave_limits(&system_id)
            .ok_or_else(|| io::Error::other("fire system has no wave limits"))?;
        let wave = ValidatedOwnerWave::validate(
            &system_id,
            &expected,
            results,
            batch,
            limits,
            |owner| {
                owner
                    .as_chunk()
                    .and_then(|chunk| captured_versions.get(&chunk).copied())
            },
            |patch| {
                Ok(patch
                    .payload::<FireOwnerPatch>()
                    .map_or(0, |patch| patch.effect_count))
            },
            |patch| {
                let prepared = patch
                    .payload::<FireOwnerPatch>()
                    .ok_or("wrong fire owner patch type")?;
                if Some(prepared.expected_chunk_version)
                    != patch
                        .owner()
                        .as_chunk()
                        .and_then(|chunk| captured_versions.get(&chunk).copied())
                {
                    return Err("fire chunk version changed".into());
                }
                Ok(())
            },
        )
        .map_err(|error| io::Error::other(format!("fire wave invalid: {error:?}")))?;
        let validated = Instant::now();
        let mut owner_patches = Vec::with_capacity(wave.patches().len());
        wave.apply(|patch| {
            owner_patches.push(
                patch
                    .into_payload::<FireOwnerPatch>()
                    .expect("validated fire patch payload"),
            );
        });
        let (transactions, deferred_owners) =
            self.encode_source_patches(owner_patches, tick, batch)?;
        self.admission.deferred_owners = self
            .admission
            .deferred_owners
            .saturating_add(deferred_owners as u64);
        self.admission.missing_chunks = self
            .admission
            .missing_chunks
            .saturating_add(missing_chunks.len() as u64);
        let encoded = Instant::now();
        Ok(FireWave {
            transactions,
            missing_chunks,
            deferred_owners,
            timings: FireWaveTimings {
                capture: captured.duration_since(started),
                worker_barrier: barrier_complete.duration_since(captured),
                validate: validated.duration_since(barrier_complete),
                route_and_encode: encoded.duration_since(validated),
            },
        })
    }

    /// Consumes a source-scoped mailbox only against resident authoritative
    /// terrain. Unloaded destinations are reported for asynchronous loading;
    /// their WAL-backed mailboxes remain untouched.
    pub(in crate::server) fn prepare_delivery_wave(
        &mut self,
        world: &mut World,
        plan: &PhasePlan,
        tick: TickId,
    ) -> io::Result<FireWave> {
        let started = Instant::now();
        let system_id = SystemId::new(FIRE_DELIVERY_SYSTEM_ID).expect("static fire delivery ID");
        let system = plan
            .system(&system_id)
            .ok_or_else(|| io::Error::other("fire delivery system not registered"))?
            .clone();
        if !system.has_executable_handler() || system.phase() != Phase::InteractionCommit {
            return Err(io::Error::other("fire delivery handler is not executable"));
        }
        let batch = BatchId::new(tick, Phase::InteractionCommit, system.wave_index());
        let selected = self.select_delivery_owners();
        let mut expected = Vec::with_capacity(selected.len());
        let mut captured_versions = BTreeMap::new();
        let mut missing_chunks = Vec::new();
        let catalog = world.catalog_arc();
        for (destination, source) in selected {
            let Some(chunk) = world.cached_arc_chunk(destination) else {
                missing_chunks.push(destination);
                continue;
            };
            let frontier = self
                .frontiers
                .get(&destination)
                .cloned()
                .unwrap_or_else(|| Arc::new(FireFrontier::default()));
            let pending = Arc::clone(&self.pending[&(destination, source)]);
            let revision = chunk.version;
            let key = JobKey::new(batch, destination, 0, revision);
            let snapshot = OwnerSnapshot::new(
                OwnerKey::Chunk(destination),
                revision,
                Arc::new(FireDeliveryInput {
                    chunk,
                    frontier,
                    source,
                    mailbox: pending,
                    catalog: Arc::clone(&catalog),
                }),
            );
            let job = OwnerJob::new(system_id.clone(), key, vec![snapshot])
                .map_err(|error| io::Error::other(format!("fire delivery job: {error:?}")))?;
            let worker_system = system.clone();
            self.executor
                .try_submit(key, move |_| worker_system.prepare(&job))
                .map_err(|error| io::Error::other(format!("fire delivery admission: {error:?}")))?;
            captured_versions.insert(destination, revision);
            expected.push(key);
        }
        let captured = Instant::now();
        if expected.is_empty() {
            self.admission.missing_chunks = self
                .admission
                .missing_chunks
                .saturating_add(missing_chunks.len() as u64);
            return Ok(FireWave {
                transactions: Vec::new(),
                missing_chunks,
                deferred_owners: 0,
                timings: FireWaveTimings {
                    capture: captured.duration_since(started),
                    ..FireWaveTimings::default()
                },
            });
        }
        let results = self
            .executor
            .barrier_with(batch, |key| {
                key.owner
                    .as_chunk()
                    .is_some_and(|owner| world.cached_version(owner) == Some(key.snapshot_revision))
            })
            .map_err(|error| io::Error::other(format!("fire delivery barrier: {error:?}")))?;
        let barrier_complete = Instant::now();
        let limits = plan
            .owner_wave_limits(&system_id)
            .ok_or_else(|| io::Error::other("fire delivery has no wave limits"))?;
        let wave = ValidatedOwnerWave::validate(
            &system_id,
            &expected,
            results,
            batch,
            limits,
            |owner| {
                owner
                    .as_chunk()
                    .and_then(|chunk| captured_versions.get(&chunk).copied())
            },
            |_| Ok(0),
            |patch| {
                let prepared = patch
                    .payload::<FireDeliveryPatch>()
                    .ok_or("wrong fire delivery patch type")?;
                if Some(prepared.expected_chunk_version)
                    != patch
                        .owner()
                        .as_chunk()
                        .and_then(|chunk| captured_versions.get(&chunk).copied())
                {
                    return Err("fire delivery chunk version changed".into());
                }
                Ok(())
            },
        )
        .map_err(|error| io::Error::other(format!("fire delivery wave invalid: {error:?}")))?;
        let validated = Instant::now();
        let mut owner_patches = Vec::with_capacity(wave.patches().len());
        wave.apply(|patch| {
            owner_patches.push(
                patch
                    .into_payload::<FireDeliveryPatch>()
                    .expect("validated fire delivery payload"),
            );
        });
        let mut transactions = Vec::with_capacity(owner_patches.len());
        let mut deferred_owners = 0;
        for patch in owner_patches {
            if let Some(transaction) = self.transaction_for_delivery(patch, tick.get())? {
                transactions.push(transaction);
            } else {
                deferred_owners += 1;
            }
        }
        self.admission.deferred_owners = self
            .admission
            .deferred_owners
            .saturating_add(deferred_owners as u64);
        self.admission.missing_chunks = self
            .admission
            .missing_chunks
            .saturating_add(missing_chunks.len() as u64);
        let encoded = Instant::now();
        Ok(FireWave {
            transactions,
            missing_chunks,
            deferred_owners,
            timings: FireWaveTimings {
                capture: captured.duration_since(started),
                worker_barrier: barrier_complete.duration_since(captured),
                validate: validated.duration_since(barrier_complete),
                route_and_encode: encoded.duration_since(validated),
            },
        })
    }

    fn select_delivery_owners(&self) -> Vec<(ChunkKey, ChunkKey)> {
        let mut selected = array::from_fn::<_, FIRE_LANES, _>(|_| None);
        let mut wrap = array::from_fn::<_, FIRE_LANES, _>(|_| None);
        for &(destination, source) in self.pending.keys() {
            let lane = owner_lane(destination);
            if self.inflight_lanes.contains(&(FIRE_LANES + lane))
                || self.inflight_frontiers.contains(&destination)
                || self.inflight_mailboxes.contains(&(destination, source))
            {
                continue;
            }
            let cursor = self.cursors[FIRE_LANES + lane];
            if cursor
                .last_owner
                .zip(cursor.last_source)
                .is_some_and(|last| (destination, source) <= last)
            {
                wrap[lane].get_or_insert((destination, source));
            } else {
                selected[lane].get_or_insert((destination, source));
            }
        }
        selected
            .into_iter()
            .zip(wrap)
            .filter_map(|(after, before)| after.or(before))
            .collect()
    }

    fn transaction_for_delivery(
        &self,
        patch: FireDeliveryPatch,
        tick: u64,
    ) -> io::Result<Option<FireTransaction>> {
        if patch.consumed == 0 {
            return Ok(None);
        }
        let destination = patch.owner;
        let source = patch.source;
        let Some(before_pending) = self.pending.get(&(destination, source)) else {
            return Err(invalid("fire delivery mailbox disappeared"));
        };
        let before_frontier = self.frontiers.get(&destination);
        let lane = FIRE_LANES + owner_lane(destination);
        let cursor_before = self.cursors[lane];
        let cursor_after = FireCursor {
            last_owner: Some(destination),
            last_source: Some(source),
            last_tick: tick,
        };
        let before_frontier = before_frontier.map_or_else(Vec::new, |frontier| frontier.encode());
        let after_frontier = patch.frontier_after.encode();
        let mut changes = Vec::with_capacity(3);
        if before_frontier != after_frontier {
            changes.push(Change::new(
                frontier_key(destination),
                before_frontier,
                after_frontier,
            ));
        }
        changes.push(Change::new(
            mailbox_key(destination, source),
            before_pending.encode(),
            patch.pending_after.encode(),
        ));
        changes.push(Change::new(
            cursor_key(lane),
            cursor_before.encode(),
            cursor_after.encode(),
        ));
        let transaction = FireTransaction {
            owner: destination,
            burns: Vec::new(),
            changed_cells: Vec::new(),
            world_edit: None,
            changes,
            emitted_effects: 0,
            delivered_effects: patch.consumed,
            frontier_after: patch.frontier_after,
            mailboxes: vec![MailboxUpdate {
                destination,
                source,
                after: patch.pending_after,
            }],
            cursor_lane: lane,
            cursor_after,
        };
        validate_transaction(&transaction)?;
        Ok(Some(transaction))
    }

    fn select_due_owners(&self, tick: u64) -> Vec<ChunkKey> {
        let mut selected = array::from_fn::<_, FIRE_LANES, _>(|_| None::<ChunkKey>);
        let mut wrap = array::from_fn::<_, FIRE_LANES, _>(|_| None::<ChunkKey>);
        let destinations_with_pending: BTreeSet<_> = self
            .pending
            .keys()
            .map(|&(destination, _)| destination)
            .collect();
        for (&owner, frontier) in &self.frontiers {
            let lane = owner_lane(owner);
            if self.inflight_lanes.contains(&lane)
                || self.inflight_frontiers.contains(&owner)
                || !frontier.is_due(tick)
                // Delivery must get a chance to take the destination
                // frontier lock. A completely full frontier is the one
                // exception: its source burn must first free capacity.
                || (frontier.len() < MAX_FRONTIER_CELLS
                    && destinations_with_pending.contains(&owner))
            {
                continue;
            }
            if self.cursors[lane]
                .last_owner
                .is_some_and(|cursor| owner <= cursor)
            {
                wrap[lane].get_or_insert(owner);
            } else {
                selected[lane].get_or_insert(owner);
            }
        }
        selected
            .into_iter()
            .zip(wrap)
            .filter_map(|(after, before)| after.or(before))
            .take(MAX_SOURCE_OWNERS_PER_WAVE)
            .collect()
    }

    /// Reserve after the journal accepted the complete transaction. A failed
    /// admission leaves all active state and cursors untouched for retry.
    pub(in crate::server) fn mark_submitted(
        &mut self,
        transaction: &FireTransaction,
    ) -> io::Result<()> {
        if self.inflight_frontiers.contains(&transaction.owner)
            || self.inflight_lanes.contains(&transaction.cursor_lane)
            || transaction.mailboxes.iter().any(|mailbox| {
                self.inflight_mailboxes
                    .contains(&(mailbox.destination, mailbox.source))
            })
        {
            return Err(invalid("overlapping fire transaction admitted"));
        }
        self.inflight_frontiers.insert(transaction.owner);
        self.inflight_lanes.insert(transaction.cursor_lane);
        for mailbox in &transaction.mailboxes {
            self.inflight_mailboxes
                .insert((mailbox.destination, mailbox.source));
        }
        Ok(())
    }

    /// Fail before applying any WAL-synced world owner if the corresponding
    /// fire reservations are missing or overlap within this receipt group.
    pub(in crate::server) fn validate_synced_batch(
        &self,
        transactions: &[FireTransaction],
    ) -> io::Result<()> {
        let mut owners = BTreeSet::new();
        let mut lanes = BTreeSet::new();
        let mut mailboxes = BTreeSet::new();
        for transaction in transactions {
            if !owners.insert(transaction.owner)
                || !lanes.insert(transaction.cursor_lane)
                || !self.inflight_frontiers.contains(&transaction.owner)
                || !self.inflight_lanes.contains(&transaction.cursor_lane)
                || transaction
                    .world_edit
                    .as_ref()
                    .is_some_and(|edit| edit.key != transaction.owner)
            {
                return Err(invalid(
                    "fire receipt batch has missing or overlapping owner",
                ));
            }
            for mailbox in &transaction.mailboxes {
                let key = (mailbox.destination, mailbox.source);
                if !mailboxes.insert(key) || !self.inflight_mailboxes.contains(&key) {
                    return Err(invalid(
                        "fire receipt batch has missing or overlapping mailbox",
                    ));
                }
            }
        }
        Ok(())
    }

    /// Called only after WAL sync and successful world-owner apply. The
    /// root/coordinator must stop on an error here, not publish half a batch.
    pub(in crate::server) fn install_synced(
        &mut self,
        transaction: FireTransaction,
    ) -> io::Result<()> {
        if !self.inflight_frontiers.contains(&transaction.owner)
            || !self.inflight_lanes.contains(&transaction.cursor_lane)
            || transaction.mailboxes.iter().any(|mailbox| {
                !self
                    .inflight_mailboxes
                    .contains(&(mailbox.destination, mailbox.source))
            })
        {
            return Err(invalid("fire receipt has no admitted owner or mailbox"));
        }
        self.inflight_frontiers.remove(&transaction.owner);
        self.inflight_lanes.remove(&transaction.cursor_lane);
        for mailbox in &transaction.mailboxes {
            self.inflight_mailboxes
                .remove(&(mailbox.destination, mailbox.source));
        }
        if transaction.frontier_after.is_empty() {
            self.frontiers.remove(&transaction.owner);
        } else {
            self.frontiers
                .insert(transaction.owner, Arc::new(transaction.frontier_after));
        }
        for mailbox in transaction.mailboxes {
            if mailbox.after.is_empty() {
                self.pending.remove(&(mailbox.destination, mailbox.source));
            } else {
                self.pending.insert(
                    (mailbox.destination, mailbox.source),
                    Arc::new(mailbox.after),
                );
            }
        }
        self.admission.burned_cells = self
            .admission
            .burned_cells
            .saturating_add(transaction.burns.len() as u64);
        self.cursors[transaction.cursor_lane] = transaction.cursor_after;
        Ok(())
    }

    #[cfg(test)]
    pub(in crate::server) fn pending_destinations(&self) -> impl Iterator<Item = ChunkKey> + '_ {
        self.pending.keys().map(|&(destination, _)| destination)
    }

    #[cfg(test)]
    pub(in crate::server) fn snapshot(&self) -> FireRecovered {
        FireRecovered {
            frontiers: self
                .frontiers
                .iter()
                .map(|(&key, value)| (key, (**value).clone()))
                .collect(),
            pending: self
                .pending
                .iter()
                .map(|(&key, mailbox)| (key, (**mailbox).clone()))
                .collect(),
            cursors: self.cursors,
        }
    }
}

pub(super) fn frontier_key(owner: ChunkKey) -> StateKey {
    StateKey::new("bloxgloom:fire_frontier", key_bytes(owner).to_vec())
}

pub(super) fn mailbox_key(destination: ChunkKey, source: ChunkKey) -> StateKey {
    StateKey::new("bloxgloom:fire_pending", pending_key(destination, source))
}

pub(super) fn cursor_key(lane: usize) -> StateKey {
    StateKey::new("bloxgloom:fire_cursor", vec![lane as u8])
}

pub(super) fn owner_lane(owner: ChunkKey) -> usize {
    // FNV's low bits preserve an x/z parity pattern for adjacent chunk keys;
    // taking `% 32` directly used only half the lanes in the 16x8 forest.
    // Avalanche before reduction so sparse spatial grids exercise every
    // owner lane. This mapping is persisted indirectly in cursor keys.
    let mut hash = checksum(&key_bytes(owner));
    hash ^= hash >> 16;
    hash = hash.wrapping_mul(0x7feb_352d);
    hash ^= hash >> 15;
    hash = hash.wrapping_mul(0x846c_a68b);
    hash ^= hash >> 16;
    (hash as usize) % FIRE_LANES
}

fn source_destinations(source: ChunkKey) -> BTreeSet<ChunkKey> {
    let mut destinations = BTreeSet::from([source]);
    for axis in 0..3 {
        for delta in [-1, 1] {
            let mut coordinates = [source.x, source.y, source.z];
            if let Some(next) = coordinates[axis].checked_add(delta) {
                coordinates[axis] = next;
                destinations.insert(ChunkKey {
                    x: coordinates[0],
                    y: coordinates[1],
                    z: coordinates[2],
                });
            }
        }
    }
    destinations
}

fn cursor_lane(owner: ChunkKey, lane: usize) -> usize {
    owner_lane(owner) + if lane >= FIRE_LANES { FIRE_LANES } else { 0 }
}

fn validate_transaction(transaction: &FireTransaction) -> io::Result<()> {
    for change in transaction.changes() {
        if change.before == change.after {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "fire owner {:?} emitted unchanged {} key {:?}",
                    transaction.owner, change.key.domain, change.key.bytes
                ),
            ));
        }
    }
    Ok(())
}
