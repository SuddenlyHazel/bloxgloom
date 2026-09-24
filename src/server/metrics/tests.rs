use super::{LatencyEvent, Metric, MetricsRecorder, SAMPLE_CAPACITY, TickSample};
use std::time::Duration;

#[test]
fn nearest_rank_percentiles_use_one_based_ceiling_ranks() {
    let mut metrics = MetricsRecorder::new();
    for value in 1..=20 {
        metrics.record(TickSample {
            tick_total: Duration::from_nanos(value),
            ..TickSample::default()
        });
    }

    assert_eq!(
        metrics.summary(Metric::TickTotalNanos),
        Some(super::Summary {
            samples: 20,
            p50: 10,
            p95: 19,
            p99: 20,
            max: 20,
        })
    );
    assert_eq!(metrics.summary(Metric::PhaseNanos(5)), None);
}

#[test]
fn resident_and_entity_counters_are_sampled_without_recounting_world_state() {
    let mut metrics = MetricsRecorder::new();
    metrics.record(TickSample {
        resident_chunks: 317,
        active_clients: 16,
        active_drops: 42,
        ..TickSample::default()
    });

    assert_eq!(metrics.summary(Metric::ResidentChunks).unwrap().max, 317);
    assert_eq!(metrics.summary(Metric::ActiveClients).unwrap().max, 16);
    assert_eq!(metrics.summary(Metric::ActiveDrops).unwrap().max, 42);
}

#[test]
fn movement_worker_utilization_uses_busy_over_available_worker_time() {
    let mut metrics = MetricsRecorder::new();
    assert_eq!(metrics.movement_worker_utilization_percent(), None);
    metrics.record(TickSample {
        movement_worker_busy_nanos: 50,
        movement_worker_capacity_nanos: 100,
        ..TickSample::default()
    });
    metrics.record(TickSample {
        movement_worker_busy_nanos: 100,
        movement_worker_capacity_nanos: 200,
        ..TickSample::default()
    });
    assert_eq!(metrics.movement_worker_utilization_percent(), Some(50.0));
}

#[test]
fn ring_keeps_only_the_last_fixed_capacity_samples() {
    let mut metrics = MetricsRecorder::new();
    for tick in 1..=(SAMPLE_CAPACITY as u64 + 3) {
        metrics.record(TickSample {
            tick_id: tick,
            tick_total: Duration::from_nanos(tick),
            ..TickSample::default()
        });
    }

    assert_eq!(metrics.len(), SAMPLE_CAPACITY);
    assert_eq!(metrics.latest().map(|sample| sample.tick_id), Some(515));
    assert_eq!(metrics.lagged_samples(), 0);
    assert_eq!(
        metrics.summary(Metric::TickTotalNanos),
        Some(super::Summary {
            samples: 512,
            p50: 259,
            p95: 490,
            p99: 510,
            max: 515,
        })
    );
}

#[test]
fn empty_recorder_has_no_latest_sample_or_summary() {
    let metrics = MetricsRecorder::new();

    assert_eq!(metrics.len(), 0);
    assert_eq!(metrics.latest(), None);
    assert_eq!(metrics.summary(Metric::TickTotalNanos), None);
    assert_eq!(metrics.lagged_samples(), 0);
    assert_eq!(
        metrics.latency_summary(LatencyEvent::DurableWalReceipt),
        None
    );
}

#[test]
fn identifies_over_budget_ticks_and_clock_backlog_separately() {
    let on_budget = TickSample {
        tick_total: Duration::from_millis(20),
        ..TickSample::default()
    };
    let slow_tick = TickSample {
        tick_total: Duration::from_micros(20_001),
        ..TickSample::default()
    };
    let backlog_only = TickSample {
        backlog_ticks: 1,
        ..TickSample::default()
    };

    assert!(!on_budget.over_budget());
    assert!(!on_budget.has_backlog());
    assert!(!on_budget.is_lagging());
    assert!(slow_tick.over_budget());
    assert!(!slow_tick.has_backlog());
    assert!(slow_tick.is_lagging());
    assert!(!backlog_only.over_budget());
    assert!(backlog_only.has_backlog());
    assert!(backlog_only.is_lagging());

    let mut metrics = MetricsRecorder::new();
    metrics.record(on_budget);
    metrics.record(slow_tick);
    metrics.record(backlog_only);
    assert_eq!(metrics.lagged_samples(), 2);
}

#[test]
fn event_latency_streams_report_nearest_rank_summaries_independently() {
    let mut metrics = MetricsRecorder::new();
    for nanos in 1..=20 {
        metrics.record_latency(LatencyEvent::DurableWalReceipt, Duration::from_nanos(nanos));
    }
    metrics.record_latency(LatencyEvent::ChunkLoad, Duration::from_millis(7));
    metrics.record_latency(LatencyEvent::PhaseBarrierWait, Duration::from_micros(42));

    assert_eq!(
        metrics.latency_summary(LatencyEvent::DurableWalReceipt),
        Some(super::Summary {
            samples: 20,
            p50: 10,
            p95: 19,
            p99: 20,
            max: 20,
        })
    );
    assert_eq!(
        metrics.latency_summary(LatencyEvent::ChunkLoad),
        Some(super::Summary {
            samples: 1,
            p50: 7_000_000,
            p95: 7_000_000,
            p99: 7_000_000,
            max: 7_000_000,
        })
    );
    assert_eq!(
        metrics.latency_summary(LatencyEvent::PhaseBarrierWait),
        Some(super::Summary {
            samples: 1,
            p50: 42_000,
            p95: 42_000,
            p99: 42_000,
            max: 42_000,
        })
    );
}

#[test]
fn event_latency_ring_discards_oldest_observations_on_wrap() {
    let mut metrics = MetricsRecorder::new();
    for value in 1..=(SAMPLE_CAPACITY as u64 + 2) {
        metrics.record_latency(LatencyEvent::ChunkLoad, Duration::from_nanos(value));
    }

    assert_eq!(
        metrics.latency_summary(LatencyEvent::ChunkLoad),
        Some(super::Summary {
            samples: 512,
            p50: 258,
            p95: 489,
            p99: 509,
            max: 514,
        })
    );
    assert_eq!(
        metrics.latency_summary(LatencyEvent::PhaseBarrierWait),
        None
    );
}
