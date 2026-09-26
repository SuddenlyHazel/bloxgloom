//! Bounded asynchronous checkpoint writes.
//!
//! A caller submits full snapshots after a durable journal receipt. Key-sharded
//! workers write independent snapshots concurrently, preserve each key's FIFO
//! order, and report completion only after the supplied write closure returns.
//! The outstanding limit includes queued/running jobs and unconsumed receipts.

use super::journal::StateKey;
use std::hash::{Hash, Hasher};
use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::thread::{self, JoinHandle};

/// A completed checkpoint write. Errors stay attached to the snapshot revision
/// so the owner can retain its dirty overlay and retry it.
#[derive(Debug)]
pub(super) struct CheckpointReceipt {
    pub(super) key: StateKey,
    pub(super) revision: u64,
    pub(super) result: io::Result<()>,
}

/// Nonblocking checkpoint submission failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CheckpointSubmitError {
    /// All accepted work and unconsumed receipts currently occupy the bound.
    Full,
    /// Shutdown has started or the worker has exited.
    Closed,
}

type CheckpointWrite = Box<dyn FnOnce(&[u8]) -> io::Result<()> + Send + 'static>;

struct CheckpointJob {
    key: StateKey,
    revision: u64,
    snapshot: Vec<u8>,
    write: CheckpointWrite,
}

/// Key-sharded checkpoint workers with one global bound on accepted, unconsumed
/// work. `try_recv` releases one slot; callers should keep the newest snapshot
/// and clear it only when a successful receipt matches that snapshot's key and
/// revision.
///
/// Dropping or explicitly shutting down the writer closes submission, drains
/// accepted writes, and joins the workers. The receipt channel has the same
/// capacity as the total outstanding bound, so a worker can always publish all
/// accepted completions even when the caller has not polled receipts yet.
pub(super) struct CheckpointWriter {
    senders: Vec<SyncSender<CheckpointJob>>,
    receipts: Option<Receiver<CheckpointReceipt>>,
    outstanding: AtomicUsize,
    capacity: usize,
    workers: Vec<JoinHandle<()>>,
}

impl CheckpointWriter {
    /// Advisory capture credit; try_submit still owns the atomic reservation.
    pub(super) fn is_full(&self) -> bool {
        self.outstanding.load(Ordering::Acquire) >= self.capacity
    }

    /// One worker keeps small fixtures deterministic.
    #[cfg(test)]
    pub(super) fn new(capacity: usize) -> Self {
        Self::new_with_workers(capacity, 1)
    }

    /// A zero capacity or worker count is normalized to one so the writer can
    /// always make progress; no more workers than outstanding slots are useful.
    pub(super) fn new_with_workers(capacity: usize, workers: usize) -> Self {
        let capacity = capacity.max(1);
        let worker_count = workers.max(1).min(capacity);
        let (receipt_sender, receipt_receiver) = mpsc::sync_channel(capacity);
        let mut senders = Vec::with_capacity(worker_count);
        let mut handles = Vec::with_capacity(worker_count);
        for index in 0..worker_count {
            let (sender, receiver) = mpsc::sync_channel::<CheckpointJob>(capacity);
            let receipts = receipt_sender.clone();
            let handle = thread::Builder::new()
                .name(format!("server-checkpoint-{index}"))
                .spawn(move || checkpoint_worker(receiver, receipts))
                .expect("failed to start checkpoint worker");
            senders.push(sender);
            handles.push(handle);
        }
        drop(receipt_sender);

        Self {
            senders,
            receipts: Some(receipt_receiver),
            outstanding: AtomicUsize::new(0),
            capacity,
            workers: handles,
        }
    }

    /// Submit a full snapshot without waiting for queue space or disk I/O.
    /// The capacity reservation remains occupied until its receipt is consumed.
    pub(super) fn try_submit(
        &self,
        key: StateKey,
        revision: u64,
        snapshot: Vec<u8>,
        write: impl FnOnce(&[u8]) -> io::Result<()> + Send + 'static,
    ) -> Result<(), CheckpointSubmitError> {
        if self.senders.is_empty() {
            return Err(CheckpointSubmitError::Closed);
        }
        let shard = checkpoint_shard(&key, self.senders.len());
        let sender = &self.senders[shard];
        let reserved =
            self.outstanding
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                    (current < self.capacity).then_some(current + 1)
                });
        if reserved.is_err() {
            return Err(CheckpointSubmitError::Full);
        }

        let job = CheckpointJob {
            key,
            revision,
            snapshot,
            write: Box::new(write),
        };
        match sender.try_send(job) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => {
                self.release_slot();
                // With the outstanding bound, this branch indicates an
                // internal invariant violation: queued jobs alone would have
                // consumed every available reservation.
                Err(CheckpointSubmitError::Full)
            }
            Err(TrySendError::Disconnected(_)) => {
                self.release_slot();
                Err(CheckpointSubmitError::Closed)
            }
        }
    }

    /// Consume one completed write without waiting.
    pub(super) fn try_recv(&self) -> Result<CheckpointReceipt, TryRecvError> {
        let receipts = self.receipts.as_ref().ok_or(TryRecvError::Disconnected)?;
        match receipts.try_recv() {
            Ok(receipt) => {
                self.release_slot();
                Ok(receipt)
            }
            Err(error) => Err(error),
        }
    }

    /// Stop admission, drain accepted jobs, and join the worker. Completed
    /// receipts remain available through `try_recv` after this returns.
    pub(super) fn shutdown(&mut self) -> io::Result<()> {
        self.senders.clear();
        let mut first_error = None;
        for worker in self.workers.drain(..) {
            if let Err(payload) = worker.join() {
                first_error.get_or_insert_with(|| {
                    io::Error::other(format!(
                        "checkpoint worker panicked: {}",
                        panic_message(payload.as_ref())
                    ))
                });
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    fn release_slot(&self) {
        let previous = self.outstanding.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0, "checkpoint reservation underflow");
    }
}

fn checkpoint_shard(key: &StateKey, count: usize) -> usize {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    (hasher.finish() as usize) % count
}

impl Drop for CheckpointWriter {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn checkpoint_worker(jobs: Receiver<CheckpointJob>, receipts: SyncSender<CheckpointReceipt>) {
    for job in jobs {
        let CheckpointJob {
            key,
            revision,
            snapshot,
            write,
        } = job;
        let result = match catch_unwind(AssertUnwindSafe(|| write(&snapshot))) {
            Ok(result) => result,
            Err(payload) => Err(io::Error::other(format!(
                "checkpoint write panicked: {}",
                panic_message(payload.as_ref())
            ))),
        };
        // Capacity is reserved at admission for each request and released only
        // when its receipt is consumed. Therefore this bounded send cannot fill
        // before all accepted requests have produced their receipts.
        if receipts
            .send(CheckpointReceipt {
                key,
                revision,
                result,
            })
            .is_err()
        {
            // Receiver closure occurs only while the writer itself is being
            // dropped. Accepted checkpoint work has already completed.
            break;
        }
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> &str {
    payload
        .downcast_ref::<&'static str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("non-string panic payload")
}

#[cfg(test)]
#[path = "checkpoint/tests.rs"]
mod tests;
