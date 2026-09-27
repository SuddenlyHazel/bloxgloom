# Execution foundation: next implementation slices

## Goal

Next architecture proposal: [built-in/mod capability parity](MODDING-SURFACE-PLAN.md).
It defines the complete host surface needed for future mod-created gameplay,
including lifecycle, client interaction/presentation, and an external-boundary
proof. It is a proposal; the implementation slices below retain their historical status.

Finish the execution foundation before adding more infrastructure. The slices below are complete; the user chose a real gameplay feature as the first consumer instead of the originally proposed independent extension crate. Fire spread and its migration remain parked.

The execution contract we are working toward:

> The coordinator selects work and owns state. Workers compute from immutable snapshots. Transactions commit atomically. Publication exposes only confirmed state. Capacity pressure delays work without silently losing it.

## Status as of 2026-09-26

- **Done:** registered inventory views/screens and independent container
  persistence. External fixture opens from either block and transfers/reopens
  after restart through the real client/listener. **652 tests passed**, formatting
  and strict Clippy passed, and release UI previews were inspected. Default save is `world-v11`;
  see [registered inventories](docs/REGISTERED-INVENTORIES.md).

- **Done:** first public storage-lifecycle surface and external fixture. Chest
  uses the registered lifecycle path; a separately compiled nine-slot/two-cell
  storage block exercises cross-chunk persistence, refunds, conflicts, retries,
  and the real listener. **647 tests passed**. See [host lifecycle](docs/HOST-LIFECYCLE.md)
  and the [remaining modding surface plan](MODDING-SURFACE-PLAN.md).

- **Done:** worker-based entity policy dispatch and initial regression tests (`ace635b`, `9b229f1`).
- **Done:** direct review of the worker slice, including its production call path.
- **Done:** review correction for captured terrain dependencies during transaction admission (`e6be3ed`), including single-drop and batched-motion regression coverage. The worker slice is complete with the verification limits noted below.
- **Done:** slice 1 and direct review of its scheduling corrections, with the capacity and atomic-wave limitations documented below.
- **Done:** slice 2 and direct review of bounded suspended-entity rechecks (`f05f2c5`).
- **Done:** slice 3 and direct review of entity conflict/publication separation (`2ca8e12`).
- **Done:** slice 4 and direct review of shared commit admission and ordered barriers (`273a70d`).
- **Done:** slice 5A, off-thread publication, reviewed with synchronous bounded worker barriers.
- **Done:** slice 5B, streaming checkpoints and bounded capture, reviewed with the parked-fire and rotation-latency limits below.
- **Deferred by user:** independent extension crate below; validate the foundation with real gameplay first.
- **Done:** first wandering NPC, Mossbun (`b6b478a`), registered within the repository. Parent reviewed the server/client paths, inspected the rendered preview and independently ran **12 Mossbun-filtered tests**, all passing. Coder reported **609 full-suite tests passed** and clean check/fmt/strict Clippy. This demonstrates the in-repository entity path, not external-crate API completeness.
- **Done:** creature movement follow-up, implemented directly without delegation: shared body locomotion and accelerating gravity, bounded local ground A*, persisted AI deadlines/waypoints, client actor interpolation and procedural animation. **621 tests passed**, strict Clippy passed, and the production animation preview frames were inspected. Schema 2 uses `world-v7`; see [capabilities and limits](docs/CREATURE-MOVEMENT.md). No live game window was observed.
- **Parked:** fire spread and its migration.
- **Done:** passive 27-slot Chest using the same storage codec, inventory ports, and interactions as Hopper. Verified Chest/Hopper persistence and conservation, collection retries, and the full Kiln/Hopper/Chest chain over the real listener. **640 tests passed**, strict Clippy passed, and compact/enlarged UI previews were inspected. New content uses `world-v10`; see [Chest details](docs/CHEST.md).
- **Done:** Hopper inventory automation: registered public inventory capabilities, atomic pushes/pulls, three-slot controls, and a feed/cook/collect chain. Verified blocked outputs, stale destination conflicts, restart/break conservation, and moving-player placement over the real listener. **635 tests passed**, with strict Clippy and inspected previews. New catalog content uses `world-v9`; see [Hopper details](docs/HOPPER.md).
- **Done:** playable Kiln workstation on the shared anchored-entity path: right-click UI, all-inventory-slot transfers, recipe-relative progress, public contents summary, and masonry/lit art. Verified place/load/cook/restart/collect/retry/break across a seam; **627 tests passed** and strict Clippy passed. Inspected desktop/compact UI and lit/unlit previews. See [Kiln details](docs/KILN.md). Schema 2 changes the default test save to `world-v8`.

## First: review the current worker slice — done

Completed implementation and review evidence:

- [x] The live drop/entity tick path invokes bounded workers through `stage_motion_batch` → `entity_dispatch::plan_motion` → `PhaseExecutor`.
- [x] Workers receive owned immutable inputs; transaction construction and authoritative application remain on the coordinator.
- [x] Worker results are restored to request order before transaction construction. A two-entity synthetic test compares one and multiple workers with an ordering gate.
- [x] Existing motion batching and receipt draining are retained; no alternate authoritative commit path was introduced.
- [x] Focused tests cover stale-capture rejection/retry and worker-panic retry.
- [x] Full test suite passed after the correction: **526 passed, 0 failed** (coder verification). Formatting check passed. Existing behavioural assertions were retained.

Completed correction and verification:

- [x] Preserve every captured terrain chunk as a transaction read dependency through admission and motion batching. Outstanding conflicting terrain edits defer dependent work for replanning. The parent inspected the correction and regression diff.
- [x] Add and pass a regression that holds a real terrain-edit receipt, exercises the durable coordinator with one and two due drops, verifies no stale or partial motion stages, then checks successful retry against updated terrain and preservation of read keys through batch combination.

Verification limits: the new synthetic ordering test covers one step, not the broader multi-tick drop/item/publication equivalence requested for the slice. Full capacity/fairness guarantees remain slice 1 work. Strict Clippy failed with warnings including unchanged code; a clean lint result has not been established.

The parent performed the focused review directly. Necessary corrections remain part of this worker slice.

## 1. Scheduling and progress under capacity pressure

**Status: done — reviewed, with explicit limits below.**

- [x] Explicit neighbour-read declarations are wired into live entity capture; drop and kiln planners skip unused neighbour views (`2c35bb6`).
- [x] Active/due owner selection and entity wake/due admission changes are wired into production dispatch (`87a2ac6`).
- [x] Oversized entity views receive bounded transient retry backoff while persisted due entries remain intact (`355a709`). This is local rejection/backoff, not guaranteed progress for a permanently oversized neighbour-dependent policy.
- [x] Replace population-sized owner selection with bounded indexed traversal; successor/empty-owner checks and durable wake inspection also use bounded lookups (`c634b78`, `ca1bdb1`).
- [x] Rotate unavailable entity ticks out of admission while preserving persisted due eligibility. A production-path regression covers 256 distinct blocked entities followed by ready work (`c241155`).
- [x] Remove the transient first-wake cursor exception. The restart test preserves wake-delivery/no-duplicate assertions and explicitly changes the ordinary rotation expectation (`c634b78`).
- [x] Due-index feeding now uses up to the job allowance and removes fed entries from the transient unfed index, avoiding repeated feeding of the same ready owners (`cb45511`). Active and due work share one circular order, preventing recurring deadlines from resetting another lane's progress.
- [x] Registered handlers select `OwnerSchedule::Active` (the existing default) or `AtTick(t)` through the live patch/validation/write/journal/apply path. Deadlines must be after the producing tick; wakes may run scheduled owners early, and the handler selects its next schedule again. Existing due persistence is reused without format changes (`cb45511`).
- [x] Parent reviewed the final production diff and regression tests and independently ran all six runtime scheduling tests: **6 passed**. Coder reported **540 full-suite tests passed**, formatting and diff checks passed. Strict Clippy remains blocked by existing warnings.

Limits retained: genuinely oversized neighbour-dependent views are locally rejected/backed off, not made executable; atomic owner-wave rejection or deferral can delay otherwise healthy owners in that wave. Persisted deadlines and ordinary rotation survive restart, but exact transient ready-set/feed ordering is not guaranteed across restart. The scheduling API is wired through registered production dispatch and exercised by test registrations; built-in adapters do not yet emit scheduled owner patches. These are not claims of reliable sleeping-entity wakes (slice 2) or isolated per-owner commit orchestration (slice 4).

**Problem:** bounded execution protects memory and prevents global failure, but does not always guarantee local progress. Dense neighbour views can prevent drop motion; wake-prioritized work can compete with normal scheduling.

### Scope

- Give policies explicit read requirements. Physics-only drops should not need a collection of neighbouring entities.
- Distinguish temporary unavailability from a permanently oversized request.
- Preserve due work across deferral with deterministic, fair admission.
- Ensure continuous wakes cannot starve ordinary due work.

**Result:** a dense but valid region continues simulating, and an oversized or unavailable job does not repeatedly obstruct unrelated work.

This slice is complete.

## 2. Reliable wake and sleep semantics

**Status: done — reviewed, with bounded recheck semantics.**

- [x] Traced the crash window between durable terrain edits and transient entity notifications. Suspended records now populate a derived index rebuilt from persisted entities, so lost hints do not erase eligibility (`f05f2c5`).
- [x] Wired a bounded circular recheck lane into production interaction-barrier admission and existing worker/transaction dispatch. A fixed upper ID per pass prevents new sleepers from indefinitely delaying wraparound; scarce admission opportunities rotate among due work, hints, and sleepers.
- [x] Bounded and coalesced entity hints to 256; terrain hint discovery examines at most 16 owners × 16 entity references. Hints accelerate work rather than determine authoritative ownership or required eligibility.
- [x] Sleeping drops explicitly recheck their supporting layer. Unchanged support reaffirms without motion or WAL writes; missing support resumes existing fixed-step physics. This prevents periodic checks from making enclosed resting drops climb through the active sweep's inclusive starting layer.
- [x] Parent reviewed the implementation and independently ran all seven new sleep integration regressions: **7 passed**. Coverage includes real support harvest, lost notifications, durable edit before apply/restart, unavailable terrain and a chunk seam, full/scarce admission, duplicate hints, expiry/deletion, and new sleepers during a pass.
- [x] Coder reported **547 full-suite tests passed**, formatting and diff checks passed. Existing assertions were preserved; the prior query test gained the bounded-query argument. Strict Clippy remains blocked by existing warnings.

No save/wire format or world-version change. Recheck latency scales with sleeping population and admission opportunities; progress requires eventual capacity and satisfiable terrain capture. Cursors reset on restart while eligibility is reconstructed. Permanently oversized views, perpetual non-entity queue saturation, and endlessly repeated restarts are outside the progress guarantee. Suspension now means exclusion from ordinary due ticks, not exclusion from bounded re-evaluation. Fire and owner wake persistence remain unchanged.

**Problem:** saying effects are optional is insufficient if a sleeping entity has no other reason to run again.

### Scope

- Trace the lifecycle of sleeping entities, beginning with settled drops.
- Ensure removing support reliably makes the affected drop eligible for work, including across unload and restart.
- Choose the required mechanism from the actual dependency: durable dirty state, reliable scheduled wakes, or deterministic re-evaluation.
- Keep notifications separate from ownership-changing transactions.

**Result:** a settled drop cannot remain suspended forever because a wake was lost or deferred.

This may be small after slice 1, but it has a distinct behavioural claim.

This slice is complete.

## 3. Separate conflict revisions from publication ordering

**Status: done — reviewed, with conservative spatial dependencies.**

- [x] Removed the global entity watermark from conflict reservations. Entity record/motion, changed membership/budget pages, anchored cells, and allocator keys retain their actual preimages (`2ca8e12`).
- [x] Finalize the publication watermark at WAL admission from the last accepted transaction, including unapplied records. Rejected admission does not advance the frontier; receipt apply and checkpoint-mirror delivery retain admission order.
- [x] Neighbour and spatial reads carry searched pages (including empty pages) and member record/motion dependencies. Shared read reservations permit independent readers while fencing writers until the last reader applies. Actual state preimages are not rebased.
- [x] Wire dependencies into entity planning, interactions, pickup, drop merging, and block-edit occupancy checks. Preserve atomic ownership changes, full terrain fences, deterministic motion batches, and receipt-before-apply.
- [x] Parent reviewed the production diff and independently ran all seven new conflict regressions: **7 passed**. Coverage includes independent in-flight updates in one owner chunk, live coordinator pickups, overlapping transfers, negative spatial reads, shared terrain reservation lifetime, ordered receipts/publication, recovery before apply, and exact checkpoint-mirror agreement.
- [x] Coder reported **554 full-suite tests passed**, formatting and diff checks passed. Strict Clippy remains blocked by existing warnings. The stale-worker test now invalidates captured terrain rather than an unrelated global revision; another fixture initializes through WAL admission. Existing rejection/retry and behavioral assertions were retained.

No encoding or world-version change. Spatial dependencies remain conservative at page scope; real shared page/allocator changes can still serialize, and oversized dependency sets reject/defer rather than truncate. Session-player projections are validated at the coordinator decision boundary, not frozen until WAL completion. The public stream counter still includes transient player publication. Whole-store projection costs and general commit orchestration remain later work.

**Problem:** the global entity revision key makes independent entity mutations conflict. Motion batching addresses one symptom, but the underlying transaction model still limits independence.

### Scope

- Use entity, owner and chunk revisions for state a transaction actually depends on.
- Retain a separate, ordered publication sequence.
- Preserve atomic multi-entity transactions, spatial-index consistency and replay.
- Keep deterministic arbitration for genuinely overlapping operations.

**Result:** unrelated entity updates can be prepared and admitted independently without conflicting merely because both are entities.

This is the most delicate slice. Use a coder followed by a targeted reviewer pass for conflict detection and recovery.

The parent performed this review directly. This slice is complete.

## 4. Consolidate commit orchestration and make barriers explicit

**Status: done — reviewed, with specialized payload application retained.**

- [x] Entity actions and registered owner waves share durable admission, the 256 pending-commit bound, reservation accounting, and one ordered pending receipt queue (`273a70d`). Owner reads now take shared reservations, including captured owners not replaced by the wave.
- [x] One receipt/apply gate handles nonblocking polling and explicit `Through(admission_id)` / `AllStaged` barriers. Confirmed application releases reservations through a shared helper. Losing an owner admission handle no longer loses its accepted payload/receipt.
- [x] Owner phase barriers complete through their last accepted admission, including earlier entity/fire work. Motion retains its existing same-tick barrier. Ordinary non-motion actions and fire can still remain pending across ticks; all consumers see confirmed state only.
- [x] Rejected admission consumes no ID and withdraws staged wake capacity. Receipt failures preserve the confirmed prefix and quarantine the failed pending record/suffix behind fatal gating. Owner wake hints and cursor advancement publish after confirmation, with same-tick wake fences.
- [x] Parent reviewed production admission, receipt handling, owner apply and phase integration, then independently ran the lifecycle-filtered tests: **7 passed** (six new lifecycle regressions plus an existing entity lifecycle test). Coder reported **561 full-suite tests passed**, a separate production nonblocking TCP join/edit/restart test passed, and formatting/diff checks passed. Strict Clippy remains blocked by existing warnings.

No save/wire format or world-version change. Fire retains specialized batch admission and contiguous apply; startup owner seeding remains synchronous bootstrap work. Atomic owner-wave deferral remains, without per-owner isolation. Feature planners/apply, checkpoint work, and client projection remain specialized. Existing fixtures were mechanically adapted to the shared barrier, replacing the former sleep/retry helper; behavioral assertions were preserved.

**Problem:** entity actions, motion batches and owner waves currently have different stage, wait and apply arrangements.

### Scope

- Consolidate common reservation, receipt, failure and publication handling.
- Keep feature-specific planners separate.
- Document and enforce one logical-tick contract.
- Preserve coordinator ownership and WAL-before-visibility.

The recommended contract for now is that a durable boundary may delay completion of a logical step in wall-clock time. Networking remains independent. Do not introduce speculative simulation or rollback in this slice.

**Result:** new systems reuse the commit lifecycle instead of adding special-purpose drain loops.

Some consolidation may naturally land during slice 3. This task should cover only what remains.

The next implementation task is slice 5. Scope publication and checkpoint work as separate tasks if their paths are substantially different, and obtain the user's go-ahead first.

## 5. Off-thread publication and bounded checkpoint work

**Status: done — publication and checkpoint implementations reviewed, with explicit limits below.**

Execute as two sequential tasks with parent review of each:

- [x] **5A — Publication:** immutable committed effects, worker interest/projection and replication preparation, shared encoded chunk pages, bounded backpressure and stale-result/session ordering.
- [x] **5B — Checkpoints:** audit and bound the shared capture/serialization path, retaining ordered mirror/recovery correctness. Parked fire's private checkpoint path remains outside the streaming guarantee.

Checkpoint progress (`d36a944`):

- [x] Stream entity checkpoints from the fenced worker-owned mirror and journal bases from the ordered latest-values map, avoiding whole-generation output allocation and rotation-time population sorting.
- [x] Ordinary dirty-key dispatch uses a bounded 16-key circular selection; writer capacity is checked before snapshot copying. Serialization turns process at most 16 bounded entries, with 64 KiB write/checksum pieces.
- [x] Preserve atomic file publication and WAL generation coverage; exclusive startup recovery discards unpublished regular entity-checkpoint temporaries before validating published state and replaying WAL. Formats remain unchanged.
- [x] Finish review verification: parent inspected implementation and independently passed **44 checkpoint-filtered tests**. A concurrent rotation run hit the previously intermittent landed-drop completion assertion. Correction `9323b8e` replaces its rotation sleep/poll loop with bounded waits on actual entity-checkpoint and WAL-rotation receipts, retaining production validation and strengthening fence/completion assertions. Parent inspected the correction and independently reran the affected test: **passed**. No production stall was found; the historical blocking phase was not reproduced or established.

After the correction, coder reported **586 full-suite tests passed**, concurrent focused runs of **44 checkpoint and 12 rotation tests passed**, and formatting/diff checks passed. Strict Clippy remains blocked by existing diagnostics. Checkpoint commands still take O(population) total work on dedicated workers, with bounded serialization turns rather than interleaved executor jobs; rotation can hold admission closed for multiple ticks. Startup recovery remains population-sized, ordinary per-key values remain schema/transaction-bounded whole values, and parked fire's private complete-map checkpoint remains outside the streaming guarantee.

Publication progress:

- [x] Bounded chunk snapshot worker preparation and matching-publication shared messages/encoded bytes (`e9cddc1`). Existing codec workers encode shared frames once; per-client outbound accounting remains independent. Parent inspected the implementation and independently ran all eight snapshot regressions: **8 passed**.
- [x] Move remaining interest/subscription preparation, drop visibility, and committed player/entity delta grouping/projection onto workers (`8905b3d`). Subscription captures share immutable maps; incrementally maintained mobile pages avoid coordinator population cloning. The existing confirmed-effects queue supplies immutable ordered inputs; no additional retained cross-tick log was introduced.
- [x] Localize oversized snapshot failures and close accepted worker batches after capture/submission failure (`443771d`). Oversized snapshots disconnect affected sessions; oversized transaction expansion triggers complete subscription resnapshots without truncating transaction pieces. Reliable replies retain order.
- [x] Parent reviewed the continuation and independently ran **7 publication tests and 11 snapshot/streaming tests**, all passing. Coder reported **579 full-suite tests passed**, real nonblocking TCP snapshot/edit/restart coverage passed, formatting/diff checks passed; strict Clippy remains blocked by baseline diagnostics.

5A limits: publication uses synchronous bounded worker barriers, including per-effect fanout, not asynchronous cross-tick streaming. Sharing is batch-local. The coordinator still creates confirmed effects, maintains derived pages, captures bounded snapshot views/page handles, sorts client IDs, validates outputs, manages subscriptions/pins/sessions, and enqueues frames. Selected anchored footprints are still cloned during bounded snapshot capture. Conservative global expansion overflow resyncs all subscribed clients. No wire/save format changes. Complete checkpoint work is assessed separately in 5B.

For the snapshot unit, coder reported **570 full-suite tests passed**, strengthened real nonblocking TCP snapshot/delta/result ordering coverage, and passing formatting/diff checks. Strict Clippy remains blocked by existing warnings. A headless preview was inspected, but no release gameplay window was observed. Snapshot sharing is within matching publication groups, and the coordinator waits at a bounded publication barrier; this is not cross-tick asynchronous publication.

**Problem:** expensive work can still accumulate after simulation has finished.

### Scope

- Publish immutable committed revision records.
- Move interest projection and replication preparation onto workers.
- Share encoded chunk pages where useful.
- Verify that the complete checkpoint path uses bounded capture and serialization units, not merely that an individual WAL change is small.
- Preserve per-client ordering, revision continuity and backpressure.

**Result:** adding clients or resident entities does not force the coordinator to scan and serialize the whole relevant population each tick or checkpoint.

If publication and checkpointing touch substantially different paths, execute them as two scoped tasks rather than forcing one large diff.

## Then: build a real extension

**Status: deferred by user in favor of real gameplay.**

Slices 1–5 are reviewed. The first gameplay exercise is now Mossbun: admin spawning, worker-based wandering and terrain checks, WAL persistence/restart, registered client presentation and a procedural voxel model. Texture paging remains separately scoped and fire stays parked. The extension proposal below is retained for reference, not an authorized next task.

Complete missing startup registration hooks while building the independent extension crate, rather than designing every possible hook in advance.

Use it to add:

- A stateful block.
- A mobile entity.
- A cross-chunk anchored entity.
- An item with components.
- A system with persistent state and notification-driven scheduling.

This gives the foundation a real consumer and reveals where core edits are still required.

Texture paging remains a separate rendering task. It can follow or run independently once server ownership boundaries are stable. Fire remains deferred.

## Execution approach

- Use one coder at a time for the tightly coupled server slices.
- Use a reviewer when a second opinion materially helps: the current worker cutover, conflict/recovery changes, and commit consolidation.
- Do not let benchmarks, soak runs or performance targets drive this work. Follow applicable repository verification instructions for relevant changes.
- Use focused behavioural tests, followed by the repository checks appropriate to the change.
- Preserve existing behavioural assertions; explicitly report mechanical fixture changes.
- Require production call-path evidence before declaring a capability complete. Tested but unwired helpers do not constitute a completed runtime feature.
- Stop after each slice for the user's go-ahead.
