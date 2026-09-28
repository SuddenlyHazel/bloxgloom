//! Live executor for registered owner handlers without trusted adapters.
//!
//! Each system owns revisioned cells in one barrier-owned
//! [`DurableOwnerStore`]: the single source of truth for owner state. The
//! coordinator selects a stable bounded owner prefix, workers prepare
//! replacements from immutable snapshots, and one validated wave commits
//! through the main journal — staged before-values, one WAL record, apply
//! only after the receipt — before anything becomes visible. This runtime
//! captures bounded authoritative world neighborhoods for opted-in public
//! chunk owners. Owner count and declared patch accounting are bounded;
//! arbitrary heap usage inside `Any` payloads is not measured or sandboxed.

use super::super::durable::TerrainReads;
use super::super::durable::{CommitBarrier, Durability};
use super::super::effects::{EffectKindRegistryFrozen, MAX_EFFECTS_PER_BATCH};
use super::super::journal::{Change, StateKey};
use super::super::parallel::{
    BatchId, JobKey, MAX_PHASE_QUEUE_CAPACITY, MAX_PHASE_RESULT_CAPACITY, MAX_PHASE_WORKERS,
    OwnerData, OwnerJob, OwnerKey, OwnerPatch, OwnerSchedule, OwnerSnapshot, OwnerWaveError,
    OwnerWaveLimits, PhaseExecutor, ValidatedOwnerWave,
};
use super::super::registry::{ExecutableSystem, SystemHandlerError, SystemId};
use super::super::simulation::TickId;
use super::owner_codec::{
    OWNER_CURSOR_DOMAIN, decode_cursor_value, decode_owner_cursor_key, encode_cursor_value,
    owner_cursor_key,
};
use super::owner_commit::{
    OwnerWaveDurables, WaveDisposition, arbitrate_key_sets, build_owner_writes_parallel,
    canonical_key_set,
};
#[cfg(test)]
use super::owner_durable::OwnerSystemConfig;
use super::owner_durable::{DurableOwnerStore, OwnerDurableError};
#[cfg(test)]
use super::owner_durable::{OwnerWrite, PreparedOwnerWave};
use super::owner_effects::{OwnerEffectPatch, route_and_consume};
use super::owner_wake::PendingWakeStore;
#[cfg(test)]
use std::any::Any;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, ErrorKind};
use std::sync::Arc;

pub(in crate::server) const MAX_OWNER_VALUES_PER_SYSTEM: usize = 16_384;

#[path = "systems/commit.rs"]
mod commit;
pub(in crate::server) mod intent;
#[path = "systems/world.rs"]
mod world;

/// Bound on staged next-tick owner wakes. The set only schedules work that
/// the destination would do on its own rotation; when it fills, producing
/// waves defer with `WouldBlock` instead of growing it.
pub(in crate::server) const MAX_PENDING_OWNER_WAKES: usize = MAX_EFFECTS_PER_BATCH;

/// Shared admission inputs for one owner wave. World capture is optional for
/// legacy owner-only handlers; production supplies it for public chunk readers.
pub(in crate::server) struct RegisteredWaveInputs<'a> {
    pub effects: &'a EffectKindRegistryFrozen,
    pub durability: &'a mut Durability,
    pub in_flight: &'a [Vec<StateKey>],
    pub world: RegisteredWorldInputs<'a>,
}

pub(in crate::server) struct RegisteredWorldInputs<'a> {
    pub world: Option<&'a mut crate::world::World>,
    pub entities: Option<&'a crate::server::entities::EntityStore>,
    pub players: &'a [[f32; 3]],
    pub seed: u64,
    pub missing: &'a mut Vec<crate::world::ChunkKey>,
}

/// Admission metadata only. The shared durable queue owns the payload,
/// receipt and reservations; losing this handle cannot lose accepted work.
#[derive(Debug)]
pub(in crate::server) struct StagedOwnerCommit {
    keys: Vec<StateKey>,
    pub barrier: CommitBarrier,
}

/// One validated owner wave, prepared but not yet staged: worker execution,
/// validation, effect routing, and wake-flag preparation are done, while the
/// journal reservation and the rotation-cursor advance still wait for the
/// stage and the receipt. Preparing every wave in a phase before staging any
/// of them lets the coordinator arbitrate overlapping key sets up front.
pub(in crate::server) struct PreparedRegisteredWave {
    durables: OwnerWaveDurables,
}

impl PreparedRegisteredWave {
    /// Full canonical key set this wave would reserve: owner changes, staged
    /// wake flags, served-flag clears, and the rotation cursor. Duplicate
    /// keys within the wave collapse, matching what `stage_owner_wave`
    /// reserves, so arbitration sees exactly the stage-time key set.
    fn keys(&self, runtime: &SystemRuntime) -> Vec<StateKey> {
        let clear_changes = runtime
            .durable_wakes
            .stage_clears(&self.durables.durable_served);
        let world_changes = self
            .durables
            .world_action
            .as_ref()
            .map_or_else(Vec::new, |action| action.changes());
        canonical_key_set(
            self.durables
                .prepared
                .changes()
                .iter()
                .chain(self.durables.wake_sets.changes().iter())
                .chain(clear_changes.iter())
                .chain(self.durables.cursor.iter())
                .chain(world_changes.iter())
                .chain(self.durables.intents.changes().iter()),
        )
    }
}

/// One staged registered wave with its rotation cursor held back: the cursor
/// advances only after the receipt applies, so a retry re-stages from
/// WAL-backed state and never double-applies.
pub(in crate::server) struct PendingRegisteredWave {
    staged: StagedOwnerCommit,
}

impl PendingRegisteredWave {
    /// Reserved journal keys for arbitration against later waves.
    pub(in crate::server) fn keys(&self) -> &[StateKey] {
        &self.staged.keys
    }

    pub(in crate::server) fn barrier(&self) -> CommitBarrier {
        self.staged.barrier
    }
}

impl std::fmt::Debug for PendingRegisteredWave {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PendingRegisteredWave")
            .field("keys", &self.staged.keys.len())
            .finish_non_exhaustive()
    }
}

pub(in crate::server) struct SystemRuntime {
    executor: Option<PhaseExecutor<OwnerPatch, SystemHandlerError>>,
    worker_count: usize,
    /// The single source of truth for owner state. Workers never touch it;
    /// waves prepare against read revisions and commit through the main
    /// journal with a receipt in hand. There is no second store.
    durable: DurableOwnerStore,
    next_owner: BTreeMap<SystemId, OwnerKey>,
    unvalidated_owners: BTreeSet<SystemId>,
    /// Destinations woken by routed effects, served from each system's normal
    /// job budget next tick. In-memory only: losing them costs latency, never
    /// state.
    pending_wakes: BTreeMap<SystemId, BTreeMap<OwnerKey, u64>>,
    /// Capacity held by prepared/accepted waves, not yet public hints.
    staged_live_wakes: usize,
    /// Durable pending-wake flags for destinations with no live cell. Staged
    /// into the producer wave's WAL record and recovered at open, so a wake
    /// to an unloaded owner is held until that owner loads instead of being
    /// skipped. Carries no effect payload: the flag only asks the destination
    /// to do its own durable work sooner.
    durable_wakes: PendingWakeStore,
    durable_wake_cursor: BTreeMap<SystemId, OwnerKey>,
    /// When set, routed effects are discarded before any consumer runs while
    /// producer commits still apply. Tests use this to prove delivery is
    /// optional; it is never set by live routing.
    drop_registered_effects: bool,
}

impl SystemRuntime {
    #[cfg(test)]
    pub fn new(workers: usize) -> io::Result<Self> {
        Self::with_durable_store(
            workers,
            DurableOwnerStore::new(Vec::new())?,
            PendingWakeStore::new(),
            BTreeMap::new(),
        )
    }

    /// Builds the live runtime over journal-recovered owner state. Recovery
    /// runs before the server accepts work, so this store already holds the
    /// last receipted wave for every registered system, and `cursors` resumes
    /// each system's round-robin rotation where the last receipted wave left
    /// it instead of restarting at the lowest owner.
    pub(in crate::server) fn with_durable_store(
        workers: usize,
        durable: DurableOwnerStore,
        durable_wakes: PendingWakeStore,
        cursors: BTreeMap<SystemId, OwnerKey>,
    ) -> io::Result<Self> {
        if workers == 0 || workers > MAX_PHASE_WORKERS {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("registered-system worker count must be 1..={MAX_PHASE_WORKERS}"),
            ));
        }
        durable_wakes.intents.validate_owners(&durable)?;
        Ok(Self {
            executor: None,
            worker_count: workers,
            durable,
            next_owner: cursors,
            unvalidated_owners: BTreeSet::new(),
            pending_wakes: BTreeMap::new(),
            staged_live_wakes: 0,
            durable_wakes,
            durable_wake_cursor: BTreeMap::new(),
            drop_registered_effects: false,
        })
    }

    /// Installs one system's value codec. Every live system needs its codec
    /// before its first seed or wave: without it the value cannot be encoded
    /// for the WAL, so the wave cannot be staged. Test harnesses driving
    /// `SystemRuntime` directly install codecs here; production builds the
    /// descriptor set from `ServerStartup` before recovery.
    #[cfg(test)]
    pub(in crate::server) fn register_owner_system(
        &mut self,
        config: OwnerSystemConfig,
    ) -> io::Result<()> {
        self.durable.register(config)
    }

    /// Installs owner state before its first scheduled tick.
    #[cfg(test)]
    pub fn insert_owner<T: Any + Send + Sync>(
        &mut self,
        system: SystemId,
        owner: OwnerKey,
        value: T,
    ) -> io::Result<()> {
        self.insert_owner_data(system, owner, OwnerData::new(value))
    }

    pub fn insert_owner_data(
        &mut self,
        system: SystemId,
        owner: OwnerKey,
        value: OwnerData,
    ) -> io::Result<()> {
        if !self.durable.is_registered(&system) {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                format!(
                    "registered owner system {} has no owner codec",
                    system.as_str()
                ),
            ));
        }
        self.durable
            .insert(&system, owner, value)
            .map_err(OwnerDurableError::io)?;
        self.unvalidated_owners.insert(system);
        Ok(())
    }

    /// Stages the exact WAL change for a fresh cell. The caller submits it,
    /// waits for the receipt, then makes the cell visible with
    /// [`SystemRuntime::insert_owner_data`].
    pub(in crate::server) fn stage_owner_insert(
        &self,
        system: &SystemId,
        owner: OwnerKey,
        value: &OwnerData,
    ) -> io::Result<Change> {
        self.durable
            .stage_insert(system, owner, value)
            .map_err(OwnerDurableError::io)
    }

    pub(in crate::server) fn owner_snapshot(
        &self,
        system: &SystemId,
        owner: OwnerKey,
    ) -> Option<(u64, OwnerData)> {
        self.durable.snapshot(system, owner)
    }

    /// Stages a bare prepared wave for tests driving the split commit path:
    /// prepares `writes` against current revisions, attaches an empty wake
    /// set, and submits without blocking. The caller polls with
    /// the common durable receipt gate.
    #[cfg(test)]
    pub(in crate::server) fn stage_test_wave(
        &mut self,
        system: &SystemId,
        writes: Vec<OwnerWrite>,
        tick: TickId,
        durability: &mut Durability,
    ) -> io::Result<StagedOwnerCommit> {
        let prepared = self
            .durable
            .prepare(system, writes)
            .map_err(OwnerDurableError::io)?;
        let wake_sets = self
            .durable_wakes
            .prepare_sets(&[], tick.get(), usize::MAX)?;
        self.stage_owner_wave(
            OwnerWaveDurables::new(prepared, tick, wake_sets, Vec::new(), None),
            durability,
        )
    }

    /// Validates a wave against the live store and stages its exact WAL
    /// changes without applying anything. Used by tests driving
    /// multi-domain commits; the live path prepares from validated patches
    /// inside [`SystemRuntime::stage_registered_wave`].
    #[cfg(test)]
    pub(in crate::server) fn prepare_owner_wave(
        &self,
        system: &SystemId,
        writes: Vec<OwnerWrite>,
    ) -> Result<PreparedOwnerWave, OwnerDurableError> {
        self.durable.prepare(system, writes)
    }

    /// Applies WAL-committed owner changes after their receipt, whether they
    /// arrived in a standalone owner wave or piggybacked on an entity
    /// transaction. State cells, durable wake flags, and rotation cursors are
    /// all applied here so piggybacked records converge exactly like
    /// standalone owner waves. A before-value mismatch is genuine corruption
    /// and stops the coordinator; capacity can never surface here.
    pub(in crate::server) fn apply_replayed_owner_changes(
        &mut self,
        changes: &[Change],
    ) -> io::Result<()> {
        self.durable.apply_replayed(changes)?;
        self.durable_wakes.apply_replayed(changes)?;
        for change in changes {
            if change.key.domain != OWNER_CURSOR_DOMAIN {
                continue;
            }
            let Some(system_name) = decode_owner_cursor_key(&change.key) else {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "owner cursor key is malformed",
                ));
            };
            let system = SystemId::new(system_name).map_err(|_| {
                io::Error::new(
                    ErrorKind::InvalidData,
                    "owner cursor key has a bad system id",
                )
            })?;
            if !self.durable.is_registered(&system) {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "owner cursor names an unregistered system",
                ));
            }
            let current = self
                .next_owner
                .get(&system)
                .map(|owner| encode_cursor_value(*owner))
                .unwrap_or_default();
            if current != change.before {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "owner cursor replay precondition mismatch",
                ));
            }
            let owner = decode_cursor_value(&change.after)?;
            self.next_owner.insert(system, owner);
        }
        Ok(())
    }

    /// Receipted durable wake flags held for destinations with no live cell.
    /// Test builds use this to assert staging, recovery, and clearing.
    #[cfg(test)]
    pub(in crate::server) fn durable_wake_count(&self) -> usize {
        self.durable_wakes.len()
    }

    #[cfg(test)]
    pub fn owner_value<T: Any + Clone + Send + Sync>(
        &self,
        system: &SystemId,
        owner: OwnerKey,
    ) -> Option<(u64, T)> {
        let (revision, data) = self.durable.snapshot(system, owner)?;
        let value = data.get::<T>()?.clone();
        Some((revision, value))
    }

    #[cfg(test)]
    pub fn worker_count(&self) -> usize {
        self.worker_count
    }

    /// Delivery-shed switch for tests. Live routing never sets this; dropped
    /// effects must leave producer commits untouched while no consumer runs.
    #[cfg(test)]
    pub(in crate::server) fn set_drop_registered_effects(&mut self, drop: bool) {
        self.drop_registered_effects = drop;
    }

    pub(in crate::server) fn pending_wake_count(&self) -> usize {
        self.pending_wakes.values().map(BTreeMap::len).sum()
    }

    /// Validates, executes, routes, and bundles one registered wave without
    /// staging or applying anything: workers prepare replacements from
    /// immutable snapshots, the wave is validated and its effects routed at
    /// the barrier, revision logs are rebuilt, and durable wake flags are
    /// prepared. The returned wave carries everything `stage_owner_wave`
    /// needs; wake capacity is reserved, but no hint is visible yet. Returns
    /// `None` when the system owns nothing this tick.
    ///
    /// Producer patches may carry effect emissions alongside their owner
    /// replacement. At the commit barrier those emissions are routed through
    /// the shared registered-effect machinery and each live destination is
    /// carried with the commit, served from its own system's normal job
    /// budget next tick. Any routing, bound, or consumer violation rejects
    /// the whole wave with `WouldBlock` before anything commits.
    fn prepare_registered_wave(
        &mut self,
        system: &ExecutableSystem,
        tick: TickId,
        batch_wave: u16,
        effect_kinds: &EffectKindRegistryFrozen,
        world_inputs: RegisteredWorldInputs<'_>,
    ) -> io::Result<Option<PreparedRegisteredWave>> {
        let RegisteredWorldInputs {
            mut world,
            entities,
            players,
            seed,
            missing,
        } = world_inputs;
        if system.is_coordinator_adapter() {
            return Err(io::Error::other(format!(
                "coordinator adapter {} cannot run as an owner handler",
                system.id().as_str()
            )));
        }
        if !system.has_executable_handler() {
            return Err(io::Error::other(format!(
                "registered system {} has no executable handler",
                system.id().as_str()
            )));
        }
        if system.writes().is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!(
                    "registered system {} has no declared owner-state write",
                    system.id().as_str()
                ),
            ));
        }
        if system.neighbor_radius_chunks() != 0 {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!(
                    "registered system {} declares unsupported neighbor owner-state snapshots",
                    system.id().as_str()
                ),
            ));
        }

        let id = system.id().clone();
        // A bounded delivery lane gets two out of three turns; the third
        // retains ordinary/wake service under sustained payload traffic. Its
        // inspection cursor advances on unavailable/conflicting work too.
        let intent_owner = if tick.get() % 3 != 1 {
            self.durable_wakes.intents.next_destination(&id, tick.get())
        } else {
            None
        };
        let ordinary = if intent_owner.is_some() {
            Vec::new()
        } else {
            self.durable.runnable_from(
                &id,
                tick.get(),
                self.next_owner.get(&id).copied(),
                system.max_jobs_per_tick(),
            )
        };
        // Reserve a slot for ordinary runnable work even when wakes arrive
        // every tick. For a one-job system that slot is the whole wave.
        let wake_budget = if intent_owner.is_some() {
            0
        } else if !ordinary.is_empty() && system.max_jobs_per_tick() == 1 {
            usize::from(tick.get() % 3 != 1)
        } else {
            system
                .max_jobs_per_tick()
                .saturating_sub(usize::from(!ordinary.is_empty()))
        };
        // Woken destinations take precedence within the normal job budget
        // next tick; leftovers stay staged. Owners that no longer exist are
        // dropped here: a missing destination only costs latency.
        let pending = self.pending_wakes.remove(&id).unwrap_or_default();
        let mut selected: Vec<_> = intent_owner.into_iter().collect();
        let mut selected_wakes = Vec::new();
        for (owner, produced_tick) in pending {
            if self.durable.revision(&id, owner).is_none() {
                continue;
            }
            if produced_tick < tick.get() && selected.len() < wake_budget {
                selected.push(owner);
                selected_wakes.push((owner, produced_tick));
            } else {
                self.pending_wakes
                    .entry(id.clone())
                    .or_default()
                    .insert(owner, produced_tick);
            }
        }
        let mut seen: BTreeSet<OwnerKey> = selected.iter().copied().collect();
        // Durable flags for destinations that have loaded since the wake was
        // staged join the normal job budget next; flags for still-absent
        // owners stay held. Served flags clear in this wave's record (built
        // at commit time below), so a served flag is never served twice.
        let mut durable_served: Vec<(SystemId, OwnerKey)> = Vec::new();
        if selected.len() < wake_budget {
            for (owner, _) in self.durable_wakes.flagged_from(
                &id,
                self.durable_wake_cursor.get(&id).copied(),
                system.max_jobs_per_tick(),
            ) {
                self.durable_wake_cursor.insert(id.clone(), owner);
                if self
                    .durable_wakes
                    .published_at(&id, owner)
                    .is_some_and(|produced| produced >= tick.get())
                {
                    continue;
                }
                if selected.len() >= wake_budget {
                    break;
                }
                if self.durable.revision(&id, owner).is_none() {
                    continue;
                }
                if seen.insert(owner) {
                    selected.push(owner);
                    durable_served.push((id.clone(), owner));
                }
            }
        }
        if self.unvalidated_owners.contains(&id) {
            if let Some(owner) = self
                .durable
                .owners_of(&id)
                .into_iter()
                .find(|owner| !system.accepts_owner(*owner))
            {
                return Err(io::Error::other(format!(
                    "registered system {} owns a value in the wrong partition: {owner:?}",
                    id.as_str()
                )));
            }
            self.unvalidated_owners.remove(&id);
        }
        if !self.durable.has_owner(&id) {
            return Ok(None);
        }

        // Due and active owners use the persisted rotation cursor. A wake-only
        // wave leaves the ordinary cursor unchanged, including across replay.
        let ordinary_cursor = ordinary.first().copied();
        let mut last_ordinary = None;
        if selected.len() < system.max_jobs_per_tick() {
            for owner in ordinary {
                if selected.len() >= system.max_jobs_per_tick() {
                    break;
                }
                last_ordinary = Some(owner);
                if seen.insert(owner) {
                    selected.push(owner);
                }
            }
        }
        if selected.is_empty() {
            return Ok(None);
        }
        let next_cursor = if let Some(last) = last_ordinary {
            self.durable.successor(&id, last)
        } else if self.next_owner.contains_key(&id) || ordinary_cursor.is_some() {
            self.next_owner.get(&id).copied().or(ordinary_cursor)
        } else {
            self.durable.successor(
                &id,
                *selected.last().expect("selected owners are non-empty"),
            )
        }
        .expect("non-empty owner store has a successor");
        // The rotation cursor persists in this wave's own record, so fairness
        // does not reset on restart. Staging mutates nothing: the live cursor
        // advances only after the receipt, and the staged before-value chains
        // from the last receipted cursor. A fixed-point cursor (one owner)
        // stages nothing: the journal rejects identical before/after values.
        let cursor_after = encode_cursor_value(next_cursor);
        let cursor_before = self
            .next_owner
            .get(&id)
            .map(|owner| encode_cursor_value(*owner))
            .unwrap_or_default();
        let cursor_change = (intent_owner.is_none() && cursor_before != cursor_after)
            .then(|| Change::new(owner_cursor_key(&id), cursor_before, cursor_after));

        let batch = BatchId::new(tick, system.phase(), batch_wave);
        let limits = OwnerWaveLimits {
            max_jobs: system.max_jobs_per_tick(),
            max_effects_per_job: system.max_effects_per_job(),
            max_effect_deliveries: system.max_effects_per_tick(),
            ..OwnerWaveLimits::new(
                system.max_jobs_per_tick(),
                system.max_effects_per_tick(),
                super::super::parallel::MAX_OWNER_WAVE_PATCH_BYTES,
            )
        };
        let mut jobs = Vec::with_capacity(selected.len());
        let mut expected = Vec::with_capacity(selected.len());
        let mut terrain_reads = TerrainReads::default();
        let mut received = Vec::new();
        let mut received_count = 0;
        for (job_id, owner) in selected.iter().copied().enumerate() {
            let (revision, data) = self.durable.snapshot(&id, owner).ok_or_else(|| {
                io::Error::other(format!(
                    "registered owner {owner:?} vanished before dispatch"
                ))
            })?;
            let inbox = self.durable_wakes.intents.capture(
                &id,
                owner,
                tick.get(),
                intent::MAX_WAVE_INTENTS - received_count,
            );
            let data = if inbox.is_empty() {
                data
            } else {
                received_count += inbox.len();
                received.push((owner, inbox.clone()));
                OwnerData::new(intent::JobInput { value: data, inbox })
            };
            let snapshot = OwnerSnapshot::new(owner, revision, Arc::new(data));
            let key = JobKey::new(batch, owner, job_id as u64, snapshot.revision());
            let mut job = OwnerJob::new(id.clone(), key, vec![snapshot])
                .map_err(|error| io::Error::other(format!("registered owner job: {error:?}")))?;
            if let Some(radius) = system.world_read_radius() {
                let world = world.as_deref_mut().ok_or_else(|| {
                    io::Error::new(
                        ErrorKind::InvalidInput,
                        "world-reading owner has no world capture",
                    )
                })?;
                if let Some(chunks) =
                    world::capture(world, &mut terrain_reads, owner, radius, missing)?
                {
                    job = job.with_world_chunks(chunks, world.catalog_arc());
                }
            }
            expected.push(key);
            jobs.push(job);
        }
        if !missing.is_empty() {
            // Nothing was dispatched or committed. Preserve live wake hints;
            // ordinary due owners and durable flags retain their WAL state.
            for (owner, produced) in selected_wakes {
                self.pending_wakes
                    .entry(id.clone())
                    .or_default()
                    .insert(owner, produced);
            }
            return Ok(None);
        }

        if self.executor.is_none() {
            self.executor = Some(
                PhaseExecutor::new(
                    self.worker_count,
                    MAX_PHASE_QUEUE_CAPACITY,
                    MAX_PHASE_RESULT_CAPACITY,
                )
                .map_err(|error| {
                    io::Error::other(format!("registered-system worker pool: {error:?}"))
                })?,
            );
        }
        let executor = self
            .executor
            .as_mut()
            .expect("registered-system executor was initialized above");

        let mut submitted = 0usize;
        let registered = Arc::new(system.clone());
        for job in jobs {
            let key = job.key();
            let registered = Arc::clone(&registered);
            match executor.try_submit(key, move |cancellation| {
                if cancellation.is_cancelled() {
                    return Err(SystemHandlerError::Rejected(
                        "owner job cancelled before execution".into(),
                    ));
                }
                registered.prepare_in_dispatch(&job, batch_wave)
            }) {
                Ok(()) => submitted += 1,
                Err(error) => {
                    if submitted > 0 {
                        let _ = executor.cancel_batch(batch);
                        let _ = executor.barrier(batch);
                    }
                    return Err(io::Error::other(format!(
                        "registered system {} admission failed after {submitted} jobs: {error:?}",
                        id.as_str()
                    )));
                }
            }
        }

        let results = executor
            .barrier_with(batch, |key| {
                self.durable.revision(&id, key.owner) == Some(key.snapshot_revision)
            })
            .map_err(|error| io::Error::other(format!("registered-system barrier: {error:?}")))?;
        let validated = ValidatedOwnerWave::validate(
            &id,
            &expected,
            results,
            batch,
            limits,
            |owner| self.durable.revision(&id, owner),
            |patch| Ok(OwnerEffectPatch::emitted_count(patch)),
            |patch| {
                if matches!(patch.schedule(), OwnerSchedule::AtTick(due) if due <= tick.get()) {
                    return Err("owner deadline must be after the producing tick".into());
                }
                if patch.usage().writes != 1 {
                    return Err("one owner replacement must declare exactly one write".into());
                }
                let Some(replacement) = OwnerEffectPatch::replacement(patch) else {
                    return Err("owner patch must contain replacement OwnerData".into());
                };
                let (_, current) = self
                    .durable
                    .snapshot(&id, patch.owner())
                    .ok_or_else(|| "current owner state missing".to_owned())?;
                if !current.same_type(&replacement) {
                    return Err("owner replacement changes its registered state type".into());
                }
                Ok(())
            },
        )
        .map_err(|error| match error {
            OwnerWaveError::JobEffectOverflow { actual, limit, .. }
            | OwnerWaveError::PhaseEffectOverflow { actual, limit } => io::Error::new(
                io::ErrorKind::WouldBlock,
                format!(
                    "registered system {} defers: effect bound exceeded ({actual} > {limit})",
                    id.as_str()
                ),
            ),
            _ => io::Error::other(format!(
                "registered system {} wave rejected: {error:?}",
                id.as_str()
            )),
        })?;
        let world_action = if validated
            .patches()
            .iter()
            .any(|patch| !OwnerEffectPatch::world_edits(patch).is_empty())
        {
            let world = world.ok_or_else(|| {
                io::Error::new(ErrorKind::InvalidInput, "owner edits have no world")
            })?;
            let entities = entities.ok_or_else(|| {
                io::Error::new(
                    ErrorKind::InvalidInput,
                    "owner edits have no entity authority",
                )
            })?;
            world::plan_edits(world::EditInputs {
                world,
                entities,
                players,
                seed,
                tick: tick.get(),
                radius: system.world_read_radius(),
                patches: validated.patches(),
                reads: &mut terrain_reads,
                missing,
            })?
        } else {
            None
        };
        let mut scheduled = Vec::new();
        let outgoing = intent::collect(&self.durable, &id, validated.patches(), tick.get())?;
        for patch in validated.patches() {
            for (destination, owner) in OwnerEffectPatch::durable_wakes(patch) {
                if scheduled.len() >= 2_048 {
                    return Err(io::Error::new(
                        ErrorKind::QuotaExceeded,
                        "owner wake wave exceeds 2048 destinations",
                    ));
                }
                if !self.durable.accepts_owner(destination, *owner) {
                    return Err(io::Error::new(
                        ErrorKind::InvalidInput,
                        format!(
                            "registered system {} cannot wake {destination:?} at {owner:?}",
                            id.as_str()
                        ),
                    ));
                }
                scheduled.push((destination.clone(), *owner));
            }
        }
        // Every public job emits at most 32; the aggregate cap bounds WAL
        // space independently of a system's declared owner job count.
        // Effects route and consume at the commit barrier, before anything
        // commits: a routing, bound, or consumer violation rejects the whole
        // wave and defers the producing work. Staged wakes are served from
        // each destination system's normal budget next tick, never this one.
        let wakes = route_and_consume(
            &self.durable,
            validated.patches(),
            tick,
            system,
            batch_wave,
            effect_kinds,
            &limits,
            self.drop_registered_effects,
        )
        .map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "registered system {} effects rejected: {error}",
                    id.as_str()
                ),
            )
        })?;
        let staged_wakes = self.pending_wake_count() + self.staged_live_wakes;
        if staged_wakes.saturating_add(wakes.live().len()) > MAX_PENDING_OWNER_WAKES {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                format!(
                    "registered system {} defers: owner wake queue full",
                    id.as_str()
                ),
            ));
        }
        // Revision logs are pure CPU work over independent patches: they are
        // rebuilt on scoped worker threads and collected in stable owner
        // order, so the staged WAL changes are byte-identical to the serial
        // path regardless of thread scheduling. Publication itself stays
        // behind the one ordered barrier below.
        let writes = build_owner_writes_parallel(validated.patches(), self.worker_count)?;
        let prepared = self
            .durable
            .prepare(&id, writes)
            .map_err(OwnerDurableError::io)?;
        // Durable flags for unloaded destinations share the wave's wake
        // budget and ride its WAL record: one logical transaction, one
        // record. A full flag set defers the whole wave before anything
        // commits, like the live set above.
        let wake_limit = MAX_PENDING_OWNER_WAKES.saturating_sub(staged_wakes + wakes.live().len());
        let refresh: BTreeSet<_> = scheduled
            .iter()
            .filter(|wake| durable_served.contains(wake))
            .cloned()
            .collect();
        let durable_served: Vec<_> = durable_served
            .into_iter()
            .filter(|wake| !refresh.contains(wake))
            .collect();
        let mut durable_to_set = wakes.unloaded().to_vec();
        durable_to_set.extend(scheduled);
        let wake_sets = self
            .durable_wakes
            .prepare_sets_with_refresh(&durable_to_set, tick.get(), wake_limit, &refresh)
            .map_err(|error| {
                io::Error::new(
                    error.kind(),
                    format!(
                        "registered system {} defers: durable owner wake queue full",
                        id.as_str()
                    ),
                )
            })?;
        let intents = match self
            .durable_wakes
            .intents
            .prepare(&id, &received, &outgoing)
        {
            Ok(intents) => intents,
            Err(error) => {
                self.durable_wakes.cancel_sets(wake_sets);
                return Err(error);
            }
        };
        self.staged_live_wakes += wakes.live().len();
        let mut durables =
            OwnerWaveDurables::new(prepared, tick, wake_sets, durable_served, cursor_change)
                .with_live_wakes(wakes.live().to_vec())
                .with_terrain_reads(terrain_reads)
                .with_world_action(world_action);
        durables.intents = intents;
        Ok(Some(PreparedRegisteredWave { durables }))
    }

    /// Prepares one registered wave and stages it as one main-journal
    /// transaction without blocking: after this returns, the caller may do
    /// other work while the fsync is in flight. Nothing is visible until the
    /// common durable barrier completes through the returned admission ID.
    ///
    /// `in_flight` holds the reserved key sets of waves already staged ahead
    /// of this one, in canonical stage order. The candidate is arbitrated
    /// against them with [`arbitrate_key_sets`]: disjoint waves stage
    /// alongside each other, while a wave sharing any key with an earlier
    /// staged wave defers with `WouldBlock` and retries after the winner's
    /// receipt. A deferral before the receipt withdraws the staged wake flags
    /// so the retry re-stages from WAL-backed state.
    #[cfg(test)]
    pub(in crate::server) fn stage_registered_wave(
        &mut self,
        system: &ExecutableSystem,
        tick: TickId,
        batch_wave: u16,
        effect_kinds: &EffectKindRegistryFrozen,
        durability: &mut Durability,
        in_flight: &[Vec<StateKey>],
    ) -> io::Result<Option<PendingRegisteredWave>> {
        let mut missing = Vec::new();
        self.stage_registered_wave_with_world(
            system,
            tick,
            batch_wave,
            RegisteredWaveInputs {
                effects: effect_kinds,
                durability,
                in_flight,
                world: RegisteredWorldInputs {
                    world: None,
                    entities: None,
                    players: &[],
                    seed: 0,
                    missing: &mut missing,
                },
            },
        )
    }

    pub(in crate::server) fn stage_registered_wave_with_world(
        &mut self,
        system: &ExecutableSystem,
        tick: TickId,
        batch_wave: u16,
        inputs: RegisteredWaveInputs<'_>,
    ) -> io::Result<Option<PendingRegisteredWave>> {
        let RegisteredWaveInputs {
            effects,
            durability,
            in_flight,
            world,
        } = inputs;
        let pending_before = self.pending_wakes.get(system.id()).cloned();
        let result = (|| {
            let prepared =
                self.prepare_registered_wave(system, tick, batch_wave, effects, world)?;
            let Some(prepared) = prepared else {
                return Ok(None);
            };
            let candidate = prepared.keys(self);
            let mut sets: Vec<Vec<StateKey>> = in_flight.to_vec();
            sets.push(candidate);
            let dispositions = arbitrate_key_sets(&sets);
            if matches!(dispositions.last(), Some(WaveDisposition::Retry { .. })) {
                let PreparedRegisteredWave {
                    durables:
                        OwnerWaveDurables {
                            wake_sets,
                            live_wakes,
                            intents,
                            ..
                        },
                    ..
                } = prepared;
                self.durable_wakes.cancel_sets(wake_sets);
                self.durable_wakes.intents.cancel(intents);
                self.staged_live_wakes -= live_wakes.len();
                return Err(io::Error::new(
                    ErrorKind::WouldBlock,
                    format!(
                        "registered system {} defers: owner key overlaps an in-flight wave",
                        system.id().as_str()
                    ),
                ));
            }
            let PreparedRegisteredWave { durables } = prepared;
            let staged = self.stage_owner_wave(durables, durability)?;
            Ok(Some(PendingRegisteredWave { staged }))
        })();
        if result
            .as_ref()
            .is_err_and(|error| error.kind() == ErrorKind::WouldBlock)
        {
            match pending_before {
                Some(wakes) => {
                    self.pending_wakes.insert(system.id().clone(), wakes);
                }
                None => {
                    self.pending_wakes.remove(system.id());
                }
            }
        }
        result
    }
}
