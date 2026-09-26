# Execution foundation: next implementation slices

## Goal

Finish the execution foundation before adding more infrastructure. After the current worker-based entity-planning slice, follow the focused slices below, then build an independent extension crate. Fire spread and its migration remain parked.

The execution contract we are working toward:

> The coordinator selects work and owns state. Workers compute from immutable snapshots. Transactions commit atomically. Publication exposes only confirmed state. Capacity pressure delays work without silently losing it.

## Status as of 2026-09-26

- **Done:** worker-based entity policy dispatch and initial regression tests (`ace635b`, `9b229f1`).
- **Done:** direct review of the worker slice, including its production call path.
- **Done:** review correction for captured terrain dependencies during transaction admission (`e6be3ed`), including single-drop and batched-motion regression coverage. The worker slice is complete with the verification limits noted below.
- **Done:** slice 1 and direct review of its scheduling corrections, with the capacity and atomic-wave limitations documented below.
- **Pending:** slices 2–5 and the independent extension crate below.
- **Parked:** fire spread and its migration.

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

This slice is complete. The next implementation task is slice 2, subject to the user's go-ahead.

## 2. Reliable wake and sleep semantics

**Status: pending.**

**Problem:** saying effects are optional is insufficient if a sleeping entity has no other reason to run again.

### Scope

- Trace the lifecycle of sleeping entities, beginning with settled drops.
- Ensure removing support reliably makes the affected drop eligible for work, including across unload and restart.
- Choose the required mechanism from the actual dependency: durable dirty state, reliable scheduled wakes, or deterministic re-evaluation.
- Keep notifications separate from ownership-changing transactions.

**Result:** a settled drop cannot remain suspended forever because a wake was lost or deferred.

This may be small after slice 1, but it has a distinct behavioural claim.

## 3. Separate conflict revisions from publication ordering

**Status: pending.**

**Problem:** the global entity revision key makes independent entity mutations conflict. Motion batching addresses one symptom, but the underlying transaction model still limits independence.

### Scope

- Use entity, owner and chunk revisions for state a transaction actually depends on.
- Retain a separate, ordered publication sequence.
- Preserve atomic multi-entity transactions, spatial-index consistency and replay.
- Keep deterministic arbitration for genuinely overlapping operations.

**Result:** unrelated entity updates can be prepared and admitted independently without conflicting merely because both are entities.

This is the most delicate slice. Use a coder followed by a targeted reviewer pass for conflict detection and recovery.

## 4. Consolidate commit orchestration and make barriers explicit

**Status: pending.**

**Problem:** entity actions, motion batches and owner waves currently have different stage, wait and apply arrangements.

### Scope

- Consolidate common reservation, receipt, failure and publication handling.
- Keep feature-specific planners separate.
- Document and enforce one logical-tick contract.
- Preserve coordinator ownership and WAL-before-visibility.

The recommended contract for now is that a durable boundary may delay completion of a logical step in wall-clock time. Networking remains independent. Do not introduce speculative simulation or rollback in this slice.

**Result:** new systems reuse the commit lifecycle instead of adding special-purpose drain loops.

Some consolidation may naturally land during slice 3. This task should cover only what remains.

## 5. Off-thread publication and bounded checkpoint work

**Status: pending.**

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

**Status: pending.**

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
