//! Revision-checked owner patches for executable gameplay systems.
//!
//! A system only sees immutable snapshots and returns a patch. `OwnerWave`
//! validates every result, all captured revisions, and aggregate output bounds
//! before it can hand any patch to the runtime's infallible apply function.

use super::{BatchId, JobCompletion, JobKey, JobOutcome, PhaseResults};
use crate::server::registry::SystemId;
use crate::world::ChunkKey;
use std::any::Any;
use std::cmp::Ordering;
use std::fmt;
use std::sync::Arc;

pub const MAX_EFFECTS_PER_OWNER_JOB: usize = 4_096;
pub const MAX_OWNER_PATCH_WRITES_PER_JOB: usize = 4_096;
pub const MAX_OWNER_WAVE_PATCH_WRITES: usize = 65_536;
pub const MAX_OWNER_PATCH_BYTES_PER_JOB: usize = 1_048_576;
pub const MAX_OWNER_WAVE_PATCH_BYTES: usize = 64 * 1_048_576;

/// Stable ownership identity. Worker placement is deliberately absent.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OwnerKey {
    Chunk(ChunkKey),
    Entity(u64),
    Profile(u128),
}

impl OwnerKey {
    #[cfg(test)]
    pub const fn chunk(key: ChunkKey) -> Self {
        Self::Chunk(key)
    }

    pub const fn as_chunk(self) -> Option<ChunkKey> {
        match self {
            Self::Chunk(key) => Some(key),
            Self::Entity(_) | Self::Profile(_) => None,
        }
    }
}

impl From<ChunkKey> for OwnerKey {
    fn from(value: ChunkKey) -> Self {
        Self::Chunk(value)
    }
}

impl PartialEq<ChunkKey> for OwnerKey {
    fn eq(&self, other: &ChunkKey) -> bool {
        matches!(self, Self::Chunk(key) if key == other)
    }
}

impl PartialEq<OwnerKey> for ChunkKey {
    fn eq(&self, other: &OwnerKey) -> bool {
        other == self
    }
}

impl Ord for OwnerKey {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Chunk(a), Self::Chunk(b)) => (a.x, a.y, a.z).cmp(&(b.x, b.y, b.z)),
            (Self::Entity(a), Self::Entity(b)) => a.cmp(b),
            (Self::Profile(a), Self::Profile(b)) => a.cmp(b),
            (Self::Chunk(_), Self::Entity(_) | Self::Profile(_))
            | (Self::Entity(_), Self::Profile(_)) => Ordering::Less,
            (Self::Entity(_) | Self::Profile(_), Self::Chunk(_))
            | (Self::Profile(_), Self::Entity(_)) => Ordering::Greater,
        }
    }
}

impl PartialOrd for OwnerKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// One authoritative owner revision captured when a job is prepared.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OwnerRevision {
    pub owner: OwnerKey,
    pub revision: u64,
}

/// Immutable, typed owner data supplied to a system job.
#[derive(Clone)]
pub struct OwnerSnapshot {
    owner: OwnerKey,
    revision: u64,
    value: Arc<dyn Any + Send + Sync>,
}

impl OwnerSnapshot {
    pub fn new<T: Any + Send + Sync>(owner: OwnerKey, revision: u64, value: Arc<T>) -> Self {
        Self {
            owner,
            revision,
            value,
        }
    }

    pub const fn owner(&self) -> OwnerKey {
        self.owner
    }

    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub fn value<T: Any + Send + Sync>(&self) -> Option<&T> {
        self.value.downcast_ref()
    }

    #[allow(
        dead_code,
        reason = "Extension handlers may retain an immutable typed snapshot Arc."
    )]
    pub fn shared_value<T: Any + Send + Sync>(&self) -> Option<Arc<T>> {
        Arc::clone(&self.value).downcast().ok()
    }

    fn stamp(&self) -> OwnerRevision {
        OwnerRevision {
            owner: self.owner,
            revision: self.revision,
        }
    }
}

impl fmt::Debug for OwnerSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OwnerSnapshot")
            .field("owner", &self.owner)
            .field("revision", &self.revision)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OwnerJobError {
    MissingPrimarySnapshot { owner: OwnerKey, revision: u64 },
    DuplicateSnapshotOwner { owner: OwnerKey },
}

/// Read-only inputs for one owner-local handler invocation.
#[derive(Clone, Debug)]
pub struct OwnerJob {
    system: SystemId,
    key: JobKey,
    snapshots: Vec<OwnerSnapshot>,
    owner_chunk: Option<Arc<crate::world::Chunk>>,
    owner_catalog: Option<Arc<crate::content::Catalog>>,
}

impl OwnerJob {
    pub fn new(
        system: SystemId,
        key: JobKey,
        mut snapshots: Vec<OwnerSnapshot>,
    ) -> Result<Self, OwnerJobError> {
        snapshots.sort_by_key(OwnerSnapshot::owner);
        for pair in snapshots.windows(2) {
            if pair[0].owner == pair[1].owner {
                return Err(OwnerJobError::DuplicateSnapshotOwner {
                    owner: pair[0].owner,
                });
            }
        }
        if !snapshots.iter().any(|snapshot| {
            snapshot.owner == key.owner && snapshot.revision == key.snapshot_revision
        }) {
            return Err(OwnerJobError::MissingPrimarySnapshot {
                owner: key.owner,
                revision: key.snapshot_revision,
            });
        }
        Ok(Self {
            system,
            key,
            snapshots,
            owner_chunk: None,
            owner_catalog: None,
        })
    }

    pub(in crate::server) fn with_owner_chunk(
        mut self,
        chunk: Arc<crate::world::Chunk>,
        catalog: Arc<crate::content::Catalog>,
    ) -> Self {
        self.owner_chunk = Some(chunk);
        self.owner_catalog = Some(catalog);
        self
    }

    pub(in crate::server) fn owner_chunk(&self) -> Option<&crate::world::Chunk> {
        self.owner_chunk.as_deref()
    }

    pub(in crate::server) fn owner_catalog(&self) -> Option<&crate::content::Catalog> {
        self.owner_catalog.as_deref()
    }

    pub fn system(&self) -> &SystemId {
        &self.system
    }

    pub const fn key(&self) -> JobKey {
        self.key
    }

    pub const fn owner(&self) -> OwnerKey {
        self.key.owner
    }

    #[allow(
        dead_code,
        reason = "Extension handlers may inspect all declared owner read dependencies."
    )]
    pub fn snapshots(&self) -> &[OwnerSnapshot] {
        &self.snapshots
    }

    pub fn snapshot(&self, owner: OwnerKey) -> Option<&OwnerSnapshot> {
        self.snapshots
            .binary_search_by_key(&owner, OwnerSnapshot::owner)
            .ok()
            .map(|index| &self.snapshots[index])
    }

    fn revisions(&self) -> Vec<OwnerRevision> {
        self.snapshots.iter().map(OwnerSnapshot::stamp).collect()
    }
}

/// Bounds reported by one handler result. Effect deliveries count routed
/// destination deliveries, not merely emitted effect objects.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PatchUsage {
    pub writes: usize,
    pub effects: usize,
    pub estimated_bytes: usize,
}

/// Eligibility after this replacement commits. Wakes may run an owner sooner;
/// the handler then explicitly chooses its next schedule again. This is not a
/// sleep policy: every owner remains active or has a persisted deadline.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OwnerSchedule {
    /// Remain eligible on every ordinary rotation (the legacy default).
    #[default]
    Active,
    /// Become ordinarily eligible at this absolute logical tick. Registered
    /// dispatch rejects deadlines not strictly after the producing job's tick.
    // Extension-facing policy; built-in adapters do not emit owner patches.
    #[allow(dead_code)]
    AtTick(u64),
}

/// Scratch output from one handler. Its patch value is type-erased only at the
/// registry boundary; runtime code downcasts it to the system's typed patch.
pub struct OwnerPatch {
    system: SystemId,
    key: JobKey,
    owner: OwnerKey,
    revisions: Vec<OwnerRevision>,
    usage: PatchUsage,
    schedule: OwnerSchedule,
    payload: Box<dyn Any + Send>,
}

impl OwnerPatch {
    pub fn new<T: Any + Send>(job: &OwnerJob, payload: T, usage: PatchUsage) -> Self {
        Self {
            system: job.system.clone(),
            key: job.key,
            owner: job.key.owner,
            revisions: job.revisions(),
            usage,
            schedule: OwnerSchedule::Active,
            payload: Box::new(payload),
        }
    }

    pub const fn key(&self) -> JobKey {
        self.key
    }

    pub const fn owner(&self) -> OwnerKey {
        self.owner
    }

    pub fn revisions(&self) -> &[OwnerRevision] {
        &self.revisions
    }

    pub const fn usage(&self) -> PatchUsage {
        self.usage
    }

    /// Schedule is part of the same validated, receipted replacement as state,
    /// for both plain OwnerData and effect-bearing payloads.
    // Registration API exercised through live dispatch by scheduling tests;
    // built-in adapters do not currently produce scheduled owner patches.
    #[allow(dead_code)]
    pub fn with_schedule(mut self, schedule: OwnerSchedule) -> Self {
        self.schedule = schedule;
        self
    }

    pub const fn schedule(&self) -> OwnerSchedule {
        self.schedule
    }

    pub fn payload<T: Any>(&self) -> Option<&T> {
        self.payload.downcast_ref()
    }

    #[allow(
        clippy::result_large_err,
        reason = "A failed downcast returns the original owned patch intact without allocating an error wrapper."
    )]
    pub fn into_payload<T: Any + Send>(self) -> Result<T, Self> {
        let Self {
            system,
            key,
            owner,
            revisions,
            usage,
            schedule,
            payload,
        } = self;
        match payload.downcast::<T>() {
            Ok(payload) => Ok(*payload),
            Err(payload) => Err(Self {
                system,
                key,
                owner,
                revisions,
                usage,
                schedule,
                payload,
            }),
        }
    }
}

impl fmt::Debug for OwnerPatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OwnerPatch")
            .field("system", &self.system)
            .field("key", &self.key)
            .field("owner", &self.owner)
            .field("revisions", &self.revisions)
            .field("usage", &self.usage)
            .field("schedule", &self.schedule)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OwnerWaveLimits {
    pub max_jobs: usize,
    pub max_writes_per_job: usize,
    pub max_writes: usize,
    pub max_effects_per_job: usize,
    pub max_effect_deliveries: usize,
    pub max_patch_bytes_per_job: usize,
    pub max_patch_bytes: usize,
}

impl OwnerWaveLimits {
    pub const fn new(
        max_jobs: usize,
        max_effect_deliveries: usize,
        max_patch_bytes: usize,
    ) -> Self {
        Self {
            max_jobs,
            max_writes_per_job: MAX_OWNER_PATCH_WRITES_PER_JOB,
            max_writes: MAX_OWNER_WAVE_PATCH_WRITES,
            max_effects_per_job: MAX_EFFECTS_PER_OWNER_JOB,
            max_effect_deliveries,
            max_patch_bytes_per_job: MAX_OWNER_PATCH_BYTES_PER_JOB,
            max_patch_bytes,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OwnerWaveError {
    WrongBatch {
        expected: BatchId,
        actual: BatchId,
    },
    WrongSystem {
        expected: SystemId,
        actual: SystemId,
    },
    MissingResult {
        key: JobKey,
    },
    UnexpectedResult {
        key: JobKey,
    },
    DuplicateExpectedJob {
        key: JobKey,
    },
    ResultOwnerMismatch {
        key: JobKey,
        group_owner: OwnerKey,
    },
    HandlerFailed {
        key: JobKey,
        message: String,
    },
    HandlerPanicked {
        key: JobKey,
        message: String,
    },
    Cancelled {
        key: JobKey,
    },
    Stale {
        owner: OwnerKey,
        expected: u64,
        actual: Option<u64>,
    },
    MismatchedPatchOwner {
        key: JobKey,
        patch_owner: OwnerKey,
    },
    DuplicateOwner {
        owner: OwnerKey,
    },
    TooManyJobs {
        actual: usize,
        limit: usize,
    },
    JobWriteOverflow {
        key: JobKey,
        actual: usize,
        limit: usize,
    },
    PhaseWriteOverflow {
        actual: usize,
        limit: usize,
    },
    JobEffectOverflow {
        key: JobKey,
        actual: usize,
        limit: usize,
    },
    PhaseEffectOverflow {
        actual: usize,
        limit: usize,
    },
    JobPatchTooLarge {
        key: JobKey,
        actual: usize,
        limit: usize,
    },
    PhasePatchTooLarge {
        actual: usize,
        limit: usize,
    },
    InvalidPatch {
        key: JobKey,
        message: String,
    },
}

/// A fully checked system wave. No patch is exposed for application until all
/// job outcomes, read revisions, payload checks, and aggregate budgets pass.
#[derive(Debug)]
pub struct ValidatedOwnerWave {
    patches: Vec<OwnerPatch>,
    #[cfg(test)]
    effect_deliveries: usize,
    #[cfg(test)]
    patch_bytes: usize,
}

impl ValidatedOwnerWave {
    #[allow(
        clippy::too_many_arguments,
        reason = "Keep batch identity, budgets, and each independent validation dependency explicit at the barrier."
    )]
    pub fn validate<E: fmt::Debug>(
        expected_system: &SystemId,
        expected_jobs: &[JobKey],
        results: PhaseResults<OwnerPatch, E>,
        expected_batch: BatchId,
        limits: OwnerWaveLimits,
        mut current_revision: impl FnMut(OwnerKey) -> Option<u64>,
        mut expanded_deliveries: impl FnMut(&OwnerPatch) -> Result<usize, String>,
        mut validate_patch: impl FnMut(&OwnerPatch) -> Result<(), String>,
    ) -> Result<Self, OwnerWaveError> {
        if results.batch != expected_batch {
            return Err(OwnerWaveError::WrongBatch {
                expected: expected_batch,
                actual: results.batch,
            });
        }

        let mut completions = Vec::<JobCompletion<OwnerPatch, E>>::new();
        for owner_results in results.owners {
            for completion in owner_results.jobs {
                if completion.key.owner != owner_results.owner {
                    return Err(OwnerWaveError::ResultOwnerMismatch {
                        key: completion.key,
                        group_owner: owner_results.owner,
                    });
                }
                completions.push(completion);
            }
        }
        let mut expected = expected_jobs.to_vec();
        expected.sort_unstable();
        for pair in expected.windows(2) {
            if pair[0] == pair[1] {
                return Err(OwnerWaveError::DuplicateExpectedJob { key: pair[0] });
            }
        }
        completions.sort_by_key(|completion| completion.key);
        let mut actual = completions.iter().map(|completion| completion.key);
        let mut expected_iter = expected.iter().copied();
        loop {
            match (expected_iter.next(), actual.next()) {
                (Some(expected_key), Some(actual_key)) if expected_key == actual_key => {}
                (Some(expected_key), Some(actual_key)) if expected_key < actual_key => {
                    return Err(OwnerWaveError::MissingResult { key: expected_key });
                }
                (Some(_), Some(actual_key)) => {
                    return Err(OwnerWaveError::UnexpectedResult { key: actual_key });
                }
                (Some(expected_key), None) => {
                    return Err(OwnerWaveError::MissingResult { key: expected_key });
                }
                (None, Some(actual_key)) => {
                    return Err(OwnerWaveError::UnexpectedResult { key: actual_key });
                }
                (None, None) => break,
            }
        }

        let mut patches = Vec::new();
        for completion in completions {
            let key = completion.key;
            match completion.outcome {
                JobOutcome::Completed(patch) => patches.push(patch),
                JobOutcome::Failed(error) => {
                    return Err(OwnerWaveError::HandlerFailed {
                        key,
                        message: format!("{error:?}"),
                    });
                }
                JobOutcome::Panicked(message) => {
                    return Err(OwnerWaveError::HandlerPanicked { key, message });
                }
                JobOutcome::Cancelled => return Err(OwnerWaveError::Cancelled { key }),
                JobOutcome::Stale => {
                    return Err(OwnerWaveError::Stale {
                        owner: key.owner,
                        expected: key.snapshot_revision,
                        actual: current_revision(key.owner),
                    });
                }
            }
        }
        if patches.len() > limits.max_jobs {
            return Err(OwnerWaveError::TooManyJobs {
                actual: patches.len(),
                limit: limits.max_jobs,
            });
        }

        patches.sort_by_key(OwnerPatch::key);
        let mut owners = Vec::with_capacity(patches.len());
        let mut writes = 0usize;
        let mut deliveries = 0usize;
        let mut patch_bytes = 0usize;
        for patch in &patches {
            let key = patch.key;
            if &patch.system != expected_system {
                return Err(OwnerWaveError::WrongSystem {
                    expected: expected_system.clone(),
                    actual: patch.system.clone(),
                });
            }
            if patch.owner != key.owner {
                return Err(OwnerWaveError::MismatchedPatchOwner {
                    key,
                    patch_owner: patch.owner,
                });
            }
            if owners.last().is_some_and(|owner| *owner == patch.owner) {
                return Err(OwnerWaveError::DuplicateOwner { owner: patch.owner });
            }
            owners.push(patch.owner);

            for stamp in &patch.revisions {
                let actual = current_revision(stamp.owner);
                if actual != Some(stamp.revision) {
                    return Err(OwnerWaveError::Stale {
                        owner: stamp.owner,
                        expected: stamp.revision,
                        actual,
                    });
                }
            }

            if patch.usage.writes > limits.max_writes_per_job {
                return Err(OwnerWaveError::JobWriteOverflow {
                    key,
                    actual: patch.usage.writes,
                    limit: limits.max_writes_per_job,
                });
            }
            writes = writes.saturating_add(patch.usage.writes);
            if writes > limits.max_writes {
                return Err(OwnerWaveError::PhaseWriteOverflow {
                    actual: writes,
                    limit: limits.max_writes,
                });
            }

            if patch.usage.effects > limits.max_effects_per_job {
                return Err(OwnerWaveError::JobEffectOverflow {
                    key,
                    actual: patch.usage.effects,
                    limit: limits.max_effects_per_job,
                });
            }
            let job_deliveries = expanded_deliveries(patch)
                .map_err(|message| OwnerWaveError::InvalidPatch { key, message })?;
            deliveries = deliveries.saturating_add(job_deliveries);
            if deliveries > limits.max_effect_deliveries {
                return Err(OwnerWaveError::PhaseEffectOverflow {
                    actual: deliveries,
                    limit: limits.max_effect_deliveries,
                });
            }

            if patch.usage.estimated_bytes > limits.max_patch_bytes_per_job {
                return Err(OwnerWaveError::JobPatchTooLarge {
                    key,
                    actual: patch.usage.estimated_bytes,
                    limit: limits.max_patch_bytes_per_job,
                });
            }
            patch_bytes = patch_bytes.saturating_add(patch.usage.estimated_bytes);
            if patch_bytes > limits.max_patch_bytes {
                return Err(OwnerWaveError::PhasePatchTooLarge {
                    actual: patch_bytes,
                    limit: limits.max_patch_bytes,
                });
            }
            validate_patch(patch)
                .map_err(|message| OwnerWaveError::InvalidPatch { key, message })?;
        }

        Ok(Self {
            patches,
            #[cfg(test)]
            effect_deliveries: deliveries,
            #[cfg(test)]
            patch_bytes,
        })
    }

    pub fn patches(&self) -> &[OwnerPatch] {
        &self.patches
    }

    #[cfg(test)]
    pub const fn effect_deliveries(&self) -> usize {
        self.effect_deliveries
    }

    #[cfg(test)]
    pub const fn patch_bytes(&self) -> usize {
        self.patch_bytes
    }

    /// Runs only trusted runtime application code, in stable owner order.
    /// The callback must apply a patch without invoking its registered handler.
    pub fn apply(self, mut apply: impl FnMut(OwnerPatch)) -> usize {
        let count = self.patches.len();
        for patch in self.patches {
            apply(patch);
        }
        count
    }
}

#[cfg(test)]
#[path = "owner_wave/tests.rs"]
mod tests;
