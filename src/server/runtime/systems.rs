//! Live executor for registered owner handlers without trusted adapters.
//!
//! Each system owns a revisioned store of typed, immutable values. The
//! coordinator selects a stable bounded owner prefix, workers prepare
//! replacements from snapshots, and one validated wave commits through the
//! store. This first runtime slice is transient: it does not persist state,
//! route effects, or assemble neighbor snapshots. Owner count and declared
//! patch accounting are bounded; arbitrary heap usage inside `Any` payloads is
//! not measured or sandboxed.

use super::super::parallel::{
    BatchId, JobKey, MAX_PHASE_QUEUE_CAPACITY, MAX_PHASE_RESULT_CAPACITY, MAX_PHASE_WORKERS,
    OwnerData, OwnerJob, OwnerKey, OwnerPatch, OwnerStore, OwnerStoreError, OwnerWaveLimits,
    PhaseExecutor, ValidatedOwnerWave,
};
use super::super::registry::{ExecutableSystem, SystemHandlerError, SystemId};
use super::super::simulation::TickId;
use std::any::Any;
use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::sync::Arc;

pub(in crate::server) const MAX_OWNER_VALUES_PER_SYSTEM: usize = 16_384;

pub(in crate::server) struct SystemRuntime {
    executor: Option<PhaseExecutor<OwnerPatch, SystemHandlerError>>,
    worker_count: usize,
    owners: BTreeMap<SystemId, OwnerStore<OwnerData>>,
    next_owner: BTreeMap<SystemId, OwnerKey>,
    unvalidated_owners: BTreeSet<SystemId>,
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

    /// Executes one registered handler as an independently ordered owner
    /// batch. `batch_wave` is unique within the phase for this tick.
    pub fn run_registered(
        &mut self,
        system: &ExecutableSystem,
        tick: TickId,
        batch_wave: u16,
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
        let Some(store) = self.owners.get_mut(&id) else {
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

        let selected = store.owners_from(
            self.next_owner.get(&id).copied(),
            system.max_jobs_per_tick(),
        );
        let next_cursor = store
            .successor(*selected.last().expect("non-empty owner store admits a job"))
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
            |patch| {
                if patch.usage().effects == 0 {
                    Ok(0)
                } else {
                    Err("registered owner effects have no live route".into())
                }
            },
            |patch| {
                if patch.usage().effects != 0 {
                    return Err("registered owner effects have no live route".into());
                }
                if patch.usage().writes != 1 {
                    return Err("one owner replacement must declare exactly one write".into());
                }
                let Some(replacement) = patch.payload::<OwnerData>() else {
                    return Err("owner patch must contain replacement OwnerData".into());
                };
                let snapshot = store
                    .snapshot(patch.owner())
                    .map_err(|error| format!("current owner state missing: {error:?}"))?;
                let current = snapshot
                    .value::<OwnerData>()
                    .ok_or_else(|| "current owner data has an invalid storage type".to_owned())?;
                if !current.same_type(replacement) {
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
        let applied = store
            .apply_validated_with(validated, |patch| {
                patch
                    .payload::<OwnerData>()
                    .cloned()
                    .ok_or(OwnerStoreError::WrongPayloadType {
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
