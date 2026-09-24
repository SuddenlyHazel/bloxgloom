# Production TCP soak evidence

This document records the loopback network baseline, not the complete combined
foundation acceptance gate. The latter also requires dense fire and
seam-crossing kiln interactions during a matched 128-client soak. Neither is
driven by the TCP load generator yet.

## Method and machine

- Host: Apple M1 Pro, 10 logical CPUs, 16 GiB RAM, macOS; Rust 1.93.1.
- Date: 2026-09-24. Both final baseline runs used one release executable,
  SHA-256 `997395244fc9dd1bd88edc5452ff7d291506b7eed4065630a134f299f613afa0`.
  Git HEAD at build was `e6d57ebfb4aa5fafb42b3ed9bbf8784690e3fa42`;
  the SHA-256 of a sorted SHA-256 manifest of existing tracked and untracked
  `src/`, `assets/`, `Cargo.toml`, and `Cargo.lock` files immediately after
  build was `96eedf36c5430496537c554ca1acef6c16b1cb6f7c9fbd7654b07d2be2c9fdc6`.
  The worktree was dirty and other agents subsequently changed entity/kiln
  sources. These numbers identify the tested binary and nearby source snapshot;
  they do not make the later worktree or final commit equivalent to it.
- Build: `cargo build --release`; run the resulting production binary as
  `target/release/bloxgloom server-perf tcp --clients 128 --ticks 15000 --scene clustered`
  and again with `--scene spread`.
- Each scene uses a new isolated temporary world-v5 save and a real nonblocking
  production listener. The 128 healthy peers use actual loopback TCP sockets,
  complete the content manifest/Welcome handshake, receive terrain and
  positions, and send server-authorized movement every five ticks. Their
  inventory starts with 64 stone items. A rotating subset sends one DropStack
  action every 100 measured ticks; one client also breaks its support block at
  measured tick 500. The harness requires every action result, an accepted
  edit, at least one streamed delta, and no chunk revision gaps or regressions.
- After a 700-tick warmup, the measured window is 15,000 coordinator ticks
  paced at 50 Hz. In the spread scene, clients move legally through the server
  movement system into eight distinct horizontal chunk regions; there is no
  benchmark-only teleport. The harness retains eight additional peers that
  complete content negotiation and Welcome, request radius six, then stop
  reading; eight more stall during handshake and eight send malformed frames.
  A separate peer repeatedly reconnects using one stable profile. Healthy
  peers remain connected throughout the measured window.
- Only the temporary benchmark state sets
  `force_rotation_at_sequence = Some(20)`. Production rotation limits remain
  unchanged. Acceptance uses the completed checkpoint-gated rotation counter,
  not merely a WAL-tail size change.
- Tick p50/p95/p99 and backlog come from the production coordinator's bounded
  tick observer. Queue peaks are exact atomic high-water marks (aggregate and
  per-client), including sub-tick spikes. Reactor and codec-worker busy times
  are service-time sums, not whole-process CPU utilization or kernel network
  CPU. Action latency is request-send to result-receive at the load generator.

## Matched 128-client network baseline

Both commands above exited successfully with `TCP configured-run checks: PASS`
on that same binary. Each used 700 warmup ticks and 15,000 measured ticks at
50 Hz, then allowed one second for results to drain. The spread scene occupied
eight horizontal chunk regions; clustered occupied one.

| Measurement | Clustered | Spread |
| --- | ---: | ---: |
| Observed / requested ticks | 15,000 / 15,000 | 15,000 / 15,000 |
| Tick p50 / p95 / p99 | 0.66 / 3.05 / 4.26 ms | 0.40 / 2.18 / 2.97 ms |
| Longest tick backlog | 0 | 0 |
| Peak active sockets | 145 | 145 |
| Terrain chunks / deltas / positions | 9,600 / 128 / 401,920 | 15,136 / 48 / 401,920 |
| Drop snapshots / pickups | 358,912 / 150 | 153,616 / 150 |
| Accepted / rejected actions; accepted edits | 150 / 0; 1 | 150 / 0; 1 |
| Action latency p95 | 61.83 ms | 58.27 ms |
| Chunk revision gaps / regressions | 0 / 0 | 0 / 0 |
| TCP inbound / outbound bytes | 10,466,258 / 71,318,877 | 10,466,258 / 84,994,719 |
| Decode / encode jobs | 402,668 / 773,156 | 402,668 / 573,322 |
| Peak pending decode / encode | 128 / 128 | 128 / 136 |
| Send-age p95 bucket upper bound / max | 32 / 70 ms | 32 / 72 ms |
| Outbound aggregate peak / end | 3,503,698 / 0 bytes | 3,569,269 / 0 bytes |
| Per-client outbound peak | 530,136 bytes | 530,136 bytes |
| Outbound limit rejections | 13 | 8 |
| Stable-profile reconnects | 16 | 16 |
| Malformed / handshake timeout / input backpressure disconnects | 8 / 8 / 0 | 8 / 8 / 0 |
| Reactor busy time / passes | 14.88 s / 130,910 | 9.33 s / 160,788 |
| Codec decode / encode busy time (four-worker sums) | 0.13 / 0.55 s | 0.11 / 0.38 s |
| Accepted / active at end sockets; EOF / socket errors | 168 / 0; 143 / 9 | 168 / 0; 143 / 9 |
| Admission rejects; outstanding decode / encode at end | 0; 0 / 0 | 0; 0 / 0 |
| WAL tail first / last sample | 1,160 / 335,749 bytes | 1,160 / 335,749 bytes |
| Observed WAL-tail reductions; completed checkpoint rotations | 0; 1 | 0; 1 |

All eight authenticated non-reading peers in each scene were disconnected
after reaching the explicit 128-frame, 530,136-byte per-client outbound limit.
The clustered run also logged five `Disconnected` outbound reservations; the
reason-coded `FrameLimit` logs, not the aggregate rejection count alone,
establish slow-reader isolation. The zero observed WAL-tail reductions do not
negate the completed rotation counter: the journal tail can grow again after a
checkpoint-gated rotation. Neither run lost an action result or stopped
healthy-client progress.

## Preflight and historical diagnostics

Short release runs with the joined nonreading peers passed before the final
matched soak: 2 healthy clients × 600 measured ticks in the clustered scene
returned 6/6 actions including 1/1 edit, 2 deltas, 6 pickups, zero revision
gaps, one completed WAL rotation, and 600/600 observed tick samples.
The 8-client × 600-tick spread run occupied eight regions, returned 6/6 actions
including 1/1 edit, 3 deltas, 6 pickups, zero revision gaps, one completed
rotation, and 600/600 samples. In both short runs, all eight joined nonreaders
hit an explicit 128-frame `FrameLimit` and disconnected while healthy clients
continued.

Two earlier 128-client × 15,000-tick clustered diagnostics used only
handshake-stalled peers, so they are **not** the final slow-outbound isolation
result. The first predated the isolated rotation trigger: p95/p99 tick time
3.27/4.86 ms, 149/149 actions accepted, 149 pickups, no revision gaps or
backlog. The second used the trigger: p95/p99 3.29/4.16 ms, longest backlog
three ticks, 149/149 actions accepted, 149 pickups, one completed rotation, no
revision gaps, exact aggregate/per-client queue peaks 15,104/12,863 bytes.

## Limits of this evidence

The TCP baseline does not seed dense fire, exercise a seam-crossing kiln under
load, test WAN latency, or measure client GPU frame time. An accepted block edit
and streamed deltas establish only that those paths function during network
load; they do not substitute for the combined gameplay workload. A final
foundation claim requires the separate matched combined soak and live client
rendering check described in `docs/growth-foundation-plan.md`.

These runs also predate later BGEN v3/entity-kiln changes in the shared
worktree. They cannot validate those changes even though they ran while that
source work was in progress; only the frozen binary above was exercised.
