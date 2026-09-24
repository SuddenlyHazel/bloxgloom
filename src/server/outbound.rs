//! Exact, allocation-free accounting for bounded outbound replication queues.
//! A queued frame releases its counters even if a slow client's receiver is
//! dropped before the network writer can serialize it.

use crate::protocol::{self, ServerMessage};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError};

#[derive(Default)]
pub(super) struct OutboundTelemetry {
    queued_bytes: AtomicU64,
    queued_messages: AtomicU64,
    sent_bytes: AtomicU64,
    rejections: AtomicU64,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct OutboundSnapshot {
    pub queued_bytes: u64,
    pub queued_messages: u64,
    pub sent_bytes: u64,
    pub rejections: u64,
}

impl OutboundTelemetry {
    pub(super) fn try_send(
        self: &Arc<Self>,
        sender: &SyncSender<OutboundFrame>,
        message: ServerMessage,
    ) -> bool {
        let bytes = protocol::server_wire_len(&message) as u64;
        self.queued_bytes.fetch_add(bytes, Ordering::Relaxed);
        self.queued_messages.fetch_add(1, Ordering::Relaxed);
        let frame = OutboundFrame {
            message: Some(message),
            telemetry: Arc::clone(self),
            bytes,
            queued: true,
        };
        match sender.try_send(frame) {
            Ok(()) => true,
            Err(TrySendError::Full(_frame) | TrySendError::Disconnected(_frame)) => {
                // The rejected frame's Drop releases the reservation.
                self.rejections.fetch_add(1, Ordering::Relaxed);
                false
            }
        }
    }

    pub(super) fn snapshot(&self) -> OutboundSnapshot {
        OutboundSnapshot {
            queued_bytes: self.queued_bytes.load(Ordering::Relaxed),
            queued_messages: self.queued_messages.load(Ordering::Relaxed),
            sent_bytes: self.sent_bytes.load(Ordering::Relaxed),
            rejections: self.rejections.load(Ordering::Relaxed),
        }
    }
}

pub(super) struct OutboundFrame {
    message: Option<ServerMessage>,
    telemetry: Arc<OutboundTelemetry>,
    bytes: u64,
    queued: bool,
}

impl OutboundFrame {
    pub(super) fn message(&self) -> &ServerMessage {
        self.message
            .as_ref()
            .expect("outbound frame still owns message")
    }

    pub(super) fn dequeue(&mut self) {
        if self.queued {
            self.queued = false;
            self.telemetry
                .queued_bytes
                .fetch_sub(self.bytes, Ordering::Relaxed);
            self.telemetry
                .queued_messages
                .fetch_sub(1, Ordering::Relaxed);
        }
    }

    pub(super) fn record_sent(&self) {
        self.telemetry
            .sent_bytes
            .fetch_add(self.bytes, Ordering::Relaxed);
    }

    #[cfg(test)]
    pub(super) fn into_message(mut self) -> ServerMessage {
        self.dequeue();
        self.message
            .take()
            .expect("outbound frame still owns message")
    }
}

impl Drop for OutboundFrame {
    fn drop(&mut self) {
        self.dequeue();
    }
}

#[cfg(test)]
#[path = "outbound/tests.rs"]
mod tests;
