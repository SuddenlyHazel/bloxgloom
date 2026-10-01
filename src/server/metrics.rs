//! Fixed-capacity server tick telemetry.
//!
//! Recording a tick only copies scalar fields into a preallocated ring. Metric
//! summaries sort a stack buffer on demand, outside the simulation hot path.

use std::time::Duration;

#[cfg(test)]
#[path = "metrics/tests.rs"]
mod tests;

pub(super) const SAMPLE_CAPACITY: usize = 512;
pub(super) const PHASE_COUNT: usize = 5;
/// One server simulation step at 50 Hz. The client frame target is separate.
pub(super) const TICK_BUDGET: Duration = Duration::from_millis(20);

/// One completed fixed-step tick's counters and phase timings.
///
/// Populate once after the tick. The recorder copies this fixed-size sample
/// into preallocated storage, so `record` is allocation-free and cheap enough
/// for every tick.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct TickSample {
    pub(super) tick_id: u64,
    pub(super) tick_total: Duration,
    /// Ordered as input authorization, durable actions, simulation,
    /// interaction commit, and publish.
    pub(super) phases: [Duration; PHASE_COUNT],
    pub(super) backlog_ticks: u64,
    pub(super) input_queue_depth: u64,
    pub(super) pending_durable_actions: u64,
    pub(super) pending_world_snapshots: u64,
    pub(super) wal_tail_bytes: u64,
    pub(super) wal_rotations: u64,
    pub(super) loader_outstanding: u64,
    pub(super) resident_chunks: u64,
    pub(super) pinned_chunks: u64,
    pub(super) active_clients: u64,
    pub(super) active_drops: u64,
    /// Bounded entity checkpoint work, including pre-WAL reservations.
    pub(super) entity_mirror_outstanding: u64,
    pub(super) entity_mirror_high_water: u64,
    pub(super) entity_mirror_applied_sequence: u64,
    pub(super) entity_mirror_checkpoint_sequence: u64,
    /// Movement-job closure time summed across workers.
    pub(super) movement_worker_busy_nanos: u64,
    /// Configured movement workers multiplied by dispatch-to-barrier time.
    pub(super) movement_worker_capacity_nanos: u64,
    pub(super) replication_bytes_queued: u64,
    pub(super) replication_bytes_sent: u64,
    pub(super) replication_queue_depth: u64,
    pub(super) replication_queue_capacity: u64,
    pub(super) replication_queue_rejections: u64,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct MotionSample {
    pub(super) attempts: u64,
    pub(super) deferred: u64,
    pub(super) failed: u64,
    pub(super) capture: Duration,
    pub(super) solve: Duration,
}

impl TickSample {
    /// True when this tick alone exceeded the server's 20 ms (50 Hz) budget.
    pub(super) fn over_budget(self) -> bool {
        self.tick_total > TICK_BUDGET
    }

    /// Backlog is independent of an individual tick's duration: the clock may
    /// still be catching up after a delayed coordinator wake-up.
    pub(super) const fn has_backlog(self) -> bool {
        self.backlog_ticks > 0
    }

    pub(super) fn is_lagging(self) -> bool {
        self.over_budget() || self.has_backlog()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)] // Full scalar query surface is consumed by the planned headless server benchmark.
pub(super) enum Metric {
    TickTotalNanos,
    PhaseNanos(usize),
    BacklogTicks,
    InputQueueDepth,
    PendingDurableActions,
    PendingWorldSnapshots,
    WalTailBytes,
    LoaderOutstanding,
    ResidentChunks,
    PinnedChunks,
    ActiveClients,
    ActiveDrops,
    EntityMirrorOutstanding,
    EntityMirrorHighWater,
    EntityMirrorAppliedSequence,
    EntityMirrorCheckpointSequence,
    MovementWorkerBusyNanos,
    MovementWorkerCapacityNanos,
    ReplicationBytesQueued,
    ReplicationBytesSent,
    ReplicationQueueDepth,
    ReplicationQueueCapacity,
    ReplicationQueueRejections,
}

impl Metric {
    fn value(self, sample: TickSample) -> Option<u64> {
        match self {
            Self::TickTotalNanos => Some(duration_nanos(sample.tick_total)),
            Self::PhaseNanos(index) => sample.phases.get(index).copied().map(duration_nanos),
            Self::BacklogTicks => Some(sample.backlog_ticks),
            Self::InputQueueDepth => Some(sample.input_queue_depth),
            Self::PendingDurableActions => Some(sample.pending_durable_actions),
            Self::PendingWorldSnapshots => Some(sample.pending_world_snapshots),
            Self::WalTailBytes => Some(sample.wal_tail_bytes),
            Self::LoaderOutstanding => Some(sample.loader_outstanding),
            Self::ResidentChunks => Some(sample.resident_chunks),
            Self::PinnedChunks => Some(sample.pinned_chunks),
            Self::ActiveClients => Some(sample.active_clients),
            Self::ActiveDrops => Some(sample.active_drops),
            Self::EntityMirrorOutstanding => Some(sample.entity_mirror_outstanding),
            Self::EntityMirrorHighWater => Some(sample.entity_mirror_high_water),
            Self::EntityMirrorAppliedSequence => Some(sample.entity_mirror_applied_sequence),
            Self::EntityMirrorCheckpointSequence => Some(sample.entity_mirror_checkpoint_sequence),
            Self::MovementWorkerBusyNanos => Some(sample.movement_worker_busy_nanos),
            Self::MovementWorkerCapacityNanos => Some(sample.movement_worker_capacity_nanos),
            Self::ReplicationBytesQueued => Some(sample.replication_bytes_queued),
            Self::ReplicationBytesSent => Some(sample.replication_bytes_sent),
            Self::ReplicationQueueDepth => Some(sample.replication_queue_depth),
            Self::ReplicationQueueCapacity => Some(sample.replication_queue_capacity),
            Self::ReplicationQueueRejections => Some(sample.replication_queue_rejections),
        }
    }
}

/// Nearest-rank summary (`ceil(p * n)`, one-based) for a metric's samples.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Summary {
    pub(super) samples: usize,
    pub(super) p50: u64,
    pub(super) p95: u64,
    pub(super) p99: u64,
    pub(super) max: u64,
}

/// Last 512 completed ticks. It never grows and has no heap allocation while
/// recording; summaries use a bounded stack scratch array when requested.
pub(super) struct MetricsRecorder {
    samples: [TickSample; SAMPLE_CAPACITY],
    start: usize,
    len: usize,
    event_latencies: [LatencyRing; EVENT_LATENCY_STREAMS],
}

const EVENT_LATENCY_STREAMS: usize = 3;

/// A sparse event latency tracked independently from tick-duration samples.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LatencyEvent {
    DurableWalReceipt,
    ChunkLoad,
    PhaseBarrierWait,
}

impl LatencyEvent {
    const fn index(self) -> usize {
        match self {
            Self::DurableWalReceipt => 0,
            Self::ChunkLoad => 1,
            Self::PhaseBarrierWait => 2,
        }
    }
}

struct LatencyRing {
    values: [u64; SAMPLE_CAPACITY],
    start: usize,
    len: usize,
}

impl LatencyRing {
    const fn new() -> Self {
        Self {
            values: [0; SAMPLE_CAPACITY],
            start: 0,
            len: 0,
        }
    }

    fn record(&mut self, latency: Duration) {
        let index = (self.start + self.len) % SAMPLE_CAPACITY;
        self.values[index] = duration_nanos(latency);
        if self.len == SAMPLE_CAPACITY {
            self.start = (self.start + 1) % SAMPLE_CAPACITY;
        } else {
            self.len += 1;
        }
    }

    fn summary(&self) -> Option<Summary> {
        let mut values = [0u64; SAMPLE_CAPACITY];
        for (offset, value) in values[..self.len].iter_mut().enumerate() {
            *value = self.values[(self.start + offset) % SAMPLE_CAPACITY];
        }
        summarize(&mut values[..self.len])
    }
}

impl Default for MetricsRecorder {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsRecorder {
    pub(super) const fn new() -> Self {
        Self {
            samples: [TickSample {
                tick_id: 0,
                tick_total: Duration::ZERO,
                phases: [Duration::ZERO; PHASE_COUNT],
                backlog_ticks: 0,
                input_queue_depth: 0,
                pending_durable_actions: 0,
                pending_world_snapshots: 0,
                wal_tail_bytes: 0,
                wal_rotations: 0,
                loader_outstanding: 0,
                resident_chunks: 0,
                pinned_chunks: 0,
                active_clients: 0,
                active_drops: 0,
                entity_mirror_outstanding: 0,
                entity_mirror_high_water: 0,
                entity_mirror_applied_sequence: 0,
                entity_mirror_checkpoint_sequence: 0,
                movement_worker_busy_nanos: 0,
                movement_worker_capacity_nanos: 0,
                replication_bytes_queued: 0,
                replication_bytes_sent: 0,
                replication_queue_depth: 0,
                replication_queue_capacity: 0,
                replication_queue_rejections: 0,
            }; SAMPLE_CAPACITY],
            start: 0,
            len: 0,
            event_latencies: [LatencyRing::new(), LatencyRing::new(), LatencyRing::new()],
        }
    }

    pub(super) fn record(&mut self, sample: TickSample) {
        let index = (self.start + self.len) % SAMPLE_CAPACITY;
        self.samples[index] = sample;
        if self.len == SAMPLE_CAPACITY {
            self.start = (self.start + 1) % SAMPLE_CAPACITY;
        } else {
            self.len += 1;
        }
    }

    #[cfg(test)]
    pub(super) const fn len(&self) -> usize {
        self.len
    }

    pub(super) fn latest(&self) -> Option<TickSample> {
        if self.len == 0 {
            None
        } else {
            let index = (self.start + self.len - 1) % SAMPLE_CAPACITY;
            Some(self.samples[index])
        }
    }

    /// Utilization of the movement worker pool over sampled active dispatch
    /// intervals. Other worker pools are not included in this percentage.
    pub(super) fn movement_worker_utilization_percent(&self) -> Option<f64> {
        let (busy, capacity) = self
            .iter()
            .fold((0u128, 0u128), |(busy, capacity), sample| {
                (
                    busy + u128::from(sample.movement_worker_busy_nanos),
                    capacity + u128::from(sample.movement_worker_capacity_nanos),
                )
            });
        (capacity != 0).then_some(busy as f64 * 100.0 / capacity as f64)
    }

    /// Number of currently retained samples whose individual tick duration
    /// exceeded the fixed-step budget or whose observed clock had backlog.
    pub(super) fn lagged_samples(&self) -> usize {
        self.iter().filter(|sample| sample.is_lagging()).count()
    }

    pub(super) fn summary(&self, metric: Metric) -> Option<Summary> {
        let mut values = [0u64; SAMPLE_CAPACITY];
        let mut count = 0;
        for sample in self.iter() {
            values[count] = metric.value(sample)?;
            count += 1;
        }
        summarize(&mut values[..count])
    }

    /// Record one completed event latency without allocating. Streams retain
    /// their own last 512 observations and are summarized on demand.
    pub(super) fn record_latency(&mut self, event: LatencyEvent, latency: Duration) {
        self.event_latencies[event.index()].record(latency);
    }

    pub(super) fn latency_summary(&self, event: LatencyEvent) -> Option<Summary> {
        self.event_latencies[event.index()].summary()
    }

    fn iter(&self) -> impl Iterator<Item = TickSample> + '_ {
        (0..self.len).map(|offset| self.samples[(self.start + offset) % SAMPLE_CAPACITY])
    }
}

fn summarize(values: &mut [u64]) -> Option<Summary> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    Some(Summary {
        samples: values.len(),
        p50: nearest_rank(values, 50),
        p95: nearest_rank(values, 95),
        p99: nearest_rank(values, 99),
        max: values[values.len() - 1],
    })
}

fn nearest_rank(sorted: &[u64], percentile: usize) -> u64 {
    debug_assert!(!sorted.is_empty());
    let rank = (percentile * sorted.len()).div_ceil(100).max(1);
    sorted[rank - 1]
}

fn duration_nanos(duration: Duration) -> u64 {
    duration.as_nanos().min(u64::MAX as u128) as u64
}
