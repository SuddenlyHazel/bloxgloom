//! Fixed-worker post-WAL installation of disjoint authoritative chunk owners.

use super::scheduler::FireRuntime;
use crate::server::parallel::{BatchId, JobKey, JobOutcome, OwnerKey};
use crate::server::simulation::{Phase, TickId};
use crate::world::{ChunkKey, PreparedEdit, World};
use std::collections::BTreeMap;
use std::io;
use std::time::{Duration, Instant};

const MAX_APPLY_JOBS_PER_BARRIER: usize = 64;
const MIN_OWNER_TASKS_PER_WORKER_GROUP: usize = 16;

#[derive(Clone, Copy, Debug, Default)]
pub(in crate::server) struct FireApplyTimings {
    pub(in crate::server) capture_and_validate: Duration,
    pub(in crate::server) worker_barrier: Duration,
    pub(in crate::server) metadata_finalize: Duration,
    pub(in crate::server) worker_run_time: Duration,
    pub(in crate::server) submitted_worker_groups: usize,
    pub(in crate::server) max_worker_groups_per_barrier: usize,
}

impl FireApplyTimings {
    pub(in crate::server) fn total(self) -> Duration {
        self.capture_and_validate + self.worker_barrier + self.metadata_finalize
    }
}

impl FireRuntime {
    /// Applies only already-synced, whole-batch-validated owner edits. The
    /// immutable after-chunks were prepared and encoded before WAL admission;
    /// each worker checks and installs one disjoint owner slot. A failure stops
    /// the server before public effects and requires WAL replay.
    pub(in crate::server) fn apply_synced_world_edits(
        &mut self,
        world: &mut World,
        edits: Vec<PreparedEdit>,
    ) -> io::Result<FireApplyTimings> {
        if edits.is_empty() {
            return Ok(FireApplyTimings::default());
        }
        let started = Instant::now();
        world.validate_owner_apply_batch(&edits)?;
        let capacity = world.owner_apply_capacity().min(MAX_APPLY_JOBS_PER_BARRIER);
        let mut edits = edits.into_iter();
        let mut timings = FireApplyTimings {
            capture_and_validate: started.elapsed(),
            ..FireApplyTimings::default()
        };
        loop {
            let chunk: Vec<_> = edits.by_ref().take(capacity).collect();
            if chunk.is_empty() {
                break;
            }
            let capture_started = Instant::now();
            let tasks = world.prepare_owner_apply_batch(chunk)?;
            self.apply_sequence = self
                .apply_sequence
                .checked_add(1)
                .ok_or_else(|| io::Error::other("fire apply barrier sequence exhausted"))?;
            let batch = BatchId::new(TickId::new(self.apply_sequence), Phase::DurableActions, 0);
            let mut expected = BTreeMap::<ChunkKey, u64>::new();
            let mut admission_error = None;
            // A checked Arc swap is small. Keep enough disjoint owner tasks
            // in each worker closure to amortize queue/barrier scheduling;
            // larger batches still grow to the configured worker count.
            let worker_count = tasks
                .len()
                .div_ceil(MIN_OWNER_TASKS_PER_WORKER_GROUP)
                .min(self.apply_executor.worker_count());
            timings.submitted_worker_groups += worker_count;
            timings.max_worker_groups_per_barrier =
                timings.max_worker_groups_per_barrier.max(worker_count);
            let mut groups: Vec<Vec<_>> = std::iter::repeat_with(Vec::new)
                .take(worker_count)
                .collect();
            for (index, task) in tasks.into_iter().enumerate() {
                expected.insert(task.key(), task.expected_version());
                groups[index % worker_count].push(task);
            }
            for group in groups {
                let owner = group[0].key();
                let version = group[0].expected_version();
                let key = JobKey::new(batch, owner, 0, version);
                if let Err(error) = self.apply_executor.try_submit(key, move |_| {
                    group.into_iter().map(|task| task.run()).collect()
                }) {
                    admission_error = Some(format!("post-WAL owner worker admission: {error:?}"));
                    break;
                }
            }
            timings.capture_and_validate += capture_started.elapsed();

            let barrier_started = Instant::now();
            let results = self
                .apply_executor
                .barrier(batch)
                .map_err(|error| io::Error::other(format!("post-WAL owner barrier: {error:?}")))?;
            timings.worker_barrier += barrier_started.elapsed();
            timings.worker_run_time += results.worker_run_time();
            if let Some(error) = admission_error {
                return Err(io::Error::other(error));
            }
            let mut receipts = Vec::with_capacity(expected.len());
            for owner in results.owners {
                let OwnerKey::Chunk(chunk) = owner.owner else {
                    return Err(io::Error::other("post-WAL owner result has non-chunk key"));
                };
                for job in owner.jobs {
                    if expected.get(&chunk) != Some(&job.key.snapshot_revision)
                        || job.key.owner != owner.owner
                    {
                        return Err(io::Error::other("post-WAL owner result key mismatch"));
                    }
                    match job.outcome {
                        JobOutcome::Completed(group_receipts) => {
                            for receipt in group_receipts {
                                if expected.remove(&receipt.key()).is_none() {
                                    return Err(io::Error::other(
                                        "post-WAL owner receipt key mismatch or duplicate",
                                    ));
                                }
                                receipts.push(receipt);
                            }
                        }
                        JobOutcome::Failed(error) => return Err(error),
                        JobOutcome::Panicked(error) => {
                            return Err(io::Error::other(format!(
                                "post-WAL owner worker panicked: {error}"
                            )));
                        }
                        JobOutcome::Cancelled | JobOutcome::Stale => {
                            return Err(io::Error::other(
                                "post-WAL owner worker was cancelled or stale",
                            ));
                        }
                    }
                }
            }
            if !expected.is_empty() {
                return Err(io::Error::other("post-WAL owner barrier lost a result"));
            }
            let finalize_started = Instant::now();
            world.finish_owner_apply_batch(receipts)?;
            timings.metadata_finalize += finalize_started.elapsed();
        }
        Ok(timings)
    }
}
