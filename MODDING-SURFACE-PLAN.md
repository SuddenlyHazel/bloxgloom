# Plan: built-in/mod capability parity

Status: implementation in progress. The public crate exposes the completed
slices below; the remaining internal interfaces are not yet a supported mod API.
Latest integration: content/assets, anchored lifecycle, component-aware machines,
registered actions/UI, and persistent owner-local systems are merged into main. This document
tracks host capability parity; an installable mod loader is a separate milestone.

## Implementation status

### Status tracker

**Planned** = remaining work, not started. **In progress** = partially implemented
with gaps still open; it does not imply an implementation task is currently running.
**Done** = the stated scope is implemented and verified. **Deferred** = deliberately
parked or assigned to a later milestone. A completed slice does not imply full
parity for its broader category.

| Surface / deliverable | Status | Scope completed or work remaining |
| --- | --- | --- |
| Public host API boundary | Done | Public crate, extension registration, and independent Rust fixture package |
| Passive storage lifecycle | Done | Chest and TallStore placement, persistence, footprint removal, and refunds |
| Registered inventory screens | Done | Shared discovery, layout, slot access, status fields, and independent 1–54-slot storage |
| Ground-creature behavior and presentation | Done | Mossbun migration and Copperling proof: behavior, sensing, host movement, interaction, lifecycle, cuboid models, and animation |
| Inventory-machine slice | Done | Kiln/Hopper migration and Crusher proof: scheduling, private bytes, recipes/fuels, filters, named/sided ports, recovery, and refunds |
| Content and asset registration parity | Done | Legal states, standalone items/component schemas, PNG textures, existing geometry/collision/selection choices, lighting/material properties, tags, and registered HUD bitmap art; Copper Lamp/Reed/Etched Chip proof |
| Registration and composition parity | Done | Deterministic bundled registration, exact-version dependencies, capability validation, frozen identities, references, and compatibility fingerprints |
| General block / anchored-entity lifecycle | Done | Bounded own-state initialization/codecs/projections/use, neighbor/support reactions, costs/refunds, and atomic footprint invalidation; SignalPost proof |
| Inventory / process parity | Done | Exact slot/component-equivalence selectors, component-aware recipes/fuels/output policies, schema validation, conserving transfers and remap recovery |
| General item use, harvest, and loot | Planned | Registered item recipe action is implemented; general harvest/drop contracts and built-in loot migration remain |
| General interaction actions and UI composition | Done | Item/empty-space/block/entity discovery, fenced durable dispatch, bounded label/button/tooltip panels, stock-client integration and external knapping proof |
| Persistent owner-local system slice | Done | Public codecs, bounded persistent bytes, schedules/seeds, region-clock fixture and real-listener restart; world reads/effects remain below |
| World-system API and growth/support migration | Planned | Expose internal owner/system scheduling through public bounded read, effect, persistence, and wake contracts |
| Fire migration | Deferred | Parked pending explicit authorization; still a blocker for full capability parity |
| World-generation API | Planned | Deterministic terrain/vegetation contributions, ordering, bounded output, and seam ownership |
| Player rules, commands, and bindings | Planned | Audit and expose supported movement/spawn/player-rule hooks and registered commands/actions/bindings |
| World-drop gameplay and presentation parity | Planned | Move remaining drop policies and presentation capabilities onto accessible contracts |
| External cross-category integration proof | Done | Independently compiled TallStore, Copperling, Crusher, SignalPost, Copper Lamp/Reed/Etched Chip, knapping action and persistent region clock; broader world-effect proof belongs to planned world-system work |
| Built-in call-path audit | Done | Category inventory and integration follow-up in docs/MODDING-PARITY-AUDIT.md; remaining privileged paths identified explicitly |
| Close remaining full-parity audit blockers | Planned | World systems/generation, harvest/loot, player rules/commands/bindings and world-drop policies; fire remains deferred |
| Runtime adapter and user-installable mod loader | Deferred | Separate milestone: select runtime/language, then package/dependency handling, discovery/loading, and isolation |
| Distribution and hot reload | Deferred | Later loader/product decisions; not part of current host-surface completion |
| Player-response path hardening | In progress | Approved focused fairness/latency pass, implemented and reviewed by the main agent only; see current task below |

### Current task: player-response path hardening

User approved implementation and personal review, **no subagents**. Complete this
before adding more capabilities. Keep the solution small: explicit admission
fairness, bounded player/simulation work, two client worker priorities (immediate
edited geometry/seams vs background), prompt committed-update delivery, and a
real-path mixed-load responsiveness/restart regression. Preserve authoritative
movement, per-profile command order, WAL ordering, exact inventory/anchored fences,
and stale mesh rejection. Avoid a configurable scheduler or extra global waits.
Measure request → commit → client update → mesh/display; use deterministic
progress assertions and repeatable release latency measurements, not fragile CI
millisecond thresholds. Verify simulation progresses under sustained player work.

Completed follow-ups (do not lose these during compaction):
- `8ba1dda`: all implementation branches integrated; public HUD art, component
  schema-safe processing, world-v14 defaults; 695 tests, Clippy/fmt, previews.
- `ecb07ee`: reproduced edit starvation with one paused and one moving Copperling;
  draining motion before commands releases terrain reservations. Real-listener
  break/place/restart regression passes. This is a tactical fix: the approved
  hardening pass must replace timing-dependent fairness, not add more barriers.
- `36e177b`: directly edited chunk mesh lane; mobile own-state use accepts older
  movement-frame revisions while checking stable identity/current reach/sight,
  rejecting zero/future revisions and retaining the actor inventory fence. Exact
  client revisions remain required for anchored/container requests. Successful
  use displays "Interaction applied". Delayed-click regression advances three
  creature revisions before dispatch. Copperling click toggles patrol/rest.
- Copper Lamp's unlit orange/black checker faces are deliberate fixture PNG art,
  not a missing texture; documented in REGISTERED-CONTENT.md.

Latest follow-up verification: 695 tests plus strict Clippy/fmt passed. Measured
loopback edit confirmations were 40–70 ms; the user's full display delay has not
been reproduced. Bounced lighting is enabled in the user's saved client settings.
Latest headless steady CPU/GPU: voxel .308/.290 ms, bounced .351/.296 ms; setup
1683.4/2045.5 ms, mesh bytes 17,292,744/17,500,968 unchanged. Headless perf does not
measure live edit response. Existing `BLOXGLOOM_TRACE_EDITS` traces client stages.

- **Combined integration verified:** 695 workspace/all-feature tests passed,
  including real nonblocking-listener extension/restart tests; strict workspace
  Clippy passed. Default saves now use **`world-v14` / `world-v14-fixture`**.
  Registered content/action previews were inspected. Integrated terrain benchmark:
  setup 1723.9 ms; 17,292,744 mesh bytes (unchanged); steady CPU/GPU medians
  0.315/0.286 ms. Content-branch baseline was 1709.2 ms setup and
  0.301/0.292 ms CPU/GPU; these small differences do not establish a regression.
  See [content](docs/REGISTERED-CONTENT.md), [actions](docs/REGISTERED-ACTIONS.md),
  [anchored behaviors](docs/ANCHORED-BEHAVIORS.md), and
  [owner systems](docs/REGISTERED-SYSTEMS.md) for supported bounds.

- **Registered machine slice implemented:** public scheduled behavior, bounded
  private bytes, registered footprints, item filters, recipes/fuels, named/sided
  automation ports, and host-owned inventory/process projections. Production Kiln
  and Hopper use the generic adapter; their older implementations are test-only
  regression references. The independent Crusher fixture exercises manual input,
  automated feed/extraction, processing, restart, and refunds over the real listener.
   Default save at that slice: **`world-v13`**. See [registered machines](docs/REGISTERED-MACHINES.md)
   for supported contracts.
  **659 workspace/all-feature tests passed**, with clean formatting and strict Clippy;
  the release shared-inventory preview was inspected.

- **Dynamic entity slice implemented:** public private-state codecs, bounded
  projections, immutable behavior/sensing contexts, host locomotion/navigation,
  scheduling, atomic spawn/self-removal effects, targeting, cuboid models, and
  procedural animation declarations. Mossbun uses these contracts. The separately
  compiled Copperling fixture spawns, patrols, pauses on interaction, and recovers
  through the real listener/client paths. **655 tests passed**, strict Clippy and
   formatting passed, and release previews were inspected. The default save
   at that slice was **`world-v12`**. See [dynamic entities](docs/DYNAMIC-ENTITIES.md).

- **Registered inventory slice implemented:** shared inventory-view schema,
  descriptor-based opening/layout/slot access/status widgets, independent bounded
  container snapshots, and a fully usable external fixture through the real client
  and listener. Chest, Hopper, and Kiln use the same screen contracts. Default save
   at that slice was `world-v11`. **652 tests passed**, with clean formatting and strict Clippy;
  see [registered inventories](docs/REGISTERED-INVENTORIES.md).

- **First lifecycle slice implemented:** dependency-free public API crate,
  declarative storage lifecycle registration, Chest migration, and a separate
  nine-slot/two-block extension fixture. Verified production transactions,
  restart/refund conservation, conflicts/retries, and real-listener replication;
  **647 tests passed**. See [implementation and limits](docs/HOST-LIFECYCLE.md).
- Storage, inventory screens, creatures, machines, bounded anchored callbacks,
  registered content, actions and composed controls are exposed. Owner systems
  currently mutate only their own persistent bytes. The broader world, player,
  harvest and drop capabilities below remain open.

## Remaining work and next sequence

The foundation is established: authoritative simulation, worker scheduling,
atomic transactions, persistence, and committed replication are reusable. The
remaining work primarily exposes capabilities, migrates built-ins, and proves
external access. Completing a creature or inventory-machine slice does not mark
the entire entity or block-entity category complete.

| Workstream | Already available | Work remaining | Rough size |
| --- | --- | --- | --- |
| Items and loot | Registered actions, component-aware processing, exact selectors, durable inventory recipes | General harvest/drop rules and builtin loot migration | Medium |
| World systems | Public persistent owner-local scheduling/codecs and region-clock proof | Public bounded world read/effect/wake contracts and support migration; fire remains parked pending explicit authorization | Large |
| World generation | Existing internal terrain and vegetation generation | Public deterministic contributions, ordering, bounded output, and seam ownership; built-in consumers and external proof | Medium–large |
| Player and presentation | Ground creatures, registered inventory/action UI and item art | Player movement/spawn/rule hooks, commands/bindings and world-drop gameplay/presentation | Medium–large |
| Final parity closure | Cross-category fixtures and call-path audit complete | Close remaining privileged paths and prove world effects/generation/player/drop extensions | Medium |

Recommended next sequence:

1. Harvest/loot contracts and builtin migration.
2. World-system reads/effects and support migration, with fire explicitly scheduled.
3. World-generation contributions and player/drop hooks.
4. Verify remaining audit blockers through independent extensions.

The remaining work is substantial; no percentage or delivery estimate is implied
by the completed slices. The capability
checklist and completion rules below determine completion, not that estimate.

### Separate loader/runtime milestone

Full host parity means external content can reproduce existing built-in gameplay
through supported contracts. User-installable mods additionally require choosing
a runtime/language and implementing its adapter, package/dependency handling,
discovery/loading, and appropriate isolation. The current Rust development fixture
proves API accessibility; it does not provide a dynamic loader. Runtime selection,
distribution, and hot reload are not counted as completed host-surface work.

## Goal and completion rule

**If a gameplay feature exists in the game, an extension must be able to create
another feature with the same capabilities through supported registration and
runtime interfaces, without editing engine dispatch code.**

This includes blocks, items, entities, block entities, inventories, machines,
creatures, world simulation, world generation, interactions, and their client
presentation. Built-ins must consume the same interfaces. An internal shortcut
available only to a built-in is an API gap, even if similar code can be copied.

Parity means access to engine-supported capabilities, not unlimited access to
engine internals. A custom creature can choose behavior and use motion services;
it cannot bypass authoritative collision or commit unjournaled inventory edits.
New engine capabilities must ship with an extension surface and a built-in
consumer, rather than accumulating private exceptions.

## Adopted direction

1. Adopt capability parity as an architecture requirement.
2. Build a runtime-neutral host contract; choose scripting, WASM, native modules,
   package distribution, and loading mechanics separately.
3. Move built-ins onto that contract as it is implemented. Avoid maintaining a
   second, more powerful built-in path.
4. Validate the public boundary with a small separately compiled Rust extension
   fixture. This proves visibility and capability access, not a native-plugin ABI
   commitment. This proof already exists for containers, creatures, and machines;
   extend it to the remaining categories as their contracts are implemented.
5. Keep the current server authority, worker model, journal, replication, and
   startup-frozen registrations. This is completion of their surfaces, not a
   replacement execution architecture.

## Current foundation and actual gaps

Existing reusable pieces include the content catalog, registered entity codecs
and policies, worker inputs, scheduled entity/owner work, atomic cross-entity
transfers, journal/checkpoint recovery, and confirmed-state replication.

Those pieces are not yet a complete external contract. For example:

- `server/block_actions.rs` has hook registration, but its context and results
  expose internal server/client/store/commit types.
- `server/durable/actions/workstation.rs` now assembles shared placement/removal
  transactions; machine registrations supply payloads, cells, and refunds.
- Inventory views and storage encoding are now generic and independent of the
  player backpack. Machine recipes, filters, and automation are now registered;
  arbitrary component-aware process predicates remain a future capability.
- Mobile entity targeting, cuboid models, and the existing procedural creature
  animation capabilities are now registered. Broader asset/material definitions,
  player presentation parity, and arbitrary custom UI remain open surfaces.
- A public-looking trait in a binary crate, or one requiring inaccessible types,
  is not an externally usable extension interface.

The inventory below is the required target surface. Rows beyond the concrete
gaps above require a call-path audit during their implementation; this proposal
does not claim that every existing subsystem has already been exhaustively audited.

## Required capability surface

| Area | What an extension must be able to define or do |
| --- | --- |
| Content and assets | Register namespaced blocks, legal states, items, entities, textures, supported models/materials, tags, display names, and references to other definitions. Use the same asset capabilities as built-ins. |
| Identity and registration | Resolve keys to host-assigned handles; declare dependencies, schemas, and required capabilities; get useful errors for duplicate keys, missing references, cycles, or unsupported capabilities. |
| Blocks | Define supported collision/selection geometry, material/light properties and state-dependent visuals; plan placement, break, use, support changes, and neighbor reactions. |
| Items | Define stack/component schemas, placement and use actions, interaction targeting, and supported presentation. Register harvest/loot rules and recipe/fuel predicates instead of extending built-in item-ID switches. |
| Entities | Define durable private data and bounded public projections; spawn/despawn, schedule, react to interactions/events, and propose state/motion changes using captured views. |
| Block entities | Declare anchors, footprints, valid states, initialization, placement costs, removal/refund rules, and invalidation behavior for every footprint edit path. |
| Movement and creatures | Use body/collision/gravity/navigation services, deterministic random input, sensing queries, and behavior state. Register body/appearance definitions without selecting a built-in creature type. |
| Storage and machines | Define bounded inventories, slot roles/filters, transactions, recipes/processes, progress/status, and automation ports. Storage capacity is independent of player backpack layout. |
| World systems | Register entity-independent scheduled work and persistent owner state; declare bounded reads, wake/event interests, and proposed world/entity/item effects. |
| World generation | Register the generation stages/features needed to reproduce current terrain, vegetation, and spawn behavior, with deterministic seed inputs, ordering, bounded output, and seam ownership. |
| Player-facing gameplay | Register actions, bindings, commands, spawn policies, and supported player state/rule extensions. Player identity and network-session ownership remain host services. |
| Client interaction and UI | Discover actions on blocks, entities, items, or empty space; open registered screens; describe inventory layouts, status, tooltips, and custom bounded UI composition; route requests to registered server handlers. |
| Presentation | Register supported block/item/entity visuals, animation projections, selection shapes, and visual effects through host render resources. Extend lighting properties through content rather than built-in ID checks. Expose sound in the same way when sound is introduced. |
| Persistence and networking | Version private/public/request schemas, retain extension-owned state, validate required content at handshake/load, and send bounded typed requests and projections through the existing replication and commit paths. |

“Supported” is important: this work must expose all existing capabilities, but
does not need to invent flight, skeletal animation, liquids, audio, or an arbitrary
GPU programming API just to call the surface complete. When those capabilities
are added, they follow the same parity rule.

## Contract design

### Registration and composition

Provide one extension entry point with focused builders for content, server
behavior, and client presentation. Resolve namespaced references and dependencies
before freezing registries. Keep built-in numeric save/wire identities reserved;
extensions should not manually choose IDs that can collide. Persist resolved
identities in the content manifest and include relevant schemas/assets in
compatibility checks.

Specify composition per hook: one owning handler for a type, ordered contributors
for suitable registries such as generation, and explicit observation hooks for
events. Reject conflicting ownership rather than silently replacing a handler.
Do not make load order an undocumented gameplay rule. Tags and named capabilities
allow independent extensions to interoperate without importing each other's types.

### Read views, scheduling, and effects

Expose immutable host-owned snapshots/handles and explicit bounded query results.
Distinguish unavailable terrain, exhausted query capacity, stale state, invalid
requests, and unsupported operations. A query may not silently truncate a result
that a planner treats as complete. Reads must become transaction dependencies.

Provide declared effects for the existing operations: block edits, entity
creation/removal/update/motion, inventory moves, recipe consumption/production,
drop creation/pickup, owner-state changes, schedules, and wakes. The coordinator
validates and commits them together. No extension gets `&mut State`, raw journal
access, or authoritative changes through presentation callbacks.

Inventory moves conserve exact stacks/components. Creation and consumption are
separate explicit operations for recipes, generation, loot, or administrative
actions, using the same authority rules as built-ins. Arbitrary opaque payload
codecs cannot prove inventory conservation: declared inventory capabilities must
use host-validated slot operations rather than trusting a policy's assertion that
it moved items correctly.

Document tick and phase ordering, retry semantics, restart behavior, conflict
handling, and whether event delivery is advisory or durable. Irreversible
external side effects cannot occur inside retryable planners. Expensive planning
uses workers; committed presentation events are released after confirmation.
Schedules and durable intent survive restart; callbacks do not block waiting for
terrain or perform synchronous I/O on the window thread.

### Entity and block-entity lifecycle

Separate lifecycle definition from transaction assembly. A registration supplies
initial data and footprint rules; shared helpers produce validated placement,
removal, inventory debit/refund, and content-drop effects.

Cover player edits, administrative edits, system edits, support loss, chunk
unload/reload, and recovery. Breaking any footprint cell must not leave an orphan
entity or duplicate its contents. Unload is not destruction. Dependencies and
footprint ownership must be checked inside the same authoritative transaction.

### Inventory, interaction, and UI

Replace the closed workstation-kind protocol with registered bounded views and
inventory capabilities. A storage descriptor defines capacity and slot groups;
optional process status is separate from the inventory itself. The host owns
container encoding independently of player inventory encoding.

Define automation endpoints with stable port IDs and optional side/access rules.
Offer selection needs an opaque stable selector or slot/stack identity, plus
revision checks, so variants sharing an item ID can be distinguished without
publishing private component bytes. Public discovery remains a hint; final
acceptance checks use authoritative state.

Target resolution produces a registered action set. A generic open-screen action
resolves the entity and its view descriptor. Standard grid/slot/status widgets
cover containers and common machines; a bounded composition path covers custom
interfaces without editing the global screen enum. All controls emit registered,
versioned requests. Server handlers validate actor, reach, target identity,
revision, and slot permissions; a visible button is not authorization.

### Client presentation and resources

Make the existing render shapes and animation helpers available through registered
presentation definitions. Define block/entity targeting independently of player
avatar rendering. Asset handles resolve at startup; GPU resource ownership,
uploads, caching, visibility, and thread scheduling stay with the renderer.

Client callbacks read replicas and produce presentation state. Interpolation,
particles, drop flight, or screen animation must never decide durable ownership.
Missing required presentation/schema support is a clear compatibility failure,
not silently invisible required gameplay content.

### Public boundary and future runtime adapters

Extract a small host-facing API with no dependency on private server `State`,
window objects, sockets, WAL internals, or concrete built-in payload types. It
must be callable from a separate compilation unit. Keep wire/data descriptions
bounded and explicit so a future runtime can marshal them; avoid forcing Rust
object identity or arbitrary closures into persisted/networked contracts.

The first implementation may use Rust traits internally. A future script/WASM
adapter translates that runtime's calls into the same host operations. Execution
isolation, runtime instruction budgets, package signing/distribution, downloads,
and hot reload belong to the eventual loader/runtime design, not to this proposal's
proof of capability access. Do not promise identical execution support across
runtime choices before choosing and testing one.

## Implementation order

These are the original milestone groups, annotated with current status. The
remaining-work sequence above identifies the next implementation slices.

### 1. Establish the boundary and parity inventory

**Status: boundary implemented; exhaustive parity audit remains open.** The public
host crate and separately compiled fixtures exist. The required capability list
above is a target, not evidence that every built-in call path has been audited.

- Trace each built-in from registration through its live execution and client
  path. Record privileged type checks and inaccessible argument/result types.
- Define focused registration/read/effect types and API ownership. Separate
  engine mechanisms from replaceable gameplay policies.
- Add a buildable host API boundary; preserve current game behavior.

Deliverable: a concrete capability checklist with owners and public signatures,
plus an external compile smoke test. Do not build a parallel simulation framework.

### 2. Complete the container/block-entity vertical slice

**Status: container and inventory-machine paths implemented; broader anchored
parity remains partial.** Chest, Kiln, and Hopper use registered production paths.
TallStore and Crusher provide external proofs. Exact automation stack selection,
general lifecycle callbacks/costs, and custom anchored projections remain open;
named/sided ports and basic recipe/fuel registration are already implemented.

- Implement registered lifecycle planning and independent container persistence.
- Add inventory/view/action/screen descriptors and generic client routing.
- Move Chest, Hopper, and Kiln onto these interfaces; remove closed workstation
  kind dispatch and built-in lifecycle exceptions.
- Expose named/sided ports and exact offer selection through the existing atomic
  transfer machinery.

Deliverable: a different-sized container and a differently shaped processing
machine can be registered without edits to shared dispatch, protocol, or UI.

### 3. Complete dynamic entities and presentation

**Status: ground-creature path implemented; drops/player/presentation audit remains
open.** Mossbun uses the public contract and Copperling proves external access.
World-drop policies, player hooks, and broader presentation are not covered by
that completed creature slice.

- Expose spawn/despawn, sensing, schedules, motion/navigation services, entity
  targeting, and registered presentation resources.
- Move Mossbun and world-drop gameplay/presentation policies onto the public
  surface. Audit player avatar/movement/spawn hooks for equivalent access while
  retaining host-owned connection identity.

Deliverable: another creature can have different behavior, appearance, and an
interaction using the same authority, movement, persistence, and rendering paths.

### 4. Close remaining gameplay and world surfaces

**Status: mostly remaining.** Machine recipe/fuel registration is implemented;
general item, player, world-system, and world-generation surfaces remain open.

- Expose item use, harvest/loot, component-aware recipe/fuel predicates, commands/actions,
  world-generation contributions, and supported player-rule hooks.
- Finish the public surface for registered owner/system handlers and effects.
- Audit existing growth, support, and fire paths for built-in-only access. Fire
  migration remains parked until this slice is explicitly undertaken; parity
  cannot be called complete while an existing gameplay system remains privileged.

Deliverable: every capability in the inventory is demonstrated by a built-in
using the host contract, or explicitly recorded as an unresolved blocker.

### 5. Prove integration outside engine internals

**Status: ongoing, with three categories proven.** The fixture already contains
TallStore, Copperling, and Crusher. Add an item action and a world/system
contribution, then finish the cross-category parity audit.

Use one small separately compiled extension package with a registration entry
point containing representative content: a container, processing block entity,
creature, item action, and small world/system contribution. Reuse the fixtures
from prior slices rather than build an elaborate demonstration game.

Load it through an explicit development registration seam. Adding its package
must not require editing engine lifecycle dispatch, screen enums, protocol kinds,
renderer type lists, or persistence switches. Give it only the intended public
API dependency; do not grant private-module access to make it compile.

Deliverable: capability parity demonstrated across server and client. This is
the evidence needed before promising mod authors the surface is complete.

## Verification and completion

Use focused behavioral tests alongside the existing test suite, fmt, and strict
Clippy. Important coverage: stale/duplicate requests, exact inventory conservation,
multi-cell destruction, late-tick restart, cross-chunk reads/edits, malformed public
views, and consistent worker-count outcomes where ordering matters.

Exercise representative extension content over the real nonblocking listener
with an isolated save, including restart and client replica assembly. Inspect
production previews or the live window for rendering/UI changes. Benchmark
rendering/meshing changes using the repository's normal checks; no new soak
campaign or performance target is required for API extraction.

The work is complete when:

1. All existing gameplay categories are covered by accessible host interfaces.
2. Built-ins use those interfaces without hidden gameplay privileges.
3. The external fixture builds and runs without engine-special-case edits.
4. Persistence, authority, threading, and publication invariants still hold.
5. Capability limits and lifecycle/error semantics are documented for a future
   runtime adapter, rather than left implicit in internal implementations.

No world/schema converters are introduced in this prerelease. If implementation
changes save compatibility, advance the default world version and reject
incompatible content explicitly. Choosing the actual mod runtime and distribution
model is the next design decision after, or alongside, these host contracts.
