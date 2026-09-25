//! Live executor for registered owner handlers without trusted adapters.
//!
//! Each system owns revisioned cells in one barrier-owned
//! [`DurableOwnerStore`]: the single source of truth for owner state. The
//! coordinator selects a stable bounded owner prefix, workers prepare
//! replacements from immutable snapshots, and one validated wave commits
//! through the main journal — staged before-values, one WAL record, apply
//! only after the receipt — before anything becomes visible. This runtime
//! assembles no neighbor snapshots. Owner count and declared patch accounting
//! are bounded; arbitrary heap usage inside `Any` payloads is not measured or
//! sandboxed.

use super::super::durable::Durability;
use super::super::effects::{EffectKindRegistryFrozen, MAX_EFFECTS_PER_BATCH};
use super::super::journal::{Change, StateKey, SubmitError, Transaction};
use super::super::parallel::{
    BatchId, JobKey, MAX_PHASE_QUEUE_CAPACITY, MAX_PHASE_RESULT_CAPACITY, MAX_PHASE_WORKERS,
    OwnerData, OwnerJob, OwnerKey, OwnerPatch, OwnerSnapshot, OwnerWaveError, OwnerWaveLimits,
    PhaseExecutor, ValidatedOwnerWave,
};
use super::super::registry::{ExecutableSystem, SystemHandlerError, SystemId};
use super::super::simulation::TickId;
use super::owner_durable::{
    DurableOwnerStore, MAX_OWNER_WAVE_BYTES, OwnerDurableError, OwnerSystemConfig, OwnerWalReceipt,
    OwnerWrite, PreparedOwnerWave,
};
use super::owner_effects::{OwnerEffectPatch, route_and_consume};
use std::any::Any;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, ErrorKind};
use std::sync::Arc;

pub(in crate::server) const MAX_OWNER_VALUES_PER_SYSTEM: usize = 16_384;

/// Bound on staged next-tick owner wakes. The set only schedules work that
/// the destination would do on its own rotation; when it fills, producing
/// waves defer with `WouldBlock` instead of growing it.
pub(in crate::server) const MAX_PENDING_OWNER_WAKES: usize = MAX_EFFECTS_PER_BATCH;

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
    pending_wakes: BTreeMap<SystemId, BTreeSet<OwnerKey>>,
    /// When set, routed effects are discarded before any consumer runs while
    /// producer commits still apply. Tests use this to prove delivery is
    /// optional; it is never set by live routing.
    drop_registered_effects: bool,
}

impl SystemRuntime {
    pub fn new(workers: usize) -> io::Result<Self> {
        Self::with_durable_store(workers, DurableOwnerStore::new(Vec::new())?)
    }

    /// Builds the live runtime over journal-recovered owner state. Recovery
    /// runs before the server accepts work, so this store already holds the
    /// last receipted wave for every registered system.
    pub(in crate::server) fn with_durable_store(
        workers: usize,
        durable: DurableOwnerStore,
    ) -> io::Result<Self> {
        if workers == 0 || workers > MAX_PHASE_WORKERS {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("registered-system worker count must be 1..={MAX_PHASE_WORKERS}"),
            ));
        }
        Ok(Self {
            executor: None,
            worker_count: workers,
            durable,
            next_owner: BTreeMap::new(),
            unvalidated_owners: BTreeSet::new(),
            pending_wakes: BTreeMap::new(),
            drop_registered_effects: false,
        })
    }

    /// Installs one system's value codec. Every live system needs its codec
    /// before its first seed or wave: without it the value cannot be encoded
    /// for the WAL, so the wave cannot be staged.
    pub(in crate::server) fn register_owner_system(
        &mut self,
        config: OwnerSystemConfig,
    ) -> io::Result<()> {
        self.durable.register(config)
    }

    /// Installs owner state before its first scheduled tick.
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

    /// Validates a wave against the live store and stages its exact WAL
    /// changes without applying anything. Used by tests driving
    /// multi-domain commits; the live path prepares from validated patches
    /// inside [`SystemRuntime::run_registered`].
    pub(in crate::server) fn prepare_owner_wave(
        &self,
        system: &SystemId,
        writes: Vec<OwnerWrite>,
    ) -> Result<PreparedOwnerWave, OwnerDurableError> {
        self.durable.prepare(system, writes)
    }

    /// Applies WAL-committed owner changes after their receipt, whether they
    /// arrived in a standalone owner wave or piggybacked on an entity
    /// transaction. A before-value mismatch is genuine corruption and stops
    /// the coordinator; capacity can never surface here.
    pub(in crate::server) fn apply_replayed_owner_changes(
        &mut self,
        changes: &[Change],
    ) -> io::Result<()> {
        self.durable.apply_replayed(changes)
    }

    pub fn owner_value<T: Any + Clone + Send + Sync>(
        &self,
        system: &SystemId,
        owner: OwnerKey,
    ) -> Option<(u64, T)> {
        let (revision, data) = self.durable.snapshot(system, owner)?;
        let value = data.get::<T>()?.clone();
        Some((revision, value))
    }

    pub fn worker_count(&self) -> usize {
        self.worker_count
    }

    /// Delivery-shed switch for tests. Live routing never sets this; dropped
    /// effects must leave producer commits untouched while no consumer runs.
    pub(in crate::server) fn set_drop_registered_effects(&mut self, drop: bool) {
        self.drop_registered_effects = drop;
    }

    pub(in crate::server) fn pending_wake_count(&self) -> usize {
        self.pending_wakes.values().map(BTreeSet::len).sum()
    }

    /// Executes one registered handler as an independently ordered owner
    /// batch. `batch_wave` is unique within the phase for this tick.
    ///
    /// Producer patches may carry effect emissions alongside their owner
    /// replacement. At the commit barrier those emissions are routed through
    /// the shared registered-effect machinery and each live destination is
    /// staged in `pending_wakes`, served from its own system's normal job
    /// budget next tick. Any routing, bound, or consumer violation rejects
    /// the whole wave with `WouldBlock` before anything commits.
    ///
    /// The validated wave commits as one main-journal transaction: staged
    /// before-values, one receipt, visibility only after the receipt. WAL
    /// admission (reservations, rotation fence, backpressure) is shared with
    /// every other domain, so a wave never half-applies and never jumps the
    /// coordinator order.
    pub fn run_registered(
        &mut self,
        system: &ExecutableSystem,
        tick: TickId,
        batch_wave: u16,
        effect_kinds: &EffectKindRegistryFrozen,
        durability: &mut Durability,
    ) -> io::Result<usize> {
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
                    "registered system {} declares unsupported neighbor snapshots",
                    system.id().as_str()
                ),
            ));
        }

        let id = system.id().clone();
        // Woken destinations take precedence within the normal job budget
        // next tick; leftovers stay staged. Owners that no longer exist are
        // dropped here: a missing destination only costs latency.
        let mut selected: Vec<OwnerKey> = self
            .pending_wakes
            .remove(&id)
            .unwrap_or_default()
            .into_iter()
            .filter(|owner| self.durable.revision(&id, *owner).is_some())
            .collect();
        if selected.len() > system.max_jobs_per_tick() {
            let leftover: Vec<OwnerKey> = selected.split_off(system.max_jobs_per_tick());
            self.pending_wakes
                .entry(id.clone())
                .or_default()
                .extend(leftover);
        }
        let mut seen: BTreeSet<OwnerKey> = selected.iter().copied().collect();
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
        if self.durable.owner_count(&id) == 0 {
            return Ok(0);
        }

        // The round-robin rotation fills whatever the woken set leaves of
        // this tick's job budget. Woken owners that also appear in the
        // rotation prefix run once; the cursor resumes after the last owner
        // served either way.
        if selected.len() < system.max_jobs_per_tick() {
            for owner in self.durable.owners_from(
                &id,
                self.next_owner.get(&id).copied(),
                system.max_jobs_per_tick(),
            ) {
                if selected.len() >= system.max_jobs_per_tick() {
                    break;
                }
                if seen.insert(owner) {
                    selected.push(owner);
                }
            }
        }
        let next_cursor = self
            .durable
            .successor(&id, *selected.last().expect("selected owners are non-empty"))
            .expect("non-empty owner store has a successor");

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
        for (job_id, owner) in selected.iter().copied().enumerate() {
            let (revision, data) = self.durable.snapshot(&id, owner).ok_or_else(|| {
                io::Error::other(format!(
                    "registered owner {owner:?} vanished before dispatch"
                ))
            })?;
            let snapshot = OwnerSnapshot::new(owner, revision, Arc::new(data));
            let key = JobKey::new(batch, owner, job_id as u64, snapshot.revision());
            let job = OwnerJob::new(id.clone(), key, vec![snapshot])
                .map_err(|error| io::Error::other(format!("registered owner job: {error:?}")))?;
            expected.push(key);
            jobs.push(job);
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
        let staged_wakes: usize = self.pending_wakes.values().map(BTreeSet::len).sum();
        if staged_wakes.saturating_add(wakes.len()) > MAX_PENDING_OWNER_WAKES {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                format!(
                    "registered system {} defers: owner wake queue full",
                    id.as_str()
                ),
            ));
        }
        for (system_id, owner) in wakes {
            self.pending_wakes
                .entry(system_id)
                .or_default()
                .insert(owner);
        }
        let mut writes = Vec::with_capacity(validated.patches().len());
        for patch in validated.patches() {
            let Some(replacement) = OwnerEffectPatch::replacement(patch) else {
                return Err(io::Error::other(format!(
                    "registered owner patch for {:?} has no replacement",
                    patch.owner()
                )));
            };
            writes.push(OwnerWrite {
                owner: patch.owner(),
                reads: patch
                    .revisions()
                    .iter()
                    .map(|stamp| (stamp.owner, stamp.revision))
                    .collect(),
                value: replacement,
                due_tick: None,
            });
        }
        let prepared = self
            .durable
            .prepare(&id, writes)
            .map_err(OwnerDurableError::io)?;
        let applied = self.commit_owner_wave(&id, prepared, tick, durability)?;
        self.next_owner.insert(id, next_cursor);
        Ok(applied)
    }

    /// Submits one prepared owner wave as one main-journal transaction and
    /// commits it only after its receipt. Stale reads and oversized values
    /// reject the wave before anything is staged; journal backpressure and a
    /// requested rotation defer it with `WouldBlock`. A receipted wave whose
    /// in-memory commit fails is genuine corruption and stops the
    /// coordinator, matching the gameplay apply path.
    fn commit_owner_wave(
        &mut self,
        system: &SystemId,
        prepared: PreparedOwnerWave,
        tick: TickId,
        durability: &mut Durability,
    ) -> io::Result<usize> {
        let bytes: usize = prepared
            .changes()
            .iter()
            .map(|change| change.after.len())
            .sum();
        if bytes > MAX_OWNER_WAVE_BYTES {
            return Err(io::Error::new(
                ErrorKind::WouldBlock,
                format!("owner wave of {bytes} bytes exceeds {MAX_OWNER_WAVE_BYTES}"),
            ));
        }
        if durability.failed {
            return Err(io::Error::other(
                "durable subsystem failed; owner wave not staged",
            ));
        }
        if durability.rotation_requested {
            return Err(io::Error::new(
                ErrorKind::WouldBlock,
                "journal rotation in progress; owner wave defers",
            ));
        }
        let keys: Vec<StateKey> = prepared
            .changes()
            .iter()
            .map(|change| change.key.clone())
            .collect();
        for key in &keys {
            if durability.reserved.contains(key) {
                return Err(io::Error::new(
                    ErrorKind::WouldBlock,
                    "owner key has a pending durable action; wave defers",
                ));
            }
        }
        let id = durability.next_id;
        durability.next_id = id
            .checked_add(1)
            .ok_or_else(|| io::Error::other("durable transaction IDs exhausted"))?;
        let transaction = Transaction::new(id, tick.get(), prepared.changes().to_vec());
        let receiver = durability.writer.try_submit(transaction).map_err(|error| match error {
            SubmitError::Full => io::Error::new(
                ErrorKind::WouldBlock,
                "durable journal is full; owner wave defers",
            ),
            SubmitError::Closed => io::Error::other("durable journal writer is closed"),
            SubmitError::Invalid(error) => {
                durability.failed = true;
                error
            }
        })?;
        // Reserved only after the writer accepts the complete transaction,
        // mirroring gameplay staging: a rejected wave reserves nothing.
        durability.reserved.extend(keys.iter().cloned());
        let receipt = match receiver.recv() {
            Ok(Ok(receipt)) => receipt,
            Ok(Err(error)) => {
                for key in &keys {
                    durability.reserved.remove(key);
                }
                durability.failed = true;
                return Err(io::Error::other(format!("durable WAL write failed: {error}")));
            }
            Err(_) => {
                for key in &keys {
                    durability.reserved.remove(key);
                }
                durability.failed = true;
                return Err(io::Error::other("durable journal worker stopped"));
            }
        };
        let applied = self
            .durable
            .commit(
                prepared,
                OwnerWalReceipt {
                    sequence: receipt.sequence,
                },
            )
            .map_err(|error| {
                for key in &keys {
                    durability.reserved.remove(key);
                }
                durability.failed = true;
                error.io()
            })?;
        for key in &keys {
            durability.reserved.remove(key);
        }
        Ok(applied)
    }
}
