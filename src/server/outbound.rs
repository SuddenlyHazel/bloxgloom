//! Bounded admission for ordered per-client replication queues.
//!
//! Admission accounts for both encoded bytes and frame count. A reservation is
//! held until its frame is written or dropped, so the limit includes a frame
//! currently being written as well as frames still in the channel.

use crate::protocol::{self, ServerMessage};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::time::{Duration, Instant};

pub(super) const OUTBOUND_FRAME_CAPACITY: usize = 128;
pub(super) const OUTBOUND_CLIENT_BYTE_CAPACITY: u64 = 2 * 1024 * 1024;
pub(super) const OUTBOUND_AGGREGATE_BYTE_CAPACITY: u64 = 128 * 1024 * 1024;

pub(super) struct OutboundTelemetry {
    queued_bytes: AtomicU64,
    queued_messages: AtomicU64,
    sent_bytes: AtomicU64,
    rejections: AtomicU64,
    max_queued_bytes: AtomicU64,
    max_client_queued_bytes: AtomicU64,
    aggregate_byte_limit: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct OutboundSnapshot {
    pub queued_bytes: u64,
    pub queued_messages: u64,
    pub sent_bytes: u64,
    pub rejections: u64,
    pub max_queued_bytes: u64,
    pub max_client_queued_bytes: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct OutboundClientSnapshot {
    pub queued_bytes: u64,
    pub queued_frames: usize,
}

impl Default for OutboundTelemetry {
    fn default() -> Self {
        Self::with_aggregate_byte_limit(OUTBOUND_AGGREGATE_BYTE_CAPACITY)
    }
}

impl OutboundTelemetry {
    fn with_aggregate_byte_limit(aggregate_byte_limit: u64) -> Self {
        Self {
            queued_bytes: AtomicU64::new(0),
            queued_messages: AtomicU64::new(0),
            sent_bytes: AtomicU64::new(0),
            rejections: AtomicU64::new(0),
            max_queued_bytes: AtomicU64::new(0),
            max_client_queued_bytes: AtomicU64::new(0),
            aggregate_byte_limit,
        }
    }

    /// Creates an outbound queue whose byte reservations contribute to this
    /// server-wide telemetry and aggregate byte budget.
    pub(super) fn client_queue(self: &Arc<Self>) -> (OutboundQueue, Receiver<OutboundFrame>) {
        self.make_client_queue(OUTBOUND_FRAME_CAPACITY, OUTBOUND_CLIENT_BYTE_CAPACITY)
    }

    #[cfg(test)]
    pub(super) fn client_queue_with_limits(
        self: &Arc<Self>,
        frame_capacity: usize,
        byte_capacity: u64,
    ) -> (OutboundQueue, Receiver<OutboundFrame>) {
        self.make_client_queue(frame_capacity, byte_capacity)
    }

    fn make_client_queue(
        self: &Arc<Self>,
        frame_capacity: usize,
        byte_capacity: u64,
    ) -> (OutboundQueue, Receiver<OutboundFrame>) {
        let (sender, receiver) = mpsc::sync_channel(frame_capacity);
        let queue = OutboundQueue {
            sender,
            telemetry: Arc::clone(self),
            client: Arc::new(ClientQueueTelemetry::default()),
            frame_capacity,
            byte_capacity,
        };
        (queue, receiver)
    }

    pub(super) fn snapshot(&self) -> OutboundSnapshot {
        OutboundSnapshot {
            queued_bytes: self.queued_bytes.load(Ordering::Relaxed),
            queued_messages: self.queued_messages.load(Ordering::Relaxed),
            sent_bytes: self.sent_bytes.load(Ordering::Relaxed),
            rejections: self.rejections.load(Ordering::Relaxed),
            max_queued_bytes: self.max_queued_bytes.load(Ordering::Relaxed),
            max_client_queued_bytes: self.max_client_queued_bytes.load(Ordering::Relaxed),
        }
    }
}

#[derive(Default)]
struct ClientQueueTelemetry {
    queued_bytes: AtomicU64,
    queued_messages: AtomicUsize,
}

/// Cloneable handle for one client's ordered outbound queue.
///
/// All clones share the same per-client limits. Rejections are explicit and
/// callers must disconnect or otherwise recover the client; reliable messages
/// are never silently discarded.
#[derive(Clone)]
pub(super) struct OutboundQueue {
    sender: SyncSender<OutboundFrame>,
    telemetry: Arc<OutboundTelemetry>,
    client: Arc<ClientQueueTelemetry>,
    frame_capacity: usize,
    byte_capacity: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum OutboundError {
    FrameLimit,
    ClientByteLimit,
    AggregateByteLimit,
    Disconnected,
    TooLarge,
}

impl OutboundQueue {
    pub(super) fn try_send(&self, message: ServerMessage) -> Result<(), OutboundError> {
        let bytes = match u64::try_from(protocol::server_wire_len(&message)) {
            Ok(bytes) => bytes,
            Err(_) => return self.reject(OutboundError::TooLarge),
        };
        let maximum_wire_frame = protocol::MAX_FRAME.saturating_add(4) as u64;
        if bytes > self.byte_capacity || bytes > maximum_wire_frame {
            return self.reject(OutboundError::TooLarge);
        }
        if !reserve_count(&self.client.queued_messages, self.frame_capacity) {
            return self.reject(OutboundError::FrameLimit);
        }
        if !reserve(&self.client.queued_bytes, bytes, self.byte_capacity) {
            self.client.queued_messages.fetch_sub(1, Ordering::Relaxed);
            return self.reject(OutboundError::ClientByteLimit);
        }
        if !reserve(
            &self.telemetry.queued_bytes,
            bytes,
            self.telemetry.aggregate_byte_limit,
        ) {
            self.client.queued_bytes.fetch_sub(bytes, Ordering::Relaxed);
            self.client.queued_messages.fetch_sub(1, Ordering::Relaxed);
            return self.reject(OutboundError::AggregateByteLimit);
        }
        self.telemetry
            .queued_messages
            .fetch_add(1, Ordering::Relaxed);
        self.telemetry.max_queued_bytes.fetch_max(
            self.telemetry.queued_bytes.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        self.telemetry.max_client_queued_bytes.fetch_max(
            self.client.queued_bytes.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );

        let frame = OutboundFrame {
            message: Some(message),
            telemetry: Arc::clone(&self.telemetry),
            client: Some(Arc::clone(&self.client)),
            bytes,
            reserved: true,
            queued_at: Instant::now(),
        };
        match self.sender.try_send(frame) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_frame)) => self.reject(OutboundError::FrameLimit),
            Err(TrySendError::Disconnected(_frame)) => self.reject(OutboundError::Disconnected),
        }
    }

    pub(super) fn snapshot(&self) -> OutboundClientSnapshot {
        OutboundClientSnapshot {
            queued_bytes: self.client.queued_bytes.load(Ordering::Relaxed),
            queued_frames: self.client.queued_messages.load(Ordering::Relaxed),
        }
    }

    fn reject<T>(&self, error: OutboundError) -> Result<T, OutboundError> {
        self.telemetry.rejections.fetch_add(1, Ordering::Relaxed);
        Err(error)
    }
}

/// A queued message owns all admission reservations until it is fully sent or
/// dropped. This also covers receiver shutdown and failed socket writes.
pub(super) struct OutboundFrame {
    message: Option<ServerMessage>,
    telemetry: Arc<OutboundTelemetry>,
    client: Option<Arc<ClientQueueTelemetry>>,
    bytes: u64,
    reserved: bool,
    queued_at: Instant,
}

impl OutboundFrame {
    pub(super) fn message(&self) -> &ServerMessage {
        self.message
            .as_ref()
            .expect("outbound frame still owns message")
    }

    pub(super) fn record_sent(&self) {
        self.telemetry
            .sent_bytes
            .fetch_add(self.bytes, Ordering::Relaxed);
    }

    pub(super) fn age(&self) -> Duration {
        self.queued_at.elapsed()
    }

    #[cfg(test)]
    pub(super) fn into_message(mut self) -> ServerMessage {
        self.message
            .take()
            .expect("outbound frame still owns message")
    }
}

impl Drop for OutboundFrame {
    fn drop(&mut self) {
        if !self.reserved {
            return;
        }
        self.reserved = false;
        self.telemetry
            .queued_bytes
            .fetch_sub(self.bytes, Ordering::Relaxed);
        self.telemetry
            .queued_messages
            .fetch_sub(1, Ordering::Relaxed);
        if let Some(client) = &self.client {
            client.queued_bytes.fetch_sub(self.bytes, Ordering::Relaxed);
            client.queued_messages.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

fn reserve(counter: &AtomicU64, amount: u64, limit: u64) -> bool {
    let mut current = counter.load(Ordering::Acquire);
    loop {
        let Some(next) = current.checked_add(amount) else {
            return false;
        };
        if next > limit {
            return false;
        }
        match counter.compare_exchange_weak(current, next, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return true,
            Err(observed) => current = observed,
        }
    }
}

fn reserve_count(counter: &AtomicUsize, limit: usize) -> bool {
    let mut current = counter.load(Ordering::Acquire);
    loop {
        let Some(next) = current.checked_add(1) else {
            return false;
        };
        if next > limit {
            return false;
        }
        match counter.compare_exchange_weak(current, next, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return true,
            Err(observed) => current = observed,
        }
    }
}

#[cfg(test)]
#[path = "outbound/tests.rs"]
mod tests;
