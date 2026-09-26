# Execution foundation: next implementation slices

## Goal

Finish the execution foundation before adding more infrastructure. After the current worker-based entity-planning slice, follow the focused slices below, then build an independent extension crate. Fire spread and its migration remain parked.

The execution contract we are working toward:

> The coordinator selects work and owns state. Workers compute from immutable snapshots. Transactions commit atomically. Publication exposes only confirmed state. Capacity pressure delays work without silently losing it.

## First: review the current worker slice

Before assigning more implementation work, verify:

- The live drop/entity tick path actually invokes workers.
- Completion order does not change plan or commit order.
- Physics cadence and atomic pickup/transfer behaviour survive.
- Missing terrain, stale results and capacity pressure preserve retry eligibility.
- No alternate commit path was introduced.

Use a focused reviewer pass because this change crosses scheduling, physics and durability. Necessary corrections belong to the current slice.

## 1. Scheduling and progress under capacity pressure

**Problem:** bounded execution protects memory and prevents global failure, but does not always guarantee local progress. Dense neighbour views can prevent drop motion; wake-prioritized work can compete with normal scheduling.

### Scope

- Give policies explicit read requirements. Physics-only drops should not need a collection of neighbouring entities.
- Distinguish temporary unavailability from a permanently oversized request.
- Preserve due work across deferral with deterministic, fair admission.
- Ensure continuous wakes cannot starve ordinary due work.

**Result:** a dense but valid region continues simulating, and an oversized or unavailable job does not repeatedly obstruct unrelated work.

This is the recommended next implementation task after reviewing the current worker slice.

## 2. Reliable wake and sleep semantics

**Problem:** saying effects are optional is insufficient if a sleeping entity has no other reason to run again.

### Scope

- Trace the lifecycle of sleeping entities, beginning with settled drops.
- Ensure removing support reliably makes the affected drop eligible for work, including across unload and restart.
- Choose the required mechanism from the actual dependency: durable dirty state, reliable scheduled wakes, or deterministic re-evaluation.
- Keep notifications separate from ownership-changing transactions.

**Result:** a settled drop cannot remain suspended forever because a wake was lost or deferred.

This may be small after slice 1, but it has a distinct behavioural claim.

## 3. Separate conflict revisions from publication ordering

**Problem:** the global entity revision key makes independent entity mutations conflict. Motion batching addresses one symptom, but the underlying transaction model still limits independence.

### Scope

- Use entity, owner and chunk revisions for state a transaction actually depends on.
- Retain a separate, ordered publication sequence.
- Preserve atomic multi-entity transactions, spatial-index consistency and replay.
- Keep deterministic arbitration for genuinely overlapping operations.

**Result:** unrelated entity updates can be prepared and admitted independently without conflicting merely because both are entities.

This is the most delicate slice. Use a coder followed by a targeted reviewer pass for conflict detection and recovery.

## 4. Consolidate commit orchestration and make barriers explicit

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
