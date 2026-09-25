//! Live executor for registered owner handlers without trusted adapters.
//!
//! Each system owns a revisioned store of typed, immutable values. The
//! coordinator selects a stable bounded owner prefix, workers prepare
//! replacements from snapshots, and one validated wave commits through the
//! store. This first runtime slice is transient: it does not persist state,
//! route effects, or assemble neighbor snapshots. Owner count and declared
//! patch accounting are bounded; arbitrary heap usage inside `Any` payloads is
//! not measured or sandboxed.

use super::super::effects::{EffectKindRegistryFrozen, MAX_EFFECTS_PER_BATCH};
use super::super::parallel::{
    BatchId, JobKey, MAX_PHASE_QUEUE_CAPACITY, MAX_PHASE_RESULT_CAPACITY, MAX_PHASE_WORKERS,
    OwnerData, OwnerJob, OwnerKey, OwnerPatch, OwnerStore, OwnerStoreError, OwnerWaveLimits,
    PhaseExecutor, ValidatedOwnerWave,
};
use super::super::registry::{ExecutableSystem, SystemHandlerError, SystemId};
use super::super::simulation::TickId;
use super::owner_effects::{OwnerEffectPatch, route_and_consume};
use std::any::Any;
use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::sync::Arc;

pub(in crate::server) const MAX_OWNER_VALUES_PER_SYSTEM: usize = 16_384;

/// Bound on staged next-tick owner wakes. The set only schedules work that
/// the destination would do on its own rotation; when it fills, producing
/// waves defer with `WouldBlock` instead of growing it.
pub(in crate::server) const MAX_PENDING_OWNER_WAKES: usize = MAX_EFFECTS_PER_BATCH;

pub(in crate::server) struct SystemRuntime {
    executor: Option<PhaseExecutor<OwnerPatch, SystemHandlerError>>,
    worker_count: usize,
    owners: BTreeMap<SystemId, OwnerStore<OwnerData>>,
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
        if workers == 0 || workers > MAX_PHASE_WORKERS {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("registered-system worker count must be 1..={MAX_PHASE_WORKERS}"),
            ));
        }
        Ok(Self {
            executor: None,
            worker_count: workers,
            owners: BTreeMap::new(),
            next_owner: BTreeMap::new(),
            unvalidated_owners: BTreeSet::new(),
            pending_wakes: BTreeMap::new(),
            drop_registered_effects: false,
        })
    }

    /// Installs runtime-local owner state before its first scheduled tick.
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
        let store = self.owners.entry(system.clone()).or_default();
        if store.revision(owner).is_none() && store.len() >= MAX_OWNER_VALUES_PER_SYSTEM {
            return Err(io::Error::other(format!(
                "registered owner store exceeds {MAX_OWNER_VALUES_PER_SYSTEM} owners"
            )));
        }
        store
            .insert(owner, 0, value)
            .map_err(|error| store_error("insert", error))?;
        self.unvalidated_owners.insert(system);
        Ok(())
    }

    pub fn owner_value<T: Any + Clone + Send + Sync>(
        &self,
        system: &SystemId,
        owner: OwnerKey,
    ) -> Option<(u64, T)> {
        let snapshot = self.owners.get(system)?.snapshot(owner).ok()?;
        let value = snapshot.value::<OwnerData>()?.get::<T>()?.clone();
        Some((snapshot.revision(), value))
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
        self.pending_wakes
            .values()
            .map(BTreeSet::len)
            .sum()
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
    pub fn run_registered(
        &mut self,
        system: &ExecutableSystem,
        tick: TickId,
        batch_wave: u16,
        effect_kinds: &EffectKindRegistryFrozen,
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
            .filter(|owner| {
                self.owners
                    .get(&id)
                    .is_some_and(|store| store.revision(*owner).is_some())
            })
            .collect();
        if selected.len() > system.max_jobs_per_tick() {
            let leftover: Vec<OwnerKey> = selected.split_off(system.max_jobs_per_tick());
            self.pending_wakes
                .entry(id.clone())
                .or_default()
                .extend(leftover);
        }
        let mut seen: BTreeSet<OwnerKey> = selected.iter().copied().collect();
        let Some(store) = self.owners.get(&id) else {
            return Ok(0);
        };
        if self.unvalidated_owners.contains(&id) {
            if let Some(owner) = store.owners().find(|owner| !system.accepts_owner(*owner)) {
                return Err(io::Error::other(format!(
                    "registered system {} owns a value in the wrong partition: {owner:?}",
                    id.as_str()
                )));
            }
            self.unvalidated_owners.remove(&id);
        }
        if store.len() == 0 {
            return Ok(0);
        }

        // The round-robin rotation fills whatever the woken set leaves of
        // this tick's job budget. Woken owners that also appear in the
        // rotation prefix run once; the cursor resumes after the last owner
        // served either way.
        if selected.len() < system.max_jobs_per_tick() {
            for owner in store.owners_from(
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
        let next_cursor = store
            .successor(*selected.last().expect("selected owners are non-empty"))
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
            let snapshot = store
                .snapshot(owner)
                .map_err(|error| store_error("snapshot", error))?;
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
                store.revision(key.owner) == Some(key.snapshot_revision)
            })
            .map_err(|error| io::Error::other(format!("registered-system barrier: {error:?}")))?;
        let validated = ValidatedOwnerWave::validate(
            &id,
            &expected,
            results,
            batch,
            limits,
            |owner| store.revision(owner),
            |patch| Ok(OwnerEffectPatch::emitted_count(patch)),
            |patch| {
                if patch.usage().writes != 1 {
                    return Err("one owner replacement must declare exactly one write".into());
                }
                let Some(replacement) = OwnerEffectPatch::replacement(patch) else {
                    return Err("owner patch must contain replacement OwnerData".into());
                };
                let snapshot = store
                    .snapshot(patch.owner())
                    .map_err(|error| format!("current owner state missing: {error:?}"))?;
                let current = snapshot
                    .value::<OwnerData>()
                    .ok_or_else(|| "current owner data has an invalid storage type".to_owned())?;
                if !current.same_type(&replacement) {
                    return Err("owner replacement changes its registered state type".into());
                }
                Ok(())
            },
        )
        .map_err(|error| {
            io::Error::other(format!(
                "registered system {} wave rejected: {error:?}",
                id.as_str()
            ))
        })?;
        // Effects route and consume at the commit barrier, before anything
        // commits: a routing, bound, or consumer violation rejects the whole
        // wave and defers the producing work. Staged wakes are served from
        // each destination system's normal budget next tick, never this one.
        let wakes = route_and_consume(
            &self.owners,
            validated.patches(),
            tick,
            system,
            batch_wave,
            effect_kinds,
            &limits,
            self.drop_registered_effects,
        )
        .map_err(|error| {
            io::Error::other(format!(
                "registered system {} effects rejected: {error}",
                id.as_str()
            ))
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
        let applied = self
            .owners
            .get_mut(&id)
            .expect("owner store validated above")
            .apply_validated_with(validated, |patch| {
                OwnerEffectPatch::replacement(patch).ok_or(OwnerStoreError::WrongPayloadType {
                    owner: patch.owner(),
                })
            })
            .map_err(|error| store_error("commit", error))?;
        self.next_owner.insert(id, next_cursor);
        Ok(applied)
    }
}

fn store_error(action: &str, error: OwnerStoreError) -> io::Error {
    io::Error::other(format!("registered owner-store {action}: {error:?}"))
}
