//! Lock-free transport counters shared with the real TCP soak harness.

use std::io::{self, ErrorKind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

const AGE_BUCKETS: usize = 16;

#[cfg(test)]
#[path = "metrics/tests.rs"]
mod tests;

#[derive(Default)]
pub(in crate::server) struct TransportStats {
    inbound_bytes: AtomicU64,
    outbound_bytes: AtomicU64,
    accepted: AtomicU64,
    active: AtomicU64,
    max_active: AtomicU64,
    admission_rejected: AtomicU64,
    peer_eof: AtomicU64,
    malformed: AtomicU64,
    timed_out: AtomicU64,
    backpressure: AtomicU64,
    socket_error: AtomicU64,
    decode_queued: AtomicU64,
    encode_queued: AtomicU64,
    max_decode_queued: AtomicU64,
    max_encode_queued: AtomicU64,
    decoded_frames: AtomicU64,
    encoded_frames: AtomicU64,
    decode_busy_ns: AtomicU64,
    encode_busy_ns: AtomicU64,
    send_age_ms_max: AtomicU64,
    send_age_buckets: [AtomicU64; AGE_BUCKETS],
    reactor_busy_ns: AtomicU64,
    reactor_passes: AtomicU64,
}

#[derive(Clone, Copy, Debug, Default)]
pub(in crate::server) struct TransportSnapshot {
    pub inbound_bytes: u64,
    pub outbound_bytes: u64,
    pub accepted: u64,
    pub active: u64,
    pub max_active: u64,
    pub admission_rejected: u64,
    pub peer_eof: u64,
    pub malformed: u64,
    pub timed_out: u64,
    pub backpressure: u64,
    pub socket_error: u64,
    pub decode_queued: u64,
    pub encode_queued: u64,
    pub max_decode_queued: u64,
    pub max_encode_queued: u64,
    pub decoded_frames: u64,
    pub encoded_frames: u64,
    pub decode_busy_ns: u64,
    pub encode_busy_ns: u64,
    pub send_age_ms_p95_upper: u64,
    pub send_age_ms_max: u64,
    pub reactor_busy_ns: u64,
    pub reactor_passes: u64,
}

impl TransportStats {
    pub(in crate::server) fn snapshot(&self) -> TransportSnapshot {
        let load = |counter: &AtomicU64| counter.load(Ordering::Relaxed);
        let completed = self.send_age_buckets.iter().map(load).sum::<u64>();
        let target = completed.saturating_mul(95).div_ceil(100);
        let mut running = 0u64;
        let mut p95_upper = 0u64;
        for (index, bucket) in self.send_age_buckets.iter().enumerate() {
            running += load(bucket);
            if target != 0 && running >= target {
                p95_upper = 1u64 << index;
                break;
            }
        }
        TransportSnapshot {
            inbound_bytes: load(&self.inbound_bytes),
            outbound_bytes: load(&self.outbound_bytes),
            accepted: load(&self.accepted),
            active: load(&self.active),
            max_active: load(&self.max_active),
            admission_rejected: load(&self.admission_rejected),
            peer_eof: load(&self.peer_eof),
            malformed: load(&self.malformed),
            timed_out: load(&self.timed_out),
            backpressure: load(&self.backpressure),
            socket_error: load(&self.socket_error),
            decode_queued: load(&self.decode_queued),
            encode_queued: load(&self.encode_queued),
            max_decode_queued: load(&self.max_decode_queued),
            max_encode_queued: load(&self.max_encode_queued),
            decoded_frames: load(&self.decoded_frames),
            encoded_frames: load(&self.encoded_frames),
            decode_busy_ns: load(&self.decode_busy_ns),
            encode_busy_ns: load(&self.encode_busy_ns),
            send_age_ms_p95_upper: p95_upper,
            send_age_ms_max: load(&self.send_age_ms_max),
            reactor_busy_ns: load(&self.reactor_busy_ns),
            reactor_passes: load(&self.reactor_passes),
        }
    }

    pub(super) fn accepted(&self) {
        self.accepted.fetch_add(1, Ordering::Relaxed);
        let active = self.active.fetch_add(1, Ordering::Relaxed) + 1;
        self.max_active.fetch_max(active, Ordering::Relaxed);
    }

    pub(super) fn removed(&self) {
        self.active.fetch_sub(1, Ordering::Relaxed);
    }

    pub(super) fn admission_rejected(&self) {
        self.admission_rejected.fetch_add(1, Ordering::Relaxed);
    }

    pub(super) fn reactor_pass(&self, busy: Duration) {
        self.reactor_busy_ns.fetch_add(
            busy.as_nanos().min(u64::MAX as u128) as u64,
            Ordering::Relaxed,
        );
        self.reactor_passes.fetch_add(1, Ordering::Relaxed);
    }

    pub(super) fn input(&self, bytes: usize) {
        self.inbound_bytes
            .fetch_add(bytes as u64, Ordering::Relaxed);
    }

    pub(super) fn output(&self, bytes: usize) {
        self.outbound_bytes
            .fetch_add(bytes as u64, Ordering::Relaxed);
    }

    pub(super) fn disconnect(&self, error: &io::Error) {
        let counter = match error.kind() {
            ErrorKind::UnexpectedEof => &self.peer_eof,
            ErrorKind::InvalidData => &self.malformed,
            ErrorKind::TimedOut => &self.timed_out,
            ErrorKind::WouldBlock => &self.backpressure,
            _ => &self.socket_error,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    pub(super) fn decode_submitted(&self) {
        let queued = self.decode_queued.fetch_add(1, Ordering::Relaxed) + 1;
        self.max_decode_queued.fetch_max(queued, Ordering::Relaxed);
    }

    pub(super) fn decode_finished(&self) {
        self.decode_queued.fetch_sub(1, Ordering::Relaxed);
        self.decoded_frames.fetch_add(1, Ordering::Relaxed);
    }

    pub(super) fn decode_busy(&self, duration: Duration) {
        self.decode_busy_ns.fetch_add(
            duration.as_nanos().min(u64::MAX as u128) as u64,
            Ordering::Relaxed,
        );
    }

    pub(super) fn decode_rejected(&self) {
        self.decode_queued.fetch_sub(1, Ordering::Relaxed);
    }

    pub(super) fn encode_submitted(&self) {
        let queued = self.encode_queued.fetch_add(1, Ordering::Relaxed) + 1;
        self.max_encode_queued.fetch_max(queued, Ordering::Relaxed);
    }

    pub(super) fn encode_finished(&self) {
        self.encode_queued.fetch_sub(1, Ordering::Relaxed);
        self.encoded_frames.fetch_add(1, Ordering::Relaxed);
    }

    pub(super) fn encode_busy(&self, duration: Duration) {
        self.encode_busy_ns.fetch_add(
            duration.as_nanos().min(u64::MAX as u128) as u64,
            Ordering::Relaxed,
        );
    }

    pub(super) fn encode_rejected(&self) {
        self.encode_queued.fetch_sub(1, Ordering::Relaxed);
    }

    pub(super) fn send_age(&self, age: Duration) {
        let milliseconds = age.as_millis().min(u64::MAX as u128) as u64;
        self.send_age_ms_max
            .fetch_max(milliseconds, Ordering::Relaxed);
        let bucket = (u64::BITS - milliseconds.saturating_sub(1).leading_zeros()) as usize;
        self.send_age_buckets[bucket.min(AGE_BUCKETS - 1)].fetch_add(1, Ordering::Relaxed);
    }
}
