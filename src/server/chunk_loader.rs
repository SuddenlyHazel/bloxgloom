//! Bounded asynchronous chunk loading and procedural generation.
//!
//! The server coordinator only submits requests and polls completions. Each
//! fixed worker owns an independent `World`, keeping storage reads and terrain
//! generation out of the tick and network-streaming paths.

use crate::world::{ChunkKey, LoadedChunk, World};
use std::collections::HashMap;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Instant;

#[cfg(test)]
#[path = "chunk_loader/tests.rs"]
mod tests;

const WORKER_COUNT: usize = 2;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(super) struct ChunkLoadTicket {
    pub(super) key: ChunkKey,
    pub(super) generation: u64,
    pub(super) edit_epoch: u64,
    pub(super) requested_at: Instant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RequestStatus {
    Enqueued(ChunkLoadTicket),
    AlreadyPending(ChunkLoadTicket),
}

#[derive(Debug)]
pub(super) enum RequestError {
    QueueFull,
    Stopped,
    GenerationExhausted,
    World(io::Error),
}

impl std::fmt::Display for RequestError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::QueueFull => formatter.write_str("chunk loader capacity is full"),
            Self::Stopped => formatter.write_str("chunk loader is stopped"),
            Self::GenerationExhausted => {
                formatter.write_str("chunk load generation counter exhausted")
            }
            Self::World(error) => write!(formatter, "world rejected chunk-load request: {error}"),
        }
    }
}

impl std::error::Error for RequestError {}

#[derive(Debug)]
pub(super) struct ChunkLoadResult {
    pub(super) ticket: ChunkLoadTicket,
    pub(super) result: io::Result<LoadedChunk>,
}

struct Job {
    ticket: ChunkLoadTicket,
    pending_snapshot: Option<Vec<u8>>,
}

/// A persistent, fixed-size worker pool with a bounded accepted-work budget.
/// Capacity covers queued, executing, and completed-but-unconsumed requests.
pub(super) struct ChunkLoader {
    job_sender: Option<SyncSender<Job>>,
    result_receiver: Option<Receiver<ChunkLoadResult>>,
    workers: Vec<JoinHandle<()>>,
    stopping: Arc<AtomicBool>,
    capacity: usize,
    outstanding: usize,
    next_generation: u64,
    pending: HashMap<ChunkKey, ChunkLoadTicket>,
}

impl ChunkLoader {
    pub(super) fn new(world: &World, capacity: usize) -> io::Result<Self> {
        if capacity == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "chunk loader capacity must be positive",
            ));
        }

        let (job_sender, job_receiver) = mpsc::sync_channel::<Job>(capacity);
        let (result_sender, result_receiver) = mpsc::sync_channel::<ChunkLoadResult>(capacity);
        let job_receiver = Arc::new(Mutex::new(job_receiver));
        let stopping = Arc::new(AtomicBool::new(false));
        let mut workers = Vec::with_capacity(WORKER_COUNT);

        for worker_id in 0..WORKER_COUNT {
            let world = world.loader_view();
            let jobs = Arc::clone(&job_receiver);
            let results = result_sender.clone();
            let stopping_flag = Arc::clone(&stopping);
            match thread::Builder::new()
                .name(format!("chunk-loader-{worker_id}"))
                .spawn(move || worker_loop(world, jobs, results, stopping_flag))
            {
                Ok(worker) => workers.push(worker),
                Err(error) => {
                    stop_workers(&stopping, &mut workers, job_sender, result_receiver);
                    return Err(error);
                }
            }
        }
        drop(result_sender);

        Ok(Self {
            job_sender: Some(job_sender),
            result_receiver: Some(result_receiver),
            workers,
            stopping,
            capacity,
            outstanding: 0,
            next_generation: 0,
            pending: HashMap::new(),
        })
    }

    /// Submits one nonblocking request, deduplicating by chunk key until the
    /// corresponding completion is consumed. A full budget registers no World
    /// load epoch, so the caller can safely retry on a later tick.
    pub(super) fn request(
        &mut self,
        world: &mut World,
        key: ChunkKey,
    ) -> Result<RequestStatus, RequestError> {
        if let Some(ticket) = self.pending.get(&key) {
            return Ok(RequestStatus::AlreadyPending(*ticket));
        }
        if self.stopping.load(Ordering::Acquire) || self.job_sender.is_none() {
            return Err(RequestError::Stopped);
        }
        if self.outstanding >= self.capacity {
            return Err(RequestError::QueueFull);
        }
        let generation = self
            .next_generation
            .checked_add(1)
            .ok_or(RequestError::GenerationExhausted)?;
        let (edit_epoch, pending_snapshot) =
            world.begin_chunk_load(key).map_err(RequestError::World)?;
        let ticket = ChunkLoadTicket {
            key,
            generation,
            edit_epoch,
            requested_at: Instant::now(),
        };
        let job = Job {
            ticket,
            pending_snapshot,
        };
        let sender = self.job_sender.as_ref().expect("checked above");
        match sender.try_send(job) {
            Ok(()) => {
                self.next_generation = generation;
                self.outstanding += 1;
                self.pending.insert(key, ticket);
                Ok(RequestStatus::Enqueued(ticket))
            }
            Err(TrySendError::Full(job)) => {
                world.finish_chunk_load(key, edit_epoch);
                drop(job);
                Err(RequestError::QueueFull)
            }
            Err(TrySendError::Disconnected(job)) => {
                world.finish_chunk_load(key, edit_epoch);
                drop(job);
                Err(RequestError::Stopped)
            }
        }
    }

    #[cfg(test)]
    pub(super) fn is_pending(&self, key: ChunkKey) -> bool {
        self.pending.contains_key(&key)
    }

    /// Polls without waiting. Consuming a result releases both its outstanding
    /// capacity slot and its per-key deduplication entry.
    pub(super) fn try_recv(&mut self) -> Result<ChunkLoadResult, TryRecvError> {
        let result = self
            .result_receiver
            .as_ref()
            .expect("receiver remains present while loader is alive")
            .try_recv()?;
        self.outstanding -= 1;
        if self.pending.get(&result.ticket.key) == Some(&result.ticket) {
            self.pending.remove(&result.ticket.key);
        }
        Ok(result)
    }

    #[cfg(test)]
    pub(super) const fn capacity(&self) -> usize {
        self.capacity
    }

    pub(super) const fn outstanding(&self) -> usize {
        self.outstanding
    }
}

impl Drop for ChunkLoader {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        drop(self.result_receiver.take());
        drop(self.job_sender.take());
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn stop_workers(
    stopping: &AtomicBool,
    workers: &mut Vec<JoinHandle<()>>,
    job_sender: SyncSender<Job>,
    result_receiver: Receiver<ChunkLoadResult>,
) {
    stopping.store(true, Ordering::Release);
    drop(result_receiver);
    drop(job_sender);
    for worker in workers.drain(..) {
        let _ = worker.join();
    }
}

fn worker_loop(
    world: World,
    jobs: Arc<Mutex<Receiver<Job>>>,
    results: SyncSender<ChunkLoadResult>,
    stopping: Arc<AtomicBool>,
) {
    loop {
        if stopping.load(Ordering::Acquire) {
            break;
        }
        let job = {
            let receiver = match jobs.lock() {
                Ok(receiver) => receiver,
                Err(_) => break,
            };
            match receiver.recv() {
                Ok(job) => job,
                Err(_) => break,
            }
        };
        if stopping.load(Ordering::Acquire) {
            break;
        }
        let result = match job.pending_snapshot.as_deref() {
            Some(snapshot) => world.load_chunk_snapshot_uncached(job.ticket.key, snapshot),
            None => world.load_chunk_uncached(job.ticket.key),
        };
        if stopping.load(Ordering::Acquire)
            || results
                .send(ChunkLoadResult {
                    ticket: job.ticket,
                    result,
                })
                .is_err()
        {
            break;
        }
    }
}
