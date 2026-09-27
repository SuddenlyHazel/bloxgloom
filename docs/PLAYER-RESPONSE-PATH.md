# Player-response path

This is a fixed host scheduling policy, not a configurable mod scheduler. New
entities inherit it through the existing authoritative action/tick adapters.
World/save/wire formats are unchanged (`world-v14` and `world-v14-fixture`).

## Admission and progress

- Movement keeps its existing per-tick authoritative path.
- The bounded durable queue reserves 128 entries for player work and 128 for
  simulation. Player work allows 96 explicit commands (at most eight queued per
  client) and 32 automatic pickups. Overflow commands receive the existing retry
  response; IDs, receipt deduplication and per-profile order are preserved.
- Pickup scans rotate by client ID. Due entities, wakes and sleeping-entity
  checks retain their existing circular scans inside the simulation allowance.
- Mixed admission rounds use three player-first turns, then one simulation-first
  turn. The cursor advances on mixed rounds, not wall-clock/tick parity. Both
  lanes can admit independent work on every round. Missing terrain still defers.
- Conflicting motion candidates are removed before combining a batch, so one
  nearby edit does not stall distant entities in that batch.
- Both lanes plan against committed state; no speculative apply occurs between
  lanes. The simulation boundary drains accepted work once at the end when motion
  or contending player work staged. There is no tactical mid-wave wait. Existing
  standalone asynchronous command/transfer paths remain asynchronous.
- Accepted journal order, exact preimages, shared read reservations, atomic
  inventories, recovery and confirmed publication remain authoritative. A turn
  guarantees an admission opportunity, not a deadline when storage/loads fail.

## Streaming and rendering

Committed updates already publish before new snapshots. The ordered socket queue
now admits at most eight frames / 128 KiB of bulk backlog, or one indivisible
larger snapshot group. Admitted frames are never reordered across dependencies.

There are two mesh priorities: immediate edited geometry/face seams and background
streaming/light refinement. Full light dependencies are still invalidated, including
roof columns; only scheduling changes. A mailbox holds at most 16 immediate and
64 background jobs, coalesces by chunk, cancels superseded queued work, and shares
a three-immediate/one-background cursor between the two workers. Submission and
GPU upload also leave background service turns. Client/GPU queues reserve room
for immediate results so a full streaming backlog cannot hide them. Version and
lighting-revision checks still discard stale results; all lighting/meshing stays
off the window thread.

## Verification and measurements

Personal implementation/review; no subagents. 702 workspace/all-feature tests
passed, strict workspace Clippy and formatting passed. Regression coverage:

- `response_fairness_tests.rs`: sustained overlapping player writes and entity
  reads both progress; distant motion advances on every round; automatic work and
  one actor's backlog leave command capacity for another actor.
- `mesh_queue_tests.rs` and renderer tests: coalescing/promotion, background
  progress under immediate pressure, reserved capacity, invalidation and shutdown.
- `mixed_response_path_keeps_edits_creatures_and_machine_progressing_across_restart`:
  real nonblocking listener, production client replication and mesh workers,
  player movement, paused/moving Copperlings, Crusher output, seam edits, both
  lighting modes, and restart. Machine output is conserved across restart.
- Existing receipt/held-WAL/conflict, inventory, lighting/cave/seam and networking
  tests pass. Capacity tests reflect the reserved simulation lane. The hopper
  recovery test now waits for bounded processing completion rather than assuming
  a fixed count of async admission attempts equals committed processing steps.

Repeat the release response measurement:

```sh
cargo test --release --all-features mixed_response_path -- --nocapture
```

Local M1 Pro sample, eight edits per row and lighting mode; milliseconds:

| Scene | Voxel confirmation / through mesh median | Bounced confirmation / through mesh median |
| --- | --- | --- |
| Baseline without fixture creatures/machine | 20.6 / 24.2 | 22.0 / 25.7 |
| Mixed load | 32.8 / 34.4 | 30.0 / 34.4 |
| Mixed after restart | 29.2 / 35.0 | 25.3 / 28.3 |

These stop at worker mesh readiness, not display presentation. They are release
observations, not fragile test time limits. No accumulating backlog or starvation
was observed. For the actual window, launch with `BLOXGLOOM_TRACE_EDITS=1`.
The common process clock records send, server receive/admit/commit, committed
chunk/version, client apply, mesh submit/start/ready, GPU upload and frame present.
Correlate action IDs, chunk versions and lighting revisions. Separate-process
client/server clocks must not be subtracted as though synchronized.

Release terrain preview inspected. Headless `perf 300 6` before/after: voxel
setup 1683.4→1686.9 ms, steady CPU .308→.315 ms, GPU .290→.226 ms; bounced setup
2045.5→2115.2 ms, CPU .351→.312 ms, GPU .296→.281 ms. Mesh sizes unchanged at
17,292,744 / 17,500,968 bytes. These GPU/frame measurements exclude live gameplay
and presentation and do not establish a user-visible latency guarantee.
