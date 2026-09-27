# Kiln: the shared anchored-entity path

The Kiln is a two-block workstation with three server-owned slots: fuel, input,
and output. The initial recipe converts gravel to stone in four scheduled pulses.
Its public screen shows slots, remaining burn time, and recipe-relative progress.

## Shared path

1. Startup registers its catalog identity, private codec, interaction policy,
   and tick planner through `EntityTypeRegistryBuilder`.
2. Placement atomically consumes one item, creates the anchored entity and both
   footprint blocks. Either half resolves to that same entity, including seams.
3. Screen clicks use `EntityInteract`, not a second inventory protocol. Version-2
   requests include entity identity and revision so an outdated open screen cannot
   modify a replacement Kiln or overwrite a concurrent change. All 36 inventory
   slots are supported; legacy keyboard requests still work.
4. The coordinator validates reach and the target, plans the inventory/entity
   transaction, reserves dependencies, and submits it to the shared journal.
   Nothing becomes visible before the durable receipt. Retries are idempotent.
5. Cooking runs in the existing entity worker dispatch against immutable input.
   Its plan changes private state and lit block states in the same transaction.
   Output capacity, fuel use, and stack conservation stay server-owned.
6. Normal committed entity snapshots/deltas carry a bounded 24-byte public
   workstation summary. Nearby players see slot item IDs/counts and cooking
   status; arbitrary item components remain private. The UI never predicts item
   ownership. Its geometry cache includes workstation state so ticks repaint.
7. Recovery restores slots, fuel, progress, and due work. The shared journal
   clock ensures cooking resumes after restart. Breaking either half atomically
   removes the complete footprint and turns the stored stacks into world drops.

The client closes the screen when the target disappears, changes identity,
leaves reach, or the connection closes. Rejected actions use existing status
receipts. Local source selection is cosmetic and never removes an item.

## Content and checks

Kiln entity schema 2 expands public presentation; the new default world folder
was `world-v8` when that schema landed; Hopper content now makes the default
`world-v9`. The masonry and dark/lit vents are original generated assets in
`assets/textures/blocks/`, reproducible with `source/kiln.py`.

Tests exercise backpack transfers and stale-target rejection, public payload
bounds/privacy, both-half UI resolution, compact layout hit testing, and the
durable place/load/cook/restart/collect/retry/break flow across a vertical seam.
The existing full-output and fuel-consumption tests remain in place.
The full suite passed **627 tests**, with clean formatting and strict all-target,
all-feature Clippy.

The inspected offscreen previews use the production UI and material renderer;
they are not a live gameplay recording. A single `perf 300 6` comparison against
`7f9fdf2` kept mesh size at 17,292,744 bytes and visible triangles at 88,026.
Scene setup was 1768.0 → 1723.9 ms, steady CPU median 0.331 → 0.321 ms, and
steady GPU median 0.280 → 0.293 ms. This terrain scene does not measure active
Kiln simulation or multiplayer inventory contention.

## Placement-latency investigation

A real nonblocking-listener test now places and removes blocks in the Kiln's
chunk and across a neighboring seam, first idle and then burning. The initial
release probe measured acknowledgement at 27–86 ms idle and 24–56 ms burning;
it did not reproduce a substantial server-side stall. These are observations,
not timing assertions in the test. The test also checks that authoritative
block publication has arrived when the action is acknowledged.

The client did have avoidable queue latency: invalidated light/mesh jobs were
still computed, then discarded only after reaching the window thread. A
16-job superseded-relight probe took 420 ms to deliver the newest mesh in a
debug build, completing 15 useless results. Worker-side revision cancellation
reduced the same exploratory probe to 54 ms and zero obsolete results. Workers
now skip stale/evicted jobs before lighting, recheck after lighting, and recheck
before publishing a mesh. An already-running lighting pass is not interrupted.
Existing result/upload revision checks still protect against races.

A deterministic regression queues superseded jobs after invalidation and
requires that only the current mesh reaches the result channel. Full checks
passed 629 tests and strict Clippy. The offscreen Kiln preview was inspected;
live-window placement-to-display latency has not been measured, so this fixes
the demonstrated backlog mechanism without claiming the user's exact stall
has been fully reproduced.

The terrain-only `perf 300 6` check retained 17,292,744 mesh bytes and 88,026
visible triangles. Compared with the previous recorded Kiln run, setup was
1723.9 → 1699.8 ms, steady CPU median 0.321 → 0.313 ms, and GPU median
0.293 → 0.282 ms. That benchmark does not exercise this client work queue.

### Live-trace follow-up: motion updates incorrectly forced resync

The user's next live reproduction still showed delay. Its trace exposed repeated
chunk resync requests roughly every 33–50 ms, despite prompt placement receipts
(one observed placement was acknowledged in 20 ms). A stronger real-listener
test, feeding movement and Kiln updates through the actual client replica
assembler, reproduced rejection of a valid player update: entity revision 1
stayed at 1 while motion revision advanced from 1 to 2.

The client had compared the whole entity for equality whenever the general
revision was unchanged, overlooking the independent motion revision. It now
accepts newer mobile positions with unchanged general revision and payload,
while rejecting regressing revisions and conflicting equal-revision data.
Resync replaces entire chunk snapshots and invalidates dependent lighting, so
this rejection loop can delay block display without stalling rendering.

The strengthened loopback regression now runs movement, idle/burning Kilns,
placement, the production replica assembler, and bounced lighting. Additional
client tests protect stale-motion and conflicting-payload rejection. All 630
tests, formatting, and strict Clippy pass. A new live confirmation is still
needed to establish that this resolves the user's full symptom.

`BLOXGLOOM_TRACE_EDITS=1 cargo run --release` enables timestamped client edit,
resync, worker, and upload diagnostics for that confirmation. It is off by
default and does not change scheduling or rendering rules.
