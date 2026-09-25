//! Bounded nonblocking admission API for the journal worker.

use super::{CommitReceipt, Journal, MAX_QUEUE_CAPACITY, SubmitError, Transaction};
use std::collections::HashSet;
use std::io;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

mod worker;

/// Dedicated append/fsync worker with nonblocking bounded submissions.
pub struct JournalWriter {
    sender: Option<SyncSender<WriterCommand>>,
    worker: Option<JoinHandle<io::Result<()>>>,
    usage: Arc<AtomicU64>,
    // Includes every accepted request not yet resolved by the worker. A single
    // atomic reservation closes the gap between the advisory rotation target
    // and the hard WAL cap even with concurrent submitters.
    pub(super) projected_usage: Arc<AtomicU64>,
    sequence: Arc<AtomicU64>,
    rotation_pending: Arc<AtomicBool>,
    // Serializes only bounded channel submissions and the rotation control
    // command. No disk work or receipt wait happens while holding this lock.
    submit_gate: Mutex<()>,
}

/// Durable checkpoint boundary completed by an explicit rotation request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RotationReceipt {
    /// Global sequence that was placed in the immutable base generation.
    pub cut_sequence: u64,
    /// Generation selected by the durable manifest switch.
    pub generation: u64,
}

/// Nonblocking failure to enqueue a rotation control command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RotateError {
    Full,
    Closed,
}

impl std::fmt::Display for RotateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Full => formatter.write_str("journal request queue is full"),
            Self::Closed => formatter.write_str("journal writer is closed"),
        }
    }
}

impl std::error::Error for RotateError {}

impl JournalWriter {
    /// Queues a transaction without waiting. Poll or receive the returned
    /// channel away from the simulation's action path before applying/acking it.
    pub fn try_submit(
        &self,
        transaction: Transaction,
    ) -> Result<Receiver<io::Result<CommitReceipt>>, SubmitError> {
        let transaction = transaction.canonicalize().map_err(SubmitError::Invalid)?;
        let _gate = self.submit_gate.try_lock().map_err(|_| SubmitError::Full)?;
        if self.rotation_pending.load(Ordering::Acquire) {
            return Err(SubmitError::Full);
        }
        let reserved_bytes = super::codec::frame_len(&transaction);
        let mut projected = self.projected_usage.load(Ordering::Acquire);
        loop {
            let Some(next) = projected.checked_add(reserved_bytes) else {
                return Err(SubmitError::Full);
            };
            if next > super::MAX_JOURNAL_BYTES {
                return Err(SubmitError::Full);
            }
            match self.projected_usage.compare_exchange_weak(
                projected,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(current) => projected = current,
            }
        }
        let (acknowledge, receiver) = mpsc::channel();
        let request = WriterCommand::Append(Request {
            transaction,
            acknowledge,
            reserved_bytes,
        });
        let Some(sender) = self.sender.as_ref() else {
            self.projected_usage
                .fetch_sub(reserved_bytes, Ordering::AcqRel);
            return Err(SubmitError::Closed);
        };
        match sender.try_send(request) {
            Ok(()) => Ok(receiver),
            Err(TrySendError::Full(_)) => {
                self.projected_usage
                    .fetch_sub(reserved_bytes, Ordering::AcqRel);
                Err(SubmitError::Full)
            }
            Err(TrySendError::Disconnected(_)) => {
                self.projected_usage
                    .fetch_sub(reserved_bytes, Ordering::AcqRel);
                Err(SubmitError::Closed)
            }
        }
    }

    /// Atomically admits a bounded set of independently durable records to
    /// the writer queue. No prefix is accepted if queue or byte capacity is
    /// unavailable. Individual records retain their own IDs and receipts.
    pub fn try_submit_batch(
        &self,
        transactions: Vec<Transaction>,
    ) -> Result<Vec<Receiver<io::Result<CommitReceipt>>>, SubmitError> {
        if transactions.is_empty() || transactions.len() > super::MAX_BATCH_RECORDS {
            return Err(SubmitError::Invalid(io::Error::new(
                io::ErrorKind::InvalidInput,
                "journal submission batch size is invalid",
            )));
        }
        let mut prepared = Vec::with_capacity(transactions.len());
        let mut total_bytes = 0u64;
        let mut ids = HashSet::with_capacity(transactions.len());
        let mut keys = HashSet::new();
        for transaction in transactions {
            let transaction = transaction.canonicalize().map_err(SubmitError::Invalid)?;
            if !ids.insert(transaction.id)
                || transaction
                    .changes
                    .iter()
                    .any(|change| !keys.insert(change.key.clone()))
            {
                return Err(SubmitError::Invalid(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "journal batch IDs and write keys must be disjoint",
                )));
            }
            let bytes = super::codec::frame_len(&transaction);
            total_bytes = total_bytes.checked_add(bytes).ok_or_else(|| {
                SubmitError::Invalid(io::Error::other("journal batch byte count overflow"))
            })?;
            prepared.push((transaction, bytes));
        }
        let _gate = self.submit_gate.try_lock().map_err(|_| SubmitError::Full)?;
        if self.rotation_pending.load(Ordering::Acquire) {
            return Err(SubmitError::Full);
        }
        let mut projected = self.projected_usage.load(Ordering::Acquire);
        loop {
            let Some(next) = projected.checked_add(total_bytes) else {
                return Err(SubmitError::Full);
            };
            if next > super::MAX_JOURNAL_BYTES {
                return Err(SubmitError::Full);
            }
            match self.projected_usage.compare_exchange_weak(
                projected,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(current) => projected = current,
            }
        }
        let mut requests = Vec::with_capacity(prepared.len());
        let mut receivers = Vec::with_capacity(prepared.len());
        for (transaction, reserved_bytes) in prepared {
            let (acknowledge, receiver) = mpsc::channel();
            requests.push(Request {
                transaction,
                acknowledge,
                reserved_bytes,
            });
            receivers.push(receiver);
        }
        let Some(sender) = self.sender.as_ref() else {
            self.projected_usage
                .fetch_sub(total_bytes, Ordering::AcqRel);
            return Err(SubmitError::Closed);
        };
        match sender.try_send(WriterCommand::AppendBatch(requests)) {
            Ok(()) => Ok(receivers),
            Err(TrySendError::Full(_)) => {
                self.projected_usage
                    .fetch_sub(total_bytes, Ordering::AcqRel);
                Err(SubmitError::Full)
            }
            Err(TrySendError::Disconnected(_)) => {
                self.projected_usage
                    .fetch_sub(total_bytes, Ordering::AcqRel);
                Err(SubmitError::Closed)
            }
        }
    }

    /// Current append-only file size, updated by the worker after each batch.
    pub fn bytes(&self) -> u64 {
        self.usage.load(Ordering::Acquire)
    }

    /// Current globally synced physical record sequence. Queued, unsynced
    /// submissions are deliberately not reflected here.
    pub fn sequence(&self) -> u64 {
        self.sequence.load(Ordering::Acquire)
    }

    /// Whether the active WAL tail has reached the checkpoint rotation target.
    /// This is advisory; append requests still fail closed at the hard cap.
    pub fn needs_rotation(&self) -> bool {
        self.projected_usage.load(Ordering::Acquire) >= super::JOURNAL_ROTATION_SOFT_LIMIT_BYTES
    }

    /// Queues an explicit generation switch after commands already submitted
    /// to this writer. The caller must freeze durable admissions, drain/apply
    /// commit receipts through `expected_sequence`, then drain successful
    /// BGED/BGIN checkpoint receipts for that exact state before calling.
    /// Checkpointing and this request must stay off the tick's blocking path.
    /// A sequence mismatch is reported through the returned receiver.
    pub(crate) fn try_rotate(
        &self,
        expected_sequence: u64,
    ) -> Result<Receiver<io::Result<RotationReceipt>>, RotateError> {
        self.try_rotate_inner(expected_sequence)
    }

    fn try_rotate_inner(
        &self,
        expected_sequence: u64,
    ) -> Result<Receiver<io::Result<RotationReceipt>>, RotateError> {
        let _gate = self.submit_gate.try_lock().map_err(|_| RotateError::Full)?;
        if self
            .rotation_pending
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(RotateError::Full);
        }
        let (acknowledge, receiver) = mpsc::channel();
        let command = WriterCommand::Rotate {
            expected_sequence,
            acknowledge,
        };
        let Some(sender) = self.sender.as_ref() else {
            self.rotation_pending.store(false, Ordering::Release);
            return Err(RotateError::Closed);
        };
        match sender.try_send(command) {
            Ok(()) => Ok(receiver),
            Err(TrySendError::Full(_)) => {
                self.rotation_pending.store(false, Ordering::Release);
                Err(RotateError::Full)
            }
            Err(TrySendError::Disconnected(_)) => {
                self.rotation_pending.store(false, Ordering::Release);
                Err(RotateError::Closed)
            }
        }
    }

    /// Closes the request queue, drains submitted work, and joins the writer.
    #[cfg(test)]
    pub fn shutdown(mut self) -> io::Result<()> {
        self.sender.take();
        self.join_worker()
    }

    fn join_worker(&mut self) -> io::Result<()> {
        let Some(worker) = self.worker.take() else {
            return Ok(());
        };
        match worker.join() {
            Ok(result) => result,
            Err(_) => Err(io::Error::other("journal worker thread panicked")),
        }
    }
}

impl Drop for JournalWriter {
    fn drop(&mut self) {
        self.sender.take();
        let _ = self.join_worker();
    }
}

pub(super) struct Request {
    pub(super) transaction: Transaction,
    pub(super) acknowledge: mpsc::Sender<io::Result<CommitReceipt>>,
    pub(super) reserved_bytes: u64,
}

enum WriterCommand {
    Append(Request),
    AppendBatch(Vec<Request>),
    Rotate {
        expected_sequence: u64,
        acknowledge: mpsc::Sender<io::Result<RotationReceipt>>,
    },
}

impl Journal {
    /// Starts a bounded writer thread. Submissions never wait for disk I/O;
    /// acknowledgments arrive only after `sync_all` succeeds.
    pub fn into_writer(
        mut self,
        queue_capacity: usize,
        batch_delay: Duration,
    ) -> io::Result<JournalWriter> {
        // Snapshot validation is complete before this handoff. The base
        // anchor can be as large as the materialized world; retaining its
        // duplicate values in the live writer would waste that much memory.
        self.base_anchor = std::collections::HashMap::new();
        self.history = std::collections::HashMap::new();
        let (sender, receiver) = mpsc::sync_channel(queue_capacity.clamp(1, MAX_QUEUE_CAPACITY));
        let usage = Arc::new(AtomicU64::new(self.log_bytes));
        let projected_usage = Arc::new(AtomicU64::new(self.log_bytes));
        let sequence = Arc::new(AtomicU64::new(self.physical_records));
        let rotation_pending = Arc::new(AtomicBool::new(false));
        let worker_usage = Arc::clone(&usage);
        let worker_projected_usage = Arc::clone(&projected_usage);
        let worker_sequence = Arc::clone(&sequence);
        let worker_rotation_pending = Arc::clone(&rotation_pending);
        let worker = thread::Builder::new()
            .name("bloxgloom-journal".into())
            .spawn(move || {
                worker::writer_loop(
                    self,
                    receiver,
                    batch_delay,
                    worker_usage,
                    worker_projected_usage,
                    worker_sequence,
                    worker_rotation_pending,
                )
            })?;
        Ok(JournalWriter {
            sender: Some(sender),
            worker: Some(worker),
            usage,
            projected_usage,
            sequence,
            rotation_pending,
            submit_gate: Mutex::new(()),
        })
    }
}
