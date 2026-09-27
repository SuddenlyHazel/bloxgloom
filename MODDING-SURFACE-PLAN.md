# Plan: built-in/mod capability parity

Status: implementation in progress. The public crate exposes the completed
slices below; the remaining internal interfaces are not yet a supported mod API.

## Implementation status

- **Registered machine slice implemented:** public scheduled behavior, bounded
  private bytes, registered footprints, item filters, recipes/fuels, named/sided
  automation ports, and host-owned inventory/process projections. Production Kiln
  and Hopper use the generic adapter; their older implementations are test-only
  regression references. The independent Crusher fixture exercises manual input,
  automated feed/extraction, processing, restart, and refunds over the real listener.
  Current default save: **`world-v13`**. See [registered machines](docs/REGISTERED-MACHINES.md)
  for supported contracts and remaining general anchored-lifecycle gaps.
  **659 workspace/all-feature tests passed**, with clean formatting and strict Clippy;
  the release shared-inventory preview was inspected.

- **Dynamic entity slice implemented:** public private-state codecs, bounded
  projections, immutable behavior/sensing contexts, host locomotion/navigation,
  scheduling, atomic spawn/self-removal effects, targeting, cuboid models, and
  procedural animation declarations. Mossbun uses these contracts. The separately
  compiled Copperling fixture spawns, patrols, pauses on interaction, and recovers
  through the real listener/client paths. **655 tests passed**, strict Clippy and
  formatting passed, and release previews were inspected. Current default save:
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
- Passive storage, generic inventory screens, mobile creature behavior, and
  inventory machines are now exposed. Arbitrary block/anchored lifecycle callbacks
  and custom UI composition remain open. The broader
  parity inventory and the remaining slices below are not marked complete.

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

## Decisions to make now

1. Adopt capability parity as an architecture requirement.
2. Build a runtime-neutral host contract; choose scripting, WASM, native modules,
   package distribution, and loading mechanics separately.
3. Move built-ins onto that contract as it is implemented. Avoid maintaining a
   second, more powerful built-in path.
4. Validate the public boundary with a small separately compiled Rust extension
   fixture. This proves visibility and capability access, not a native-plugin ABI
   commitment. The previously deferred extension-crate proof becomes a final
   integration step in this proposal.
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

### 1. Establish the boundary and parity inventory

- Trace each built-in from registration through its live execution and client
  path. Record privileged type checks and inaccessible argument/result types.
- Define focused registration/read/effect types and API ownership. Separate
  engine mechanisms from replaceable gameplay policies.
- Add a buildable host API boundary; preserve current game behavior.

Deliverable: a concrete capability checklist with owners and public signatures,
plus an external compile smoke test. Do not build a parallel simulation framework.

### 2. Complete the container/block-entity vertical slice

- Implement registered lifecycle planning and independent container persistence.
- Add inventory/view/action/screen descriptors and generic client routing.
- Move Chest, Hopper, and Kiln onto these interfaces; remove closed workstation
  kind dispatch and built-in lifecycle exceptions.
- Expose named/sided ports and exact offer selection through the existing atomic
  transfer machinery.

Deliverable: a different-sized container and a differently shaped processing
machine can be registered without edits to shared dispatch, protocol, or UI.

### 3. Complete dynamic entities and presentation

- Expose spawn/despawn, sensing, schedules, motion/navigation services, entity
  targeting, and registered presentation resources.
- Move Mossbun and world-drop gameplay/presentation policies onto the public
  surface. Audit player avatar/movement/spawn hooks for equivalent access while
  retaining host-owned connection identity.

Deliverable: another creature can have different behavior, appearance, and an
interaction using the same authority, movement, persistence, and rendering paths.

### 4. Close remaining gameplay and world surfaces

- Expose item use, harvest/loot, recipe/fuel registration, commands/actions,
  world-generation contributions, and supported player-rule hooks.
- Finish the public surface for registered owner/system handlers and effects.
- Audit existing growth, support, and fire paths for built-in-only access. Fire
  migration remains parked until this slice is explicitly undertaken; parity
  cannot be called complete while an existing gameplay system remains privileged.

Deliverable: every capability in the inventory is demonstrated by a built-in
using the host contract, or explicitly recorded as an unresolved blocker.

### 5. Prove integration outside engine internals

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
