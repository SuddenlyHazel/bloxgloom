# Authoritative server simulation design

Status: architecture and migration record. The design below is the target contract; the implementation status here distinguishes shipped foundations from future game systems.

## Implementation status (2026-09-24)

The server now runs an authoritative 50 Hz coordinator, including with no clients connected. Socket threads use bounded channels; they do not own a gameplay lock. Input, durability, simulation, interaction, and publication execute in a registered, validated phase order. Player movement runs as independent jobs over revisioned, resident voxel views in a bounded worker pool; the coordinator applies their results in stable order. Drop motion, edits, pickups, inventories, checkpointing, and replication are coordinator-owned, with blocking chunk loads and journal sync on separate workers. Missing authoritative terrain defers work rather than treating procedural client fallback as state.

Durable changes use full-key, checksummed WAL transactions and only become visible after a sync receipt. Recovery validates checkpoints before replay; journal rotation is gated on checkpoint completion and bounds retained WAL size. The current action-receipt representation has a finite one-million-entry safety limit: admissions reject at that limit, and compact receipt storage is future work. Bounded queues and per-tick telemetry make overload and backpressure observable.

The scheduling, effect-routing, and transaction primitives are internal foundations, **not** a public mod API. Movement is parallel today; drop simulation and durable commits remain coordinator-owned. Fire, plant-growth propagation, cross-chunk block entities, and mod registration have not been implemented, so their boundary and performance gates below remain open. A server benchmark must measure the workloads that actually exist; it must not claim fire or growth coverage before those systems exist.

## Goals and constraints

- One authoritative, fixed-step timeline for all connected players and for a dedicated server with no players. The client may predict or animate, but cannot decide world, movement, inventory, or drop ownership.
- Parallel work without making thread timing a gameplay rule. A fire front, plant, drop, or structure behaves the same at a chunk or scheduling boundary as it does in the middle of a chunk.
- Fast local paths: active state only, bounded queues, no full-world copy, no filesystem or socket wait on a simulation worker. Optimize measured costs without relaxing correctness.
- Explicit cross-system and cross-owner effects. New systems and eventual mods can extend behavior without gaining unrestricted mutable access to the world.
- Durable edits and item transfers preserve finite inventories, the 128-item stack cap, and stable profile IDs. Existing `world-v4/` data is not silently migrated or reinterpreted.

The former migration baseline used one `Mutex<State>` shared by client stream threads and stopped advancing drops with no clients. It has been replaced by the coordinator described above; the remaining work is to expand owner-local execution and prove the new gameplay systems against the acceptance gates below.

## Clock, phases, and state visibility

The server has one logical tick number and a 20 ms fixed simulation step (50 Hz). The coordinator owns the clock and phase barriers, not all simulation work. It drains bounded incoming commands into tick `T`, gives them a stable order `(tick, connection ID, client sequence)`, and runs the phase graph below. Commands arriving after the input cut-off enter `T+1`. Server-generated effects carry their origin tick and a stable producer/sequence key.

| Phase | Reads | Writes or output |
| --- | --- | --- |
| 1. Input/authorization | Connected profiles, last acknowledged commands | Validated commands or explicit rejections |
| 2. Durable actions | Authoritative state and content definitions | Previously synced actions commit; new edits, inventory transfers, and structure transactions are validated/staged |
| 3. Simulation | State visible after phase 2, immutable neighbor views | Owner-local movement, falling drops, fire/growth/AI candidates; cross-owner effects |
| 4. Interaction/commit | Phase-3 results | Ordered pickups, damage, spread effects, transfers and conflict resolution |
| 5. Publish | Committed revisioned state | Replication deltas, checkpoint jobs, metrics |

Within a phase, jobs with disjoint write ownership can run concurrently. A phase barrier makes committed changes visible to the next phase. A system may not mutate a neighboring owner's state or publish a packet directly from a worker. Cross-owner effects are resolved at the prescribed barrier. An effect emitted by fire at `T` may ignite a neighboring cell's transient fire state during `T`'s commit phase; that new cell first *spreads* at `T+1`, regardless of which chunk or region contains it. A fire effect that permanently changes a block goes through the durable path before that block change is visible. This avoids an accidental extra tick at boundaries without pretending disk sync has zero latency.

Simulation uses `dt = 20 ms`, never measured elapsed wall time. The coordinator does not skip tick numbers to hide overload; it may run a bounded number of catch-up ticks, then records lag and applies backpressure/admission limits rather than silently changing physics speed. Render cadence and network snapshot cadence are independent. Timers have an explicit policy: in-world simulation delays use ticks; drop expiry, which survives restart and currently includes offline time, uses a persisted real-time deadline with monotonic elapsed time while running. Restart behavior is defined per timer, not inferred from frame timing.

## Ownership and parallel execution

Chunk coordinates, using Euclidean division for negative positions, are the basic block-state ownership keys. Entities have stable, generation-checked IDs and one current owner, normally the chunk containing their anchor. Player inventory is owned by the player profile, not by the chunk the player occupies. A multi-chunk object has one anchor owner plus a recorded footprint; it is not duplicated into independent entities per chunk.

A **region** is a batch of nearby ownership keys scheduled as one task. Its size is a measured runtime tuning parameter, not a save or gameplay identity. The scheduler may split a hot batch or combine quiet chunks without changing effects or rules. It only schedules active/resident work: sleeping drops, empty chunks, and distant inactive entities do not consume a per-tick scan. Systems can partition by chunk, entity batch, or profile, provided they declare the same ownership and effect contract.

Each system registers before the server starts with a stable name, phase, read dependencies, write ownership, and maximum neighbor/footprint requirements. The phase graph is checked at startup: conflicting writes need an explicit order or transaction, and dependency cycles fail startup. The default API gives a job immutable, revisioned views of required chunks and exclusive access to its owned state, plus a bounded buffer for typed effects. Immutable views share unchanged chunk storage; the server does not clone an endless world each tick. Work that needs a missing authoritative chunk waits for a load/generation result or rejects the command cleanly. Procedural fallback is never accepted as authoritative edited state.

The implementation can begin with a fixed worker pool and explicit phase job batches, then tune work stealing and region sizes from measurements. Avoid one OS thread per player, chunk, system, or mod. A concentrated group of 16 players must not be forced through an arbitrarily large single-owner region: keep ownership fine-grained and permit independent entity calculations to fan out over immutable terrain, with ordered owner commits.

## Effects, transfers, and multi-owner transactions

There are two intentional cross-owner paths:

1. **Effects** express independent changes such as `Ignite(cell)`, `Grow(cell)`, `WakeDrop(id)`, or `Damage(entity, amount)`. Producers never apply them remotely. At the phase barrier, effects are grouped by destination, sorted by stable key, validated against the then-visible state, deduplicated where the effect defines that rule, and applied by the destination owner. Local and remote effects follow the same semantics. Limits on effects per source/tick and a defined overflow policy prevent runaway fire or mod code from consuming unbounded memory; overflow must be observable, not silently truncate gameplay.
2. **Transactions** express all-or-nothing changes: a tree/structure spanning chunks, a block entity with a cross-chunk footprint, a pickup moving an item from world ownership to a profile inventory, or placement consuming inventory while changing a block. A transaction declares its complete read/write key set before commit. Validation includes revisions, reach/permissions, collision, content IDs, inventory capacity, and footprint conflicts. Keys are acquired/processed in canonical order; either all changes commit at one barrier or none do. The scheduler may serialize rare cross-owner commits while ordinary owner-local simulation remains parallel. No half-placed structure or duplicated item may be visible or durable.

A cross-chunk block entity's footprint is indexed in every occupied chunk, but state and lifecycle live only at its anchor owner. Reads from other chunks resolve through the index; edits that touch the footprint must go through its transaction policy. When an entity or drop crosses an ownership boundary, an ordered transfer moves its ID and state at a barrier—never two live copies. Border snapshots/halos are read-only and revisioned. A region boundary is never a wall, a source of duplicate updates, or an automatic tick of latency.

For fire and growth, the rules specify propagation cadence, conflict priority, and unloaded-chunk behavior explicitly. Scheduled frontier work is stored with the destination chunk so unloading and reloading cannot erase it. A bounded local wave can cross several chunks in one phase only if the system declares and budgets that behavior; the normal cellular spread rule advances one propagation step per tick everywhere.

## Networking and client presentation

Socket readers decode and validate frame bounds off the simulation workers, then enqueue typed commands into bounded per-connection queues. The simulation checks authority and game rules again; a packet is never a direct world mutation. Repeated movement commands carry sequence numbers, and duplicate/replayed commands cannot repeat edits or item transfers. Queue overflow has a visible rejection/disconnect policy, not unbounded memory growth.

Replication consumes committed revisioned state after the tick. Interest management, chunk snapshots, block deltas, entity/drop state, and inventory acknowledgments have independent bandwidth budgets. A snapshot may run at 10–20 Hz while the server simulates at 50 Hz; important discrete events are not lost merely because a snapshot was skipped. Clients interpolate moving drops/entities and may predict their own movement, but reconcile to server acknowledgments. Client-side pop, hover, spin, and pickup flight never govern ownership. Stale chunk/lighting/mesh jobs are rejected using revisions, and affected seams are refreshed after edits.

## Persistence and restart semantics

The existing chunk-override, inventory, drop, and `content.map` formats remain valid until an explicit versioned migration is written and tested. In particular, numeric content IDs retain their mapped namespaced meanings. The proposed simulation must not treat multiple independent file writes as one atomic inventory/world transaction.

Durable gameplay transactions use versioned, checksummed write-ahead records containing the tick, unique transaction ID, full affected keys, and replayable after-values. A dedicated I/O worker batches writes and syncs them. The simulation tracks pending reservations and does not apply or acknowledge a durable transaction as committed until the durability result is known; failure rejects the action or stops the coordinator. Edits, placement, harvest, pickup, and inventory transfers use this path. Per-key checkpoints anchor journal rotation. Cross-chunk structures and persistent fire/growth effects must use the same path when implemented; the presence of a transaction primitive alone does not validate those future behaviors.

High-frequency positions need not fsync every 20 ms. They use revisioned periodic checkpoints plus a documented restart policy (for example, a falling drop resumes from its last checkpoint, while its item identity/count never rolls back across a committed pickup). Backpressure or disk failure cannot be allowed to create items; persistent actions pause or reject rather than commit without their durability guarantee. Save, compaction, and chunk generation run off simulation workers. A loaded result is installed only if its requested version/epoch still matches.

## Content and future mods

The existing startup `content::Catalog` remains immutable during play, with namespaced block/item definitions and a persisted `content.map`. Mods register content and system hooks before world load and multiplayer handshake. Catalog fingerprinting continues to reject clients that would interpret IDs differently. Public mod support also requires explicit migrations for today's `u8` block/item IDs and texture-layer limit; this design does not pretend those limits are gone.

A mod hook receives a constrained view and emits typed effects/transactions through the same phase and ownership rules as built-in systems. It declares dependencies, affected domains, neighbor radius, state version, and resource budget. No hook receives raw mutable world access, socket or filesystem handles, or the ability to spawn unbounded work. Data-driven rules and a sandboxed runtime are possible implementations; arbitrary native plugins cannot be promised safe or deterministic. Mod state persists by namespaced, versioned records with a defined missing-mod policy before public support.

## Migration and acceptance gates

1. Introduce tick numbers, a dedicated coordinator, bounded command queues, and separate per-client replication loops. Move current drop stepping and expiry off client stream threads. Prove the simulation continues with zero clients and that 1 versus 16 connections does not change drop trajectories.
2. Move movement, edits, inventory, and pickups into ordered phases without changing their externally observed rules. Replace the global `Mutex<State>` hot path with owner-local state and revisioned read views. Preserve current saves and wire behavior while this migration is internal.
3. Add parallel batches and the typed cross-owner effect path. Test fire-like propagation and supported-drop wakeups at cell center, chunk edge, negative-coordinate edge, and region edge; results and tick numbers must match across worker counts and scheduling orders.
4. Add footprint transactions and durable atomic records before shipping cross-chunk block entities or multi-key mod actions. Fault-inject I/O and process restart between prepare, sync, commit, snapshot, and compaction; verify no partial structure, lost inventory, or duplicated item.
5. Expose a constrained startup system-registration API only after the built-in fire/growth/structure examples prove the contract. Defer a mod package format and script engine until those semantics and ID migrations are settled.

Benchmarks must report tick p50/p95/p99, worst-case backlog, worker utilization, barrier wait, active chunks/entities, chunk-load latency, durable-action latency, and replication bytes/queue depth. Test both 16 players clustered in one area and 16 spread across regions, with active fire/growth/drops and edits. A release target is sustained 50 Hz without growing queues on the target host; if a scene misses that budget, report which phase and workload is responsible. The client 60 FPS GPU target is separate and must be measured separately. No new system is accepted solely because an isolated unit test or unloaded benchmark is fast.
