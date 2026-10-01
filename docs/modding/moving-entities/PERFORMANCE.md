# Moving-entity acceptance measurements

Measured on 2026-10-01 on an Apple M1 Pro with 16 GiB RAM, using the optimized
release test profile. This is a real nonblocking TCP listener, an isolated save,
finite launch inventory and the production movement/WAL/replication path.

The probe launches batches of at most 32 bodies, waits for the committed
inventory revision between batches, then observes every body moving and one
crossing an owner-chunk seam. It checks durable recovery and item conservation.
The 64-body case rejects two additional bodies in an already occupied chunk;
the 256-body case rejects the 257th body. Rejections leave inventory unchanged.

Each body has a small box, no gravity, terrain collisions, two authored bytes
and a Lua callback that reads its current motion. Interval 1 stresses the most
frequent authored callback cadence; interval 10 matches the runnable fixture.
The observation lasts at least two seconds and continues until all bodies have
moved and the seam crossing is observed, with a 60-second bound. Discovery time
and initial admission are excluded from the per-tick measurements.

| Bodies | Callback interval | Tick p50 / p95 / p99 (ms) | Launch p50 / p95 (ms) | Launch samples |
| --- | --- | --- | --- | --- |
| 1 | 1 tick | 0.416 / 0.955 / 1.191 | 39.187 / 39.187 | 1 |
| 64 | 1 tick | 7.619 / 9.891 / 13.827 | 38.709 / 45.527 | 2 |
| 256 | 1 tick | 54.932 / 75.121 / 79.740 | 42.883 / 63.941 | 8 |
| 256 | 10 ticks | 20.651 / 54.274 / 62.858 | 27.347 / 43.757 | 8 |

Launch percentiles use nearest rank. With only 1, 2 or 8 samples, p95 and p99
are the maximum observed launch latency; these are acceptance probes, not a
statistical latency study.

| Bodies / interval | Capture p95 (ms/tick) | Solver p95 (ms/tick) | Commit phase p95 (ms/tick) | Deferred / attempted plans | Encoded private record bytes |
| --- | --- | --- | --- | --- | --- |
| 1 / 1 | 0.010 | 0.004 | 0.016 | 0 / 50 | 94 |
| 64 / 1 | 0.074 | 0.007 | 0.016 | 32 / 3264 | 6016 |
| 256 / 1 | 0.169 | 0.015 | 0.043 | 96 / 7168 | 24064 |
| 256 / 10 | 0.185 | 0.016 | 0.039 | 2 / 6974 | 24064 |

Capture and solver timings are sums of successful native substeps in a tick,
recorded through a bounded test-only observer. Deferred counts include transient
terrain availability; these runs produced zero failed motion plans. Commit
phase timing covers the whole server phase, not exclusively motion. Private
record bytes are the sum of encoded recovered records; they exclude entity
indexes, VM state, terrain, allocator overhead and process RSS.

Replica entity traffic was 9865, 601714 and 1367067 bytes for the interval-1
1/64/256 runs, with 49, 3012 and 6784 observed entity upserts respectively. The
256-body interval-10 run produced 1341483 bytes and 6848 upserts. These are
whole observation totals, including entity snapshot/commit envelopes, rather
than rates measured over equal wall-clock windows.

The hard limit of 256 bodies bounds ownership and storage safely. It does not
guarantee a 20 ms server tick: 256 bodies exceeded that budget even at interval
10 on this machine. The 64-body stress workload remained below 20 ms at p99.
The dominant cost at 256 was the combined durable planning/Lua phase (p95
70.140 ms at interval 1 and 49.149 ms at interval 10), rather than collision
capture or the solver. Use these measured profiles when choosing package spawn
and callback rates; overloaded bodies remain bounded and can slow down.

A single interval-1 body executed 49 fixed 0.04-second steps over 101 observed
logical ticks. This protects the bounded catch-up behavior against the former
physics slowdown caused by a separate authored callback transaction. Restart,
dormancy and missing-terrain recovery begin with one step and do not simulate
elapsed inactive time.

Reproduce the stress runs with:

```sh
cargo test --release moving_real_listener_capacity_measurements -- --ignored --nocapture --test-threads=1
```

Reproduce the fixture-cadence run with:

```sh
BLOXGLOOM_MOTION_LOAD_COUNT=256 BLOXGLOOM_MOTION_LOAD_INTERVAL=10 cargo test --release moving_real_listener_capacity_measurements -- --ignored --nocapture --test-threads=1
```

`BLOXGLOOM_MOTION_LOAD_COUNT` selects 1, 64 or 256 bodies;
`BLOXGLOOM_MOTION_LOAD_INTERVAL` accepts 1–1000 logical ticks and defaults to 1.
Debug builds passed the same ownership/conservation checks but are excluded
from production latency conclusions.
