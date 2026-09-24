//! Text reporting for authoritative server benchmark measurements.

use super::super::State;
use super::super::metrics::{LatencyEvent, Metric, PHASE_COUNT};
use super::super::simulation::FIXED_STEP;
use super::fixture::{ACTION_INTERVAL, DROP_HEIGHTS, DrainTotals, PLAYER_COUNT, Scenario};
use std::time::Duration;

const PHASE_NAMES: [&str; PHASE_COUNT] = [
    "input-authorization",
    "durable-actions",
    "simulation",
    "interaction-commit",
    "publish",
];

pub(super) fn print_report(
    scenario: Scenario,
    ticks: usize,
    wall: Duration,
    state: &State,
    drains: &DrainTotals,
) {
    println!(
        "{} measured: {} ticks in {:.3}s wall (target {:.3}s), {PLAYER_COUNT} active clients, {} retained metrics samples",
        scenario.name(),
        ticks,
        wall.as_secs_f64(),
        ticks as f64 * FIXED_STEP.as_secs_f64(),
        state
            .metrics
            .summary(Metric::TickTotalNanos)
            .map_or(0, |s| s.samples),
    );
    print_metric("tick CPU", state, Metric::TickTotalNanos, true);
    print_metric("backlog ticks", state, Metric::BacklogTicks, false);
    for (index, name) in PHASE_NAMES.into_iter().enumerate() {
        print_metric(name, state, Metric::PhaseNanos(index), true);
    }
    print_latency("phase barrier wait", state, LatencyEvent::PhaseBarrierWait);
    print_latency(
        "durable WAL receipt",
        state,
        LatencyEvent::DurableWalReceipt,
    );
    print_latency("chunk load", state, LatencyEvent::ChunkLoad);
    println!(
        "  worker utilization: {} (movement-worker pool only; other pools not measured)",
        state
            .metrics
            .movement_worker_utilization_percent()
            .map_or_else(
                || "no active dispatch samples".to_owned(),
                |value| format!("{value:.2}%")
            )
    );
    print_metric("resident chunks", state, Metric::ResidentChunks, false);
    print_metric("active clients", state, Metric::ActiveClients, false);
    print_metric("airborne active drops", state, Metric::ActiveDrops, false);
    print_metric(
        "pending durable actions",
        state,
        Metric::PendingDurableActions,
        false,
    );
    print_metric(
        "pending world snapshots",
        state,
        Metric::PendingWorldSnapshots,
        false,
    );
    print_metric(
        "loader outstanding",
        state,
        Metric::LoaderOutstanding,
        false,
    );
    print_metric(
        "outbound queued bytes",
        state,
        Metric::ReplicationBytesQueued,
        false,
    );
    print_metric(
        "outbound queue depth",
        state,
        Metric::ReplicationQueueDepth,
        false,
    );
    print_metric(
        "outbound sent bytes/tick",
        state,
        Metric::ReplicationBytesSent,
        false,
    );
    print_metric(
        "outbound queue rejections/tick",
        state,
        Metric::ReplicationQueueRejections,
        false,
    );
    let outbound = state.outbound.snapshot();
    println!(
        "  outbound sink totals: {} frames / {} bytes; {} accepted and {} rejected durable action results; final queue {} bytes / {} frames",
        drains.sent_frames,
        drains.sent_bytes,
        drains.accepted_actions,
        drains.rejected_actions,
        outbound.queued_bytes,
        outbound.queued_messages,
    );
    let scheduled_actions = (ticks / ACTION_INTERVAL) * PLAYER_COUNT;
    println!(
        "  durable receipt floor: {} accepted of {} scheduled (minimum {})",
        drains.accepted_actions,
        scheduled_actions,
        scheduled_actions * 3 / 4,
    );
    println!(
        "  caveats: no TCP socket writes; drop snapshots are still encoded synchronously on the coordinator during interaction checkpoint capture ({} seeded drops; not a large-drop serialization benchmark)",
        PLAYER_COUNT * DROP_HEIGHTS.len(),
    );
}

fn print_metric(label: &str, state: &State, metric: Metric, nanos: bool) {
    let Some(summary) = state.metrics.summary(metric) else {
        println!("  {label}: no samples");
        return;
    };
    let (unit, scale) = if nanos {
        ("ms", 1_000_000.0)
    } else {
        ("", 1.0)
    };
    let convert = |value: u64| value as f64 / scale;
    println!(
        "  {label}: p50 {:.3}{unit}, p95 {:.3}{unit}, p99 {:.3}{unit}, max {:.3}{unit} (n={})",
        convert(summary.p50),
        convert(summary.p95),
        convert(summary.p99),
        convert(summary.max),
        summary.samples,
    );
}

fn print_latency(label: &str, state: &State, event: LatencyEvent) {
    match state.metrics.latency_summary(event) {
        Some(summary) => println!(
            "  {label}: p50 {:.3}ms, p95 {:.3}ms, p99 {:.3}ms, max {:.3}ms (n={})",
            summary.p50 as f64 / 1_000_000.0,
            summary.p95 as f64 / 1_000_000.0,
            summary.p99 as f64 / 1_000_000.0,
            summary.max as f64 / 1_000_000.0,
            summary.samples,
        ),
        None => println!("  {label}: no samples"),
    }
}
