//! Bounded fixed-worker execution for pure, owner-scoped phase jobs.
//!
//! Jobs receive no mutable world handle. Capture immutable `Arc` snapshots and
//! owned inputs; collect every accepted job at its phase barrier before applying
//! any owner result. Region scheduling may regroup chunk owners, but never
//! changes a job's stable ordering key.

use super::simulation::{Phase, TickId};
use crate::world::ChunkKey;
use std::any::Any;
use std::cmp::Ordering as CmpOrdering;
use std::collections::{HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

pub const MAX_PHASE_WORKERS: usize = 64;
pub const MAX_PHASE_QUEUE_CAPACITY: usize = 16_384;
pub const MAX_PHASE_RESULT_CAPACITY: usize = 16_384;

/// Identity of one executor barrier. A tick can contain multiple dependency
/// waves and phases; total ordering is tick, phase, then wave.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BatchId {
    tick: TickId,
    phase: Phase,
    wave: u16,
}

impl BatchId {
    pub const fn new(tick: TickId, phase: Phase, wave: u16) -> Self {
        Self { tick, phase, wave }
    }

    /// Tick encoded by this barrier.
    ///
    /// Built-in dispatch currently needs only the ordered ID itself;
    /// extension systems use the component for scheduling and diagnostics.
    #[allow(dead_code)]
    pub const fn tick(self) -> TickId {
        self.tick
    }

    /// Phase encoded by this barrier.
    ///
    /// Built-in dispatch currently needs only the ordered ID itself;
    /// extension systems use the component for scheduling and diagnostics.
    #[allow(dead_code)]
    pub const fn phase(self) -> Phase {
        self.phase
    }

    /// Dependency wave encoded by this barrier. Built-ins currently use wave
    /// zero; extension systems can use later waves after dependency barriers.
    #[allow(dead_code)]
    pub const fn wave(self) -> u16 {
        self.wave
    }
}

/// Stable job identity. A snapshot revision lets the barrier discard work that
/// finished against an obsolete immutable owner snapshot.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct JobKey {
    pub batch: BatchId,
    pub owner: ChunkKey,
    pub job_id: u64,
    pub snapshot_revision: u64,
}

impl JobKey {
    pub const fn new(batch: BatchId, owner: ChunkKey, job_id: u64, snapshot_revision: u64) -> Self {
        Self {
            batch,
            owner,
            job_id,
            snapshot_revision,
        }
    }
}

impl PartialOrd for JobKey {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}

impl Ord for JobKey {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        self.batch
            .cmp(&other.batch)
            .then_with(|| owner_order(self.owner).cmp(&owner_order(other.owner)))
            .then_with(|| self.job_id.cmp(&other.job_id))
            .then_with(|| self.snapshot_revision.cmp(&other.snapshot_revision))
    }
}

fn owner_order(owner: ChunkKey) -> (i32, i32, i32) {
    (owner.x, owner.y, owner.z)
}

/// Cooperative cancellation shared with one job. Workers also check this token
/// before starting queued work and again before publishing its result.
#[derive(Clone, Debug)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
}

pub type PhaseJob<R, E> = Box<dyn FnOnce(CancellationToken) -> Result<R, E> + Send + 'static>;

struct WorkerTask<R, E> {
    key: JobKey,
    cancellation: CancellationToken,
    run: PhaseJob<R, E>,
}

enum WorkerOutcome<R, E> {
    Finished(Result<R, E>),
    Panicked(String),
    Cancelled,
}

struct WorkerCompletion<R, E> {
    key: JobKey,
    cancellation: CancellationToken,
    outcome: WorkerOutcome<R, E>,
}

enum WorkerMessage<R, E> {
    Completed(WorkerCompletion<R, E>),
    Stopped,
}

#[derive(Default)]
struct PendingBatch {
    keys: HashSet<JobKey>,
    cancellations: Vec<CancellationToken>,
}

/// Result state returned only after every accepted job for a batch has reached
/// the barrier. Stale and cancelled values are dropped before owner commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobOutcome<R, E> {
    Completed(R),
    Failed(E),
    Panicked(String),
    Cancelled,
    Stale,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobCompletion<R, E> {
    pub key: JobKey,
    pub outcome: JobOutcome<R, E>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnerResults<R, E> {
    pub owner: ChunkKey,
    /// Jobs are ordered by stable job ID (then snapshot revision).
    pub jobs: Vec<JobCompletion<R, E>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhaseResults<R, E> {
    pub batch: BatchId,
    /// Owners are ordered lexicographically by chunk coordinate.
    pub owners: Vec<OwnerResults<R, E>>,
}

impl<R, E> PhaseResults<R, E> {
    /// Borrow stable owner results without taking ownership of the phase
    /// output. Built-in movement currently consumes the vector directly.
    #[allow(dead_code)]
    pub fn owners(&self) -> &[OwnerResults<R, E>] {
        &self.owners
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecutorConfigError {
    NoWorkers,
    TooManyWorkers { requested: usize, maximum: usize },
    ZeroQueueCapacity,
    QueueCapacityTooLarge { requested: usize, maximum: usize },
    ZeroResultCapacity,
    ResultCapacityTooLarge { requested: usize, maximum: usize },
    ThreadSpawn(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmitError {
    QueueSaturated {
        key: JobKey,
        capacity: usize,
    },
    DuplicateKey {
        key: JobKey,
    },
    ClosedBatch {
        key: JobKey,
        closed_through: BatchId,
    },
    OutOfOrderBatch {
        key: JobKey,
        latest_submitted: BatchId,
    },
    EarlierBatchPending {
        key: JobKey,
        pending: BatchId,
    },
    WorkerPoolStopped {
        key: JobKey,
    },
}

/// Failure to cancel speculative work. Built-in jobs currently drain at their
/// barriers; extension systems can cancel superseded speculative batches.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelError {
    UnknownBatch {
        batch: BatchId,
    },
    ClosedBatch {
        batch: BatchId,
        closed_through: BatchId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarrierError {
    AlreadyClosed {
        batch: BatchId,
        closed_through: BatchId,
    },
    EarlierBatchPending {
        requested: BatchId,
        pending: BatchId,
    },
    WorkerPoolStopped {
        batch: BatchId,
        expected: usize,
        received: usize,
    },
}

/// Fixed OS worker pool with bounded task and result channels.
///
/// `try_submit` never waits for queue capacity. A saturated queue rejects that
/// job explicitly. Job closures are `Send + 'static` and should capture only
/// immutable `Arc` snapshots or owned inputs, never a shared mutable `World`.
/// There is one thread per configured worker, not one per submitted job.
pub struct PhaseExecutor<R: Send + 'static, E: Send + 'static> {
    job_sender: Option<SyncSender<WorkerTask<R, E>>>,
    result_receiver: Receiver<WorkerMessage<R, E>>,
    workers: Vec<JoinHandle<()>>,
    active_workers: usize,
    queue_capacity: usize,
    pending: HashMap<BatchId, PendingBatch>,
    completed: HashMap<BatchId, Vec<WorkerCompletion<R, E>>>,
    closed_through: Option<BatchId>,
    latest_submitted: Option<BatchId>,
}

impl<R: Send + 'static, E: Send + 'static> PhaseExecutor<R, E> {
    pub fn new(
        worker_count: usize,
        queue_capacity: usize,
        result_capacity: usize,
    ) -> Result<Self, ExecutorConfigError> {
        if worker_count == 0 {
            return Err(ExecutorConfigError::NoWorkers);
        }
        if worker_count > MAX_PHASE_WORKERS {
            return Err(ExecutorConfigError::TooManyWorkers {
                requested: worker_count,
                maximum: MAX_PHASE_WORKERS,
            });
        }
        if queue_capacity == 0 {
            return Err(ExecutorConfigError::ZeroQueueCapacity);
        }
        if queue_capacity > MAX_PHASE_QUEUE_CAPACITY {
            return Err(ExecutorConfigError::QueueCapacityTooLarge {
                requested: queue_capacity,
                maximum: MAX_PHASE_QUEUE_CAPACITY,
            });
        }
        if result_capacity == 0 {
            return Err(ExecutorConfigError::ZeroResultCapacity);
        }
        if result_capacity > MAX_PHASE_RESULT_CAPACITY {
            return Err(ExecutorConfigError::ResultCapacityTooLarge {
                requested: result_capacity,
                maximum: MAX_PHASE_RESULT_CAPACITY,
            });
        }

        let (job_sender, job_receiver) = mpsc::sync_channel::<WorkerTask<R, E>>(queue_capacity);
        let (result_sender, result_receiver) =
            mpsc::sync_channel::<WorkerMessage<R, E>>(result_capacity);
        let shared_receiver = Arc::new(Mutex::new(job_receiver));
        let mut workers = Vec::with_capacity(worker_count);

        for index in 0..worker_count {
            let shared_receiver = Arc::clone(&shared_receiver);
            let result_sender = result_sender.clone();
            let worker = thread::Builder::new()
                .name(format!("bloxgloom-phase-{index}"))
                .spawn(move || worker_entry(shared_receiver, result_sender))
                .map_err(|error| ExecutorConfigError::ThreadSpawn(error.to_string()));
            match worker {
                Ok(worker) => workers.push(worker),
                Err(error) => {
                    drop(job_sender);
                    drop(result_receiver);
                    for worker in workers {
                        let _ = worker.join();
                    }
                    return Err(error);
                }
            }
        }
        drop(result_sender);

        Ok(Self {
            job_sender: Some(job_sender),
            result_receiver,
            workers,
            active_workers: worker_count,
            queue_capacity,
            pending: HashMap::new(),
            completed: HashMap::new(),
            closed_through: None,
            latest_submitted: None,
        })
    }

    /// Enqueues one pure/read-only job without blocking for queue space.
    /// Submission batches are monotonic, and every earlier batch must cross its
    /// barrier before a later batch can be submitted.
    pub fn try_submit<F>(&mut self, key: JobKey, run: F) -> Result<(), SubmitError>
    where
        F: FnOnce(CancellationToken) -> Result<R, E> + Send + 'static,
    {
        if self.active_workers == 0 || self.job_sender.is_none() {
            return Err(SubmitError::WorkerPoolStopped { key });
        }
        if let Some(closed_through) = self.closed_through
            && key.batch <= closed_through
        {
            return Err(SubmitError::ClosedBatch {
                key,
                closed_through,
            });
        }
        if let Some(latest_submitted) = self.latest_submitted
            && key.batch < latest_submitted
        {
            return Err(SubmitError::OutOfOrderBatch {
                key,
                latest_submitted,
            });
        }
        if let Some((&pending, _)) = self
            .pending
            .iter()
            .filter(|(pending, _)| **pending < key.batch)
            .min_by_key(|(pending, _)| **pending)
        {
            return Err(SubmitError::EarlierBatchPending { key, pending });
        }
        if self
            .pending
            .get(&key.batch)
            .is_some_and(|pending| pending.keys.contains(&key))
        {
            return Err(SubmitError::DuplicateKey { key });
        }

        let cancellation = CancellationToken::new();
        let task = WorkerTask {
            key,
            cancellation: cancellation.clone(),
            run: Box::new(run),
        };
        let send_result = self
            .job_sender
            .as_ref()
            .expect("live worker pool has a job sender")
            .try_send(task);
        match send_result {
            Ok(()) => {
                let pending = self.pending.entry(key.batch).or_default();
                pending.keys.insert(key);
                pending.cancellations.push(cancellation);
                self.latest_submitted = Some(key.batch);
                Ok(())
            }
            Err(TrySendError::Full(_)) => Err(SubmitError::QueueSaturated {
                key,
                capacity: self.queue_capacity,
            }),
            Err(TrySendError::Disconnected(_)) => Err(SubmitError::WorkerPoolStopped { key }),
        }
    }

    /// Requests cooperative cancellation for every accepted job in this batch.
    /// Built-in jobs currently drain at their barriers; extension systems can
    /// cancel batches whose snapshots have been superseded.
    #[allow(dead_code)]
    pub fn cancel_batch(&mut self, batch: BatchId) -> Result<(), CancelError> {
        if let Some(closed_through) = self.closed_through
            && batch <= closed_through
        {
            return Err(CancelError::ClosedBatch {
                batch,
                closed_through,
            });
        }
        let Some(pending) = self.pending.get(&batch) else {
            return Err(CancelError::UnknownBatch { batch });
        };
        for cancellation in &pending.cancellations {
            cancellation.cancel();
        }
        Ok(())
    }

    /// Waits for all accepted jobs for `batch`, retaining results whose owner
    /// snapshot is still current. Invalidated result values are dropped as Stale.
    pub fn barrier_with<F>(
        &mut self,
        batch: BatchId,
        mut is_current: F,
    ) -> Result<PhaseResults<R, E>, BarrierError>
    where
        F: FnMut(JobKey) -> bool,
    {
        if let Some(closed_through) = self.closed_through
            && batch <= closed_through
        {
            return Err(BarrierError::AlreadyClosed {
                batch,
                closed_through,
            });
        }
        if let Some((&pending, _)) = self
            .pending
            .iter()
            .filter(|(pending, _)| **pending < batch)
            .min_by_key(|(pending, _)| **pending)
        {
            return Err(BarrierError::EarlierBatchPending {
                requested: batch,
                pending,
            });
        }

        let expected = self
            .pending
            .get(&batch)
            .map_or(0, |pending| pending.keys.len());
        let mut completions = self.completed.remove(&batch).unwrap_or_default();
        if completions.len() > expected {
            return Err(BarrierError::WorkerPoolStopped {
                batch,
                expected,
                received: completions.len(),
            });
        }
        while completions.len() < expected {
            match self.result_receiver.recv() {
                Ok(WorkerMessage::Completed(completion)) if completion.key.batch == batch => {
                    completions.push(completion);
                }
                Ok(WorkerMessage::Completed(completion)) => {
                    self.completed
                        .entry(completion.key.batch)
                        .or_default()
                        .push(completion);
                }
                Ok(WorkerMessage::Stopped) => {
                    self.active_workers = self.active_workers.saturating_sub(1);
                    if self.active_workers == 0 {
                        let received = completions.len();
                        self.completed.entry(batch).or_default().extend(completions);
                        return Err(BarrierError::WorkerPoolStopped {
                            batch,
                            expected,
                            received,
                        });
                    }
                }
                Err(_) => {
                    let received = completions.len();
                    self.completed.entry(batch).or_default().extend(completions);
                    return Err(BarrierError::WorkerPoolStopped {
                        batch,
                        expected,
                        received,
                    });
                }
            }
        }

        completions.sort_by_key(|completion| completion.key);
        let mut owners: Vec<OwnerResults<R, E>> = Vec::new();
        for completion in completions {
            let outcome = if completion.cancellation.is_cancelled() {
                JobOutcome::Cancelled
            } else if !is_current(completion.key) {
                JobOutcome::Stale
            } else {
                match completion.outcome {
                    WorkerOutcome::Finished(Ok(value)) => JobOutcome::Completed(value),
                    WorkerOutcome::Finished(Err(error)) => JobOutcome::Failed(error),
                    WorkerOutcome::Panicked(message) => JobOutcome::Panicked(message),
                    WorkerOutcome::Cancelled => JobOutcome::Cancelled,
                }
            };
            if owners
                .last()
                .is_none_or(|owner_results| owner_results.owner != completion.key.owner)
            {
                owners.push(OwnerResults {
                    owner: completion.key.owner,
                    jobs: Vec::new(),
                });
            }
            owners.last_mut().unwrap().jobs.push(JobCompletion {
                key: completion.key,
                outcome,
            });
        }

        self.pending.remove(&batch);
        self.closed_through = Some(batch);
        Ok(PhaseResults { batch, owners })
    }

    /// Barrier variant for batches that do not need revision checks.
    pub fn barrier(&mut self, batch: BatchId) -> Result<PhaseResults<R, E>, BarrierError> {
        self.barrier_with(batch, |_| true)
    }
}

impl<R: Send + 'static, E: Send + 'static> Drop for PhaseExecutor<R, E> {
    fn drop(&mut self) {
        for pending in self.pending.values() {
            for cancellation in &pending.cancellations {
                cancellation.cancel();
            }
        }
        drop(self.job_sender.take());

        let mut stopped = 0usize;
        while stopped < self.active_workers {
            match self.result_receiver.recv() {
                Ok(WorkerMessage::Stopped) => stopped += 1,
                Ok(WorkerMessage::Completed(_)) => {}
                Err(_) => break,
            }
        }
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn worker_entry<R: Send + 'static, E: Send + 'static>(
    receiver: Arc<Mutex<Receiver<WorkerTask<R, E>>>>,
    result_sender: SyncSender<WorkerMessage<R, E>>,
) {
    let stopped_sender = result_sender.clone();
    let _ = catch_unwind(AssertUnwindSafe(|| worker_loop(receiver, result_sender)));
    let _ = stopped_sender.send(WorkerMessage::Stopped);
}

fn worker_loop<R: Send + 'static, E: Send + 'static>(
    receiver: Arc<Mutex<Receiver<WorkerTask<R, E>>>>,
    result_sender: SyncSender<WorkerMessage<R, E>>,
) {
    loop {
        let received = receiver
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .recv();
        let Ok(task) = received else {
            break;
        };

        let WorkerTask {
            key,
            cancellation,
            run,
        } = task;
        let computed = catch_unwind(AssertUnwindSafe(|| {
            if cancellation.is_cancelled() {
                return WorkerOutcome::Cancelled;
            }
            let result = run(cancellation.clone());
            if cancellation.is_cancelled() {
                WorkerOutcome::Cancelled
            } else {
                WorkerOutcome::Finished(result)
            }
        }));
        let outcome = match computed {
            Ok(outcome) => outcome,
            Err(_payload) if cancellation.is_cancelled() => WorkerOutcome::Cancelled,
            Err(payload) => WorkerOutcome::Panicked(panic_message(payload)),
        };

        if result_sender
            .send(WorkerMessage::Completed(WorkerCompletion {
                key,
                cancellation,
                outcome,
            }))
            .is_err()
        {
            return;
        }
    }
}

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "job panicked with a non-string payload".to_owned()
    }
}

#[cfg(test)]
mod tests;
