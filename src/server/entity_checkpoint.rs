//! Bounded off-tick mirror for entity aggregate checkpoints.
//!
//! The live store remains authoritative. Callers reserve mirror capacity
//! before accepting a durable entity transaction,
//! then submit the corresponding event only after the live state changes.
//! A FIFO fence writes BGEN from an independently decoded worker-owned store.

#[cfg(test)]
use super::entities::EntityMotionSnapshot;
use super::entities::{EntityCheckpointStore, EntityStore, PreparedEntityBatch};
use std::io;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

mod worker;

#[cfg(test)]
mod tests;

const MAX_MIRROR_ADMISSIONS: usize = 256;

struct CheckpointWork {
    entries: usize,
    #[cfg(test)]
    first_turn: Option<(SyncSender<usize>, Receiver<()>)>,
}

impl Default for CheckpointWork {
    fn default() -> Self {
        Self {
            entries: super::checkpoint_stream::TURN_ENTRIES,
            #[cfg(test)]
            first_turn: None,
        }
    }
}

impl CheckpointWork {
    fn after_turn(&mut self, _count: usize) -> io::Result<()> {
        #[cfg(test)]
        if let Some((reached, resume)) = self.first_turn.take() {
            reached
                .send(_count)
                .map_err(|_| io::Error::other("checkpoint test observer closed"))?;
            resume
                .recv_timeout(std::time::Duration::from_secs(5))
                .map_err(|_| io::Error::other("checkpoint test resume missing"))?;
        }
        std::thread::yield_now();
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PermitKind {
    Durable,
    // Legacy checkpoint-only motion is retained solely as a mirror fence fixture.
    // Live player/drop motion now travels in receipted durable batches.
    #[cfg(test)]
    Motion,
}

/// A non-Clone reservation. Dropping it cancels admission without losing a
/// committed event; the caller must retain it across a pending WAL receipt.
pub(in crate::server) struct MirrorPermit {
    shared: Arc<Shared>,
    kind: PermitKind,
    active: bool,
    must_submit: bool,
}

impl Drop for MirrorPermit {
    fn drop(&mut self) {
        if self.active {
            if self.must_submit {
                self.shared
                    .fail("authoritative entity change lost its checkpoint mirror event");
            }
            self.shared.reserved.fetch_sub(1, Ordering::AcqRel);
            self.shared.outstanding.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

impl MirrorPermit {
    /// Arm only after a durable WAL submission is accepted, or after a
    /// checkpoint-only motion has changed the live store. An armed permit
    /// cannot disappear silently; submission or server restart is required.
    pub(in crate::server) fn mark_authoritative_change(&mut self) -> io::Result<()> {
        if !self.active || self.must_submit {
            self.shared.fail("invalid entity mirror permit activation");
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid entity mirror permit activation",
            ));
        }
        self.must_submit = true;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::server) struct CheckpointReceipt {
    pub event_sequence: u64,
    pub durable_sequence: u64,
    pub registry_revision: u64,
}

/// Must be finished explicitly after the journal rotation decision. Dropping
/// a pending/finished ticket does not silently reopen entity admissions.
pub(in crate::server) struct CheckpointTicket {
    fence_id: u64,
    receiver: Receiver<io::Result<CheckpointReceipt>>,
    receipt: Option<CheckpointReceipt>,
}

#[cfg(test)]
impl CheckpointTicket {
    /// Synchronize a fixture with the real worker without finishing its fence.
    /// The next production poll still validates the ticket and receipt frontier.
    pub(in crate::server) fn wait_for_worker(
        &mut self,
        timeout: std::time::Duration,
    ) -> io::Result<()> {
        if self.receipt.is_none() {
            self.receipt = Some(self.receiver.recv_timeout(timeout).map_err(|error| {
                io::Error::other(format!("entity checkpoint completion unavailable: {error}"))
            })??);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::server) struct MirrorMetrics {
    pub capacity: usize,
    pub outstanding: usize,
    pub reserved: usize,
    pub high_water: usize,
    pub submitted_sequence: u64,
    pub applied_sequence: u64,
    pub checkpoint_sequence: u64,
    pub fenced: bool,
    pub failed: bool,
}

struct Shared {
    outstanding: AtomicUsize,
    reserved: AtomicUsize,
    high_water: AtomicUsize,
    applied_sequence: AtomicU64,
    checkpoint_sequence: AtomicU64,
    failed: AtomicBool,
    failure: Mutex<Option<String>>,
}

impl Shared {
    fn new() -> Self {
        Self {
            outstanding: AtomicUsize::new(0),
            reserved: AtomicUsize::new(0),
            high_water: AtomicUsize::new(0),
            applied_sequence: AtomicU64::new(0),
            checkpoint_sequence: AtomicU64::new(0),
            failed: AtomicBool::new(false),
            failure: Mutex::new(None),
        }
    }

    fn fail(&self, reason: impl Into<String>) {
        if let Ok(mut failure) = self.failure.lock() {
            if failure.is_none() {
                *failure = Some(reason.into());
            }
        }
        self.failed.store(true, Ordering::Release);
    }

    fn health(&self) -> io::Result<()> {
        if !self.failed.load(Ordering::Acquire) {
            return Ok(());
        }
        let reason = self
            .failure
            .lock()
            .ok()
            .and_then(|failure| failure.clone())
            .unwrap_or_else(|| "entity checkpoint mirror worker failed".to_owned());
        Err(io::Error::other(format!(
            "entity checkpoint mirror requires restart: {reason}"
        )))
    }

    fn try_admit(&self, capacity: usize) -> bool {
        let admitted =
            self.outstanding
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                    (current < capacity).then_some(current + 1)
                });
        if let Ok(previous) = admitted {
            self.high_water.fetch_max(previous + 1, Ordering::AcqRel);
            true
        } else {
            false
        }
    }
}

enum Command {
    Event {
        sequence: u64,
        event: Event,
    },
    Checkpoint {
        required_sequence: u64,
        reply: mpsc::Sender<io::Result<CheckpointReceipt>>,
    },
}

enum Event {
    Durable(PreparedEntityBatch),
    #[cfg(test)]
    Motion(EntityMotionSnapshot),
}

/// Coordinator-side handle. All methods are nonblocking except `Drop` during
/// shutdown. The worker never borrows the live store or its world lock.
pub(in crate::server) struct EntityCheckpointMirror {
    sender: Option<SyncSender<Command>>,
    worker: Option<JoinHandle<()>>,
    shared: Arc<Shared>,
    capacity: usize,
    submitted_sequence: u64,
    next_fence_id: u64,
    active_fence: Option<u64>,
}

impl EntityCheckpointMirror {
    pub(in crate::server) fn start(
        baseline: EntityStore,
        checkpoint_store: EntityCheckpointStore,
        capacity: usize,
    ) -> io::Result<Self> {
        Self::start_with_work(
            baseline,
            checkpoint_store,
            capacity,
            CheckpointWork::default(),
        )
    }

    fn start_with_work(
        baseline: EntityStore,
        checkpoint_store: EntityCheckpointStore,
        capacity: usize,
        work: CheckpointWork,
    ) -> io::Result<Self> {
        if !(1..=MAX_MIRROR_ADMISSIONS).contains(&capacity) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "entity checkpoint mirror capacity must be 1..=256",
            ));
        }
        let (sender, receiver) = mpsc::sync_channel(capacity);
        let shared = Arc::new(Shared::new());
        let worker_shared = Arc::clone(&shared);
        let worker = thread::Builder::new()
            .name("entity-checkpoint".to_owned())
            .spawn(move || {
                worker::run(receiver, worker_shared, baseline, checkpoint_store, work)
            })?;
        Ok(Self {
            sender: Some(sender),
            worker: Some(worker),
            shared,
            capacity,
            submitted_sequence: 0,
            next_fence_id: 0,
            active_fence: None,
        })
    }

    pub(in crate::server) fn try_reserve_durable(&mut self) -> io::Result<Option<MirrorPermit>> {
        self.try_reserve(PermitKind::Durable)
    }

    #[cfg(test)]
    pub(in crate::server) fn try_reserve_motion(&mut self) -> io::Result<Option<MirrorPermit>> {
        self.try_reserve(PermitKind::Motion)
    }

    fn try_reserve(&mut self, kind: PermitKind) -> io::Result<Option<MirrorPermit>> {
        self.check_health()?;
        if self.active_fence.is_some() || !self.shared.try_admit(self.capacity) {
            return Ok(None);
        }
        self.shared.reserved.fetch_add(1, Ordering::AcqRel);
        Ok(Some(MirrorPermit {
            shared: Arc::clone(&self.shared),
            kind,
            active: true,
            must_submit: false,
        }))
    }

    pub(in crate::server) fn submit_durable(
        &mut self,
        permit: MirrorPermit,
        batch: PreparedEntityBatch,
    ) -> io::Result<u64> {
        self.submit(permit, PermitKind::Durable, Event::Durable(batch))
    }

    #[cfg(test)]
    pub(in crate::server) fn submit_motion(
        &mut self,
        permit: MirrorPermit,
        snapshot: EntityMotionSnapshot,
    ) -> io::Result<u64> {
        self.submit(permit, PermitKind::Motion, Event::Motion(snapshot))
    }

    fn submit(
        &mut self,
        mut permit: MirrorPermit,
        kind: PermitKind,
        event: Event,
    ) -> io::Result<u64> {
        self.check_health()?;
        if !Arc::ptr_eq(&permit.shared, &self.shared)
            || permit.kind != kind
            || !permit.active
            || !permit.must_submit
            || self.active_fence.is_some()
        {
            self.shared
                .fail("invalid or fenced entity mirror event submission");
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid or fenced entity mirror event submission",
            ));
        }
        let sequence = self
            .submitted_sequence
            .checked_add(1)
            .ok_or_else(|| io::Error::other("entity mirror event sequence exhausted"))?;
        let command = Command::Event { sequence, event };
        if let Err(error) = self.sender()?.try_send(command) {
            self.shared.fail(format!(
                "admitted entity mirror event could not reach worker: {error}"
            ));
            return Err(io::Error::other(
                "admitted entity mirror event could not reach worker",
            ));
        }
        self.submitted_sequence = sequence;
        permit.active = false;
        self.shared.reserved.fetch_sub(1, Ordering::AcqRel);
        Ok(sequence)
    }

    /// Closes new admissions and queues a FIFO checkpoint behind every
    /// submitted event. An unresolved pre-WAL permit or full queue defers this
    /// attempt; neither is silently skipped or coalesced.
    pub(in crate::server) fn try_begin_checkpoint(
        &mut self,
    ) -> io::Result<Option<CheckpointTicket>> {
        self.check_health()?;
        if self.active_fence.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "entity checkpoint fence already active",
            ));
        }
        if self.shared.reserved.load(Ordering::Acquire) != 0
            || !self.shared.try_admit(self.capacity)
        {
            return Ok(None);
        }
        let fence_id = match self.next_fence_id.checked_add(1) {
            Some(id) => id,
            None => {
                self.shared.outstanding.fetch_sub(1, Ordering::AcqRel);
                return Err(io::Error::other("entity checkpoint fence ID exhausted"));
            }
        };
        let (reply, receiver) = mpsc::channel();
        let command = Command::Checkpoint {
            required_sequence: self.submitted_sequence,
            reply,
        };
        let sent = self.sender()?.try_send(command);
        if let Err(error) = sent {
            self.shared.outstanding.fetch_sub(1, Ordering::AcqRel);
            self.shared.fail(format!(
                "admitted entity checkpoint fence could not reach worker: {error}"
            ));
            return Err(io::Error::other(
                "admitted entity checkpoint fence could not reach worker",
            ));
        }
        self.next_fence_id = fence_id;
        self.active_fence = Some(fence_id);
        Ok(Some(CheckpointTicket {
            fence_id,
            receiver,
            receipt: None,
        }))
    }

    pub(in crate::server) fn poll_checkpoint(
        &self,
        ticket: &mut CheckpointTicket,
    ) -> io::Result<Option<CheckpointReceipt>> {
        self.check_health()?;
        if self.active_fence != Some(ticket.fence_id) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "entity checkpoint ticket does not match active fence",
            ));
        }
        if let Some(receipt) = ticket.receipt {
            return Ok(Some(receipt));
        }
        match ticket.receiver.try_recv() {
            Ok(Ok(receipt)) => {
                ticket.receipt = Some(receipt);
                Ok(Some(receipt))
            }
            Ok(Err(error)) => Err(error),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(io::Error::other(
                "entity checkpoint worker reply disconnected",
            )),
        }
    }

    pub(in crate::server) fn finish_checkpoint_fence(
        &mut self,
        ticket: CheckpointTicket,
    ) -> io::Result<CheckpointReceipt> {
        self.check_health()?;
        if self.active_fence != Some(ticket.fence_id) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "entity checkpoint ticket does not match active fence",
            ));
        }
        let receipt = ticket.receipt.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::WouldBlock,
                "entity checkpoint fence is not complete",
            )
        })?;
        self.active_fence = None;
        Ok(receipt)
    }

    pub(in crate::server) fn check_health(&self) -> io::Result<()> {
        self.shared.health()
    }

    pub(in crate::server) fn metrics(&self) -> MirrorMetrics {
        MirrorMetrics {
            capacity: self.capacity,
            outstanding: self.shared.outstanding.load(Ordering::Acquire),
            reserved: self.shared.reserved.load(Ordering::Acquire),
            high_water: self.shared.high_water.load(Ordering::Acquire),
            submitted_sequence: self.submitted_sequence,
            applied_sequence: self.shared.applied_sequence.load(Ordering::Acquire),
            checkpoint_sequence: self.shared.checkpoint_sequence.load(Ordering::Acquire),
            fenced: self.active_fence.is_some(),
            failed: self.shared.failed.load(Ordering::Acquire),
        }
    }

    fn sender(&self) -> io::Result<&SyncSender<Command>> {
        self.sender
            .as_ref()
            .ok_or_else(|| io::Error::other("entity checkpoint mirror sender closed"))
    }
}

impl Drop for EntityCheckpointMirror {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
