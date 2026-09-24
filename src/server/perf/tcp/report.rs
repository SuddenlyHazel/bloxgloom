//! Honest acceptance summary for a real-socket run.

use super::TcpScene;
use super::client::ClientStats;
use crate::server::metrics::TickSample;
use crate::server::net::TransportSnapshot;
use crate::server::outbound::OutboundSnapshot;
use std::collections::HashSet;
use std::time::Duration;

#[derive(Debug)]
pub(in crate::server) struct TcpSoakReport {
    pub clients: usize,
    pub requested_ticks: usize,
    pub observed_ticks: usize,
    pub tick_p50: Duration,
    pub tick_p95: Duration,
    pub tick_p99: Duration,
    pub longest_backlog_run: usize,
    pub max_clients: u64,
    pub chunks: u64,
    pub deltas: u64,
    pub positions: u64,
    pub drop_snapshots: u64,
    pub pickups: u64,
    pub accepted_actions: u64,
    pub rejected_actions: u64,
    pub accepted_edits: u64,
    pub rejected_edits: u64,
    pub revision_gaps: u64,
    pub revision_regressions: u64,
    pub action_p95: Duration,
    pub outbound_peak_bytes: u64,
    pub max_client_queued_bytes: u64,
    pub outbound_end_bytes: u64,
    pub outbound_rejections: u64,
    pub transport: TransportSnapshot,
    pub reconnects: usize,
    pub occupied_regions: usize,
    pub wal_tail_reductions: usize,
    pub wal_tail_start_bytes: u64,
    pub wal_tail_end_bytes: u64,
    pub completed_wal_rotations: u64,
    pub passed: bool,
    pub reasons: Vec<String>,
}

pub(super) fn summarize(
    clients: usize,
    scene: TcpScene,
    requested_ticks: usize,
    samples: &[TickSample],
    client_stats: &[ClientStats],
    transport: TransportSnapshot,
    outbound: OutboundSnapshot,
    reconnects: usize,
) -> TcpSoakReport {
    let mut tick_times: Vec<_> = samples.iter().map(|sample| sample.tick_total).collect();
    tick_times.sort_unstable();
    let mut action_times: Vec<_> = client_stats
        .iter()
        .flat_map(|client| client.action_latencies.iter().copied())
        .collect();
    action_times.sort_unstable();
    let mut longest_backlog_run = 0;
    let mut backlog_run = 0;
    for sample in samples {
        if sample.backlog_ticks > 0 {
            backlog_run += 1;
        } else {
            backlog_run = 0;
        }
        longest_backlog_run = longest_backlog_run.max(backlog_run);
    }
    let sum = |f: fn(&ClientStats) -> u64| client_stats.iter().map(f).sum();
    let chunks = sum(|stats| stats.chunks);
    let deltas = sum(|stats| stats.deltas);
    let positions = sum(|stats| stats.positions);
    let drop_snapshots = sum(|stats| stats.drops);
    let pickups = sum(|stats| stats.pickups);
    let accepted_actions = sum(|stats| stats.accepted_actions);
    let rejected_actions = sum(|stats| stats.rejected_actions);
    let accepted_edits = sum(|stats| stats.accepted_edits);
    let rejected_edits = sum(|stats| stats.rejected_edits);
    let scheduled_actions =
        requested_ticks.saturating_sub(1) / 100 + usize::from(requested_ticks > 500);
    let revision_gaps = sum(|stats| stats.revision_gaps);
    let revision_regressions = sum(|stats| stats.revision_regressions);
    let occupied_regions = client_stats
        .iter()
        .filter_map(|stats| stats.last_position)
        .map(|position| {
            (
                (position[0].floor() as i32).div_euclid(crate::world::CHUNK_SIZE as i32),
                (position[2].floor() as i32).div_euclid(crate::world::CHUNK_SIZE as i32),
            )
        })
        .collect::<HashSet<_>>()
        .len();
    let tick_p95 = percentile(&tick_times, 95);
    let tick_p99 = percentile(&tick_times, 99);
    let wal_tail_reductions = samples
        .windows(2)
        .filter(|pair| pair[1].wal_tail_bytes < pair[0].wal_tail_bytes)
        .count();
    let completed_wal_rotations = samples
        .iter()
        .map(|sample| sample.wal_rotations)
        .max()
        .unwrap_or(0);
    let mut reasons = Vec::new();
    if samples.len() != requested_ticks {
        reasons.push(format!(
            "observed {} of {requested_ticks} requested tick samples",
            samples.len()
        ));
    }
    if samples
        .windows(2)
        .any(|pair| pair[1].tick_id != pair[0].tick_id + 1)
    {
        reasons.push("tick observer has a sequence gap".into());
    }
    if transport.max_active < clients as u64 {
        reasons.push(format!(
            "peak concurrent sockets {} < {clients}",
            transport.max_active
        ));
    }
    if client_stats.len() != clients {
        reasons.push(format!(
            "{} healthy client results < {clients}",
            client_stats.len()
        ));
    }
    if client_stats
        .iter()
        .any(|stats| stats.chunks == 0 || stats.positions == 0)
    {
        reasons.push("a healthy client received no terrain or movement position".into());
    }
    if (accepted_actions + rejected_actions) as usize != scheduled_actions {
        reasons.push(format!(
            "received {} of {scheduled_actions} scheduled action results",
            accepted_actions + rejected_actions
        ));
    }
    if accepted_actions == 0 || action_times.is_empty() || rejected_actions != 0 {
        reasons.push(format!(
            "durable actions: {accepted_actions} accepted, {rejected_actions} rejected"
        ));
    }
    if requested_ticks > 500 && (accepted_edits != 1 || rejected_edits != 0 || deltas == 0) {
        reasons.push(format!(
            "edit/delta stream incomplete: {accepted_edits} accepted edits, {rejected_edits} rejected, {deltas} deltas"
        ));
    }
    if revision_gaps != 0 {
        reasons.push(format!("{revision_gaps} received chunk revision gaps"));
    }
    if revision_regressions != 0 {
        reasons.push(format!(
            "{revision_regressions} received chunk revision regressions"
        ));
    }
    if tick_p95 > Duration::from_millis(15) || tick_p99 > Duration::from_millis(20) {
        reasons.push(format!(
            "tick p95/p99 {:.2}/{:.2} ms exceed 15/20 ms",
            tick_p95.as_secs_f64() * 1000.0,
            tick_p99.as_secs_f64() * 1000.0
        ));
    }
    if longest_backlog_run > 50 {
        reasons.push(format!(
            "backlog persisted {longest_backlog_run} ticks (>1 second)"
        ));
    }
    if samples.len() >= 2000 {
        let first = samples
            .iter()
            .take(1000)
            .map(|sample| sample.replication_bytes_queued)
            .sum::<u64>()
            / 1000;
        let last = samples
            .iter()
            .rev()
            .take(1000)
            .map(|sample| sample.replication_bytes_queued)
            .sum::<u64>()
            / 1000;
        if last > first.saturating_add(64 * 1024) {
            reasons.push(format!("outbound queue rose from {first} to {last} bytes"));
        }
    }
    if outbound.queued_bytes > 128 * 1024 * 1024 {
        reasons.push("aggregate outbound queue exceeded 128 MiB".into());
    }
    if outbound.max_queued_bytes > 128 * 1024 * 1024 {
        reasons.push("aggregate outbound high-water mark exceeded 128 MiB".into());
    }
    if outbound.max_client_queued_bytes > 2 * 1024 * 1024 {
        reasons.push("one client outbound queue exceeded 2 MiB".into());
    }
    if clients == 128 && requested_ticks == 15_000 && outbound.rejections < 8 {
        reasons.push(format!(
            "only {} outbound-limit rejections from eight active stalled readers",
            outbound.rejections
        ));
    }
    if reconnects == 0 {
        reasons.push("no reconnect probe completed".into());
    }
    if matches!(scene, TcpScene::Spread) && occupied_regions < 4 {
        reasons.push(format!(
            "spread scene occupied only {occupied_regions} horizontal chunk regions"
        ));
    }
    if transport.malformed < 8 || transport.timed_out < 8 {
        reasons.push(format!(
            "slow/malformed isolation incomplete: {} malformed, {} timeouts",
            transport.malformed, transport.timed_out
        ));
    }
    if clients == 128 && requested_ticks == 15_000 && completed_wal_rotations == 0 {
        reasons.push("no completed checkpoint-gated WAL rotation observed".into());
    }

    TcpSoakReport {
        clients,
        requested_ticks,
        observed_ticks: samples.len(),
        tick_p50: percentile(&tick_times, 50),
        tick_p95,
        tick_p99,
        longest_backlog_run,
        max_clients: transport.max_active,
        chunks,
        deltas,
        positions,
        drop_snapshots,
        pickups,
        accepted_actions,
        rejected_actions,
        accepted_edits,
        rejected_edits,
        revision_gaps,
        revision_regressions,
        action_p95: percentile(&action_times, 95),
        outbound_peak_bytes: outbound.max_queued_bytes,
        max_client_queued_bytes: outbound.max_client_queued_bytes,
        outbound_end_bytes: outbound.queued_bytes,
        outbound_rejections: outbound.rejections,
        transport,
        reconnects,
        occupied_regions,
        wal_tail_reductions,
        wal_tail_start_bytes: samples.first().map_or(0, |sample| sample.wal_tail_bytes),
        wal_tail_end_bytes: samples.last().map_or(0, |sample| sample.wal_tail_bytes),
        completed_wal_rotations,
        passed: reasons.is_empty(),
        reasons,
    }
}

fn percentile(values: &[Duration], percentile: usize) -> Duration {
    if values.is_empty() {
        return Duration::ZERO;
    }
    let index = values
        .len()
        .saturating_mul(percentile)
        .div_ceil(100)
        .saturating_sub(1);
    values[index.min(values.len() - 1)]
}

impl TcpSoakReport {
    pub fn print(&self) {
        println!(
            "TCP soak: {} healthy clients, {}/{} paced ticks, peak sockets {}, occupied regions {}",
            self.clients,
            self.observed_ticks,
            self.requested_ticks,
            self.max_clients,
            self.occupied_regions
        );
        println!(
            "ticks p50/p95/p99: {:.2}/{:.2}/{:.2} ms; longest backlog {} ticks",
            self.tick_p50.as_secs_f64() * 1000.0,
            self.tick_p95.as_secs_f64() * 1000.0,
            self.tick_p99.as_secs_f64() * 1000.0,
            self.longest_backlog_run
        );
        println!(
            "TCP terrain/deltas/positions/drop snapshots/pickups: {}/{}/{}/{}/{}; actions accepted/rejected {} / {} (edits {} / {}); action p95 {:.2} ms; revision gaps/regressions {} / {}",
            self.chunks,
            self.deltas,
            self.positions,
            self.drop_snapshots,
            self.pickups,
            self.accepted_actions,
            self.rejected_actions,
            self.accepted_edits,
            self.rejected_edits,
            self.action_p95.as_secs_f64() * 1000.0,
            self.revision_gaps,
            self.revision_regressions
        );
        println!(
            "transport: inbound/outbound {} / {} bytes, codec decode/encode {} / {}, peak pending {} / {}, send age p95 upper/max {} / {} ms",
            self.transport.inbound_bytes,
            self.transport.outbound_bytes,
            self.transport.decoded_frames,
            self.transport.encoded_frames,
            self.transport.max_decode_queued,
            self.transport.max_encode_queued,
            self.transport.send_age_ms_p95_upper,
            self.transport.send_age_ms_max
        );
        println!(
            "outbound aggregate peak/end {} / {} bytes; per-client peak {} bytes; rejections {}; reconnects {}; malformed/timeout/backpressure {} / {} / {}",
            self.outbound_peak_bytes,
            self.outbound_end_bytes,
            self.max_client_queued_bytes,
            self.outbound_rejections,
            self.reconnects,
            self.transport.malformed,
            self.transport.timed_out,
            self.transport.backpressure
        );
        println!(
            "reactor busy {:.2}s over {} passes; admission rejects {}",
            self.transport.reactor_busy_ns as f64 / 1e9,
            self.transport.reactor_passes,
            self.transport.admission_rejected
        );
        println!(
            "codec worker busy decode/encode {:.2}/{:.2}s (summed across four workers each)",
            self.transport.decode_busy_ns as f64 / 1e9,
            self.transport.encode_busy_ns as f64 / 1e9
        );
        println!(
            "sockets accepted/active/peak {} / {} / {}; EOF/socket-error {} / {}; codec outstanding decode/encode {} / {}",
            self.transport.accepted,
            self.transport.active,
            self.transport.max_active,
            self.transport.peer_eof,
            self.transport.socket_error,
            self.transport.decode_queued,
            self.transport.encode_queued
        );
        println!(
            "WAL tail start/end {} / {} bytes; observed reductions {}; completed checkpoint rotations {}",
            self.wal_tail_start_bytes,
            self.wal_tail_end_bytes,
            self.wal_tail_reductions,
            self.completed_wal_rotations
        );
        if self.passed {
            println!(
                "TCP configured-run checks: PASS (full gate requires 128 clients, 15000 ticks, both scenes)"
            );
        } else {
            println!(
                "TCP configured-run checks: FAIL — {}",
                self.reasons.join("; ")
            );
        }
    }
}
