# Complete modding implementation proposal

**Status: Approved — implementation in progress.**

The user approved full implementation and instructed personal review,
incremental documentation updates and commits, continued execution, scoped
agents where useful, and focused verification without excessive test ceremony.

This is the single implementation proposal for completing Bloxgloom's modding
surface and making it usable by mod authors and players. It consolidates the
previous discussion. Once approved, implement the entire non-deferred scope;
do not stop after the host API, a runtime experiment, or one demonstration mod.

This document supersedes [the earlier gameplay proposal](history/MODDING-GAMEPLAY-SURFACE-PROPOSAL.md).
[The earlier surface plan](history/MODDING-SURFACE-PLAN.md) remains a historical
record of work already done. [Start at the modding index](README.md) for the
current authoring and reference documents.

## 1. What approval means

Approval authorizes the following decisions and all implementation phases below:

- **Use Luau through `mlua` as the first supported mod runtime.** It is currently
  the likely choice; approval of this proposal makes it the implementation choice.
- Complete a shared gameplay host API, rather than continuing to add unrelated
  specialized extension interfaces.
- Make built-in gameplay use the same accessible capabilities as mods.
- **Explicit exception to built-in migration:** the original approval included
  native fire migration, but the user subsequently removed it from Phase 2 and
  deferred that migration. Existing server-owned fire remains in production;
  do not claim it uses the public owner contract or block Phase 2 on replacing it.
- Implement server-delivered client/shared mod packages and assets.
- Implement authored UI using existing Rust UI infrastructure, with library
  selection delegated to the implementation process described below.
- Implement mod-authored shaders, textures, materials, and visual effects.
- Complete the documentation, examples, local development workflow, and relevant
  correctness/responsiveness checks as part of the implementation.
- Review integrated work personally; scoped parallel agents are now explicitly
  authorized by the user, with reviewed incremental commits.

Custom imported models and their authoring pipeline remain deferred. Approval
does not authorize choosing that workflow or building a model importer now.

There are no additional phase-by-phase approval gates. Resolve ordinary technical
choices during implementation and record them here. Ask the user only when an
actual blocker requires changing the agreed scope or product behavior.

## 2. Product outcome

A developer can create a mod as Luau scripts and assets without recompiling the
game. They can define content, implement gameplay, generate terrain, build UI,
and supply custom visual effects through documented APIs.

A player can join a modded server and receive everything required to play:

**Connect → resolve/download required client content → prepare it → enter play.**

Our own game development uses that same gameplay surface. New ordinary gameplay
features should not require another engine-only dispatch branch.

**Mod authors are peers, not children.** This is an open-source game; the host
contract is not meant to hide engine ideas or limit authors to toy callbacks.
Expose the full supported gameplay capabilities, including combined world,
entity, inventory and scheduled operations. If an ordinary mod needs a missing
engine operation, add a composable public primitive rather than reserving it
for built-ins. Scoped handles, authority checks, budgets and WAL transactions
protect multiplayer ownership, conservation and recovery; they are not a
pretext for an intentionally weaker mod API. The Rust source and native
extension path remain available to developers who want to change the engine.

“Complete” means covering the game's existing capabilities and the explicitly
requested additions in this proposal. It does not mean predicting every future
physics or rendering feature. New engine mechanisms must acquire public host
interfaces when introduced.

## 3. Starting point: preserve the useful work

Already implemented and integrated:

- Frozen content catalogs, stable namespaced identities, schemas, dependencies,
  capability checks, registered states/items/textures and existing geometry.
- Storage, finite inventories, component-aware recipes, automation, inventory UI,
  anchored lifecycle callbacks, ground creatures, and bounded action panels.
- Persistent owner-local scheduled systems.
- Authoritative transaction assembly, WAL/checkpoint recovery, committed
  replication, worker inputs, and stale lighting/mesh rejection.
- Response-path hardening: bounded player/simulation admission, independent
  motion conflict filtering, fair mesh/upload lanes, and bounded snapshot backlog.

Relevant baseline commits: `8ba1dda` (combined integration), `ecb07ee` and
`36e177b` (gameplay follow-ups), and `4332d32` (response-path hardening).
At the last completed hardening milestone, 702 tests and strict Clippy/formatting
passed. Default worlds are `world-v14` / `world-v14-fixture`.

Reuse these mechanisms. Existing specialized definitions become conveniences
over the shared layer; they do not remain the limits of what gameplay can express.

## 4. One coherent gameplay surface

Expose five concepts consistently across the supported contexts:

| Concept | Author capability |
| --- | --- |
| Define | Register content, entities, handlers, commands, assets, generation and UI |
| Read | Inspect blocks, entities, players, inventories and mod-owned data |
| Change | Modify the world, entity state and items through composable operations |
| React | Handle gameplay decisions and receive committed-event notifications |
| Schedule | Run work on ticks, after delays, or following relevant events |

### Gameplay context and handles

- Give handlers a consistent context for world queries, actor/target information,
  inventory services, persistent state, deterministic randomness and logical time.
- Use namespaced keys for authoring and stable host handles during execution.
- Provide useful spatial queries, block/state access, entity lookup, inventory
  inspection and supported collision/navigation queries.
- Represent unavailable terrain and query-budget exhaustion explicitly. Never
  silently treat missing data as air or truncate a supposedly complete query.
- The host captures read dependencies automatically, including absence checks.
  Authors do not manage chunk stamps, reservations or journal participants.

### Composable changes

Provide shared operations for block edits; entity creation, removal and permitted
state updates; item creation/consumption; inventory transfers; world drops;
owner/player state; and scheduled work.

One handler can combine these operations in one atomic gameplay transaction.
Reads after proposed changes must have documented, consistent read-your-writes
behavior. Return useful results for ordinary failures such as insufficient items
or obstructed placement; errors must not leave partially applied changes.

Transfers preserve exact components and quantities. Item creation and consumption
are explicit gameplay operations, rather than accidental consequences of copying
opaque inventory payloads. Keep the 128-item stack limit and stable profile IDs.

The host handles conflict detection, retries, commit and replication. Retryable
handlers cannot produce irreversible external effects before commit. Persistent
authoritative data belongs in transaction-managed state, not mutable VM globals.
Define handler-error and retry semantics once and use them across gameplay types.

### General entities, not mandatory archetypes

An author can define a durable entity with owned data, public presentation data,
events and scheduled behavior without pretending it is a creature or a machine.
Storage, processing, locomotion and navigation remain optional host services.
Mutating another system's owned state uses its exposed operations/events; scripts
do not replace private host records or circumvent lifecycle ownership.

### Events and composition

- Distinguish decision handlers, which can alter/reject an operation, from
  notifications of already committed changes.
- Cover use, placement, break/harvest, interaction, support/neighbour changes,
  entity lifecycle, inventory/drop activity and player lifecycle as needed by
  existing gameplay.
- Define which handlers own a decision and which can contribute or observe.
  Specify ordering and conflict errors; package load order is not an accidental
  gameplay rule.
- Notifications cannot retroactively cancel a commit. Specify which events are
  durable and which are advisory, and how duplicates/restart are handled.

### Scheduling and persistence

Expose logical ticks, delayed work, periodic work and event-driven wakes through
the same context. Persist the state, schedule and durable intent together.
Provide versioned serializable mod data; do not persist VM internals, closures,
raw pointers or live userdata handles. Long-running/cross-chunk operations can
continue in bounded steps without authors implementing a recovery subsystem.

### Four execution contexts

1. **Startup:** definitions, dependencies, assets and handler registration.
2. **Server gameplay:** authoritative reads and transactions.
3. **Generation:** deterministic bounded world construction.
4. **Client presentation:** replicas, UI, animation, visual effects and requests.

Share API vocabulary and resource identities across contexts. Preserve server
authority: client UI or shader execution never decides ownership or world state.

## 5. Close the remaining built-in gameplay gaps

Treat this list as migration coverage, not five more bespoke frameworks.

### Harvest and loot

Move self-drops, leaves/sticks/saplings, tall-grass seeds and applicable removal
rules to public handlers. Supply removal cause and deterministic random input.
Unify normal breaking, replacement and support-loss behavior. Keep anchored
refunds and container contents correctly accounted for. Removal and resulting
drops/items commit together.

### World behavior

Extend owner-local systems with shared world/entity reads and effects. Move
support behavior and existing entity-independent simulation to the public layer.
Use the existing world machinery; do not invent crop/growth features merely to
fill a checklist.

Native fire propagation and delivery are an explicit deferred exception to
this migration. Keep the current server-owned behavior working; its private
frontier is not evidence that other built-in simulation has migrated.

### World generation

Expose deterministic seed inputs, sampling services, ordered contributors and
bounded output. Specify ownership of features crossing chunk boundaries and how
overlapping contributions compose. Migrate existing terrain and vegetation.
Keep generated base terrain separate from saved edits, and make per-cell fallback
and full-chunk generation agree. Include generator configuration/identities in
world compatibility metadata.

### Player rules, commands and bindings

Expose existing spawn and player movement/body/rule choices, supported player
state and appearance, and command/input registration. Keep authentication,
session ownership, authoritative collision and input sequencing in the host.
Rule changes must be consistent with client prediction and reconciliation.

Move give/spawn and existing gameplay bindings onto semantic action/command
definitions with argument schemas, permissions, discovery and rebinding behavior.
Keep `help` as client-local text generated from those descriptors; it must not
create a server action or WAL transaction. Use the same transaction operations
as other handlers. Player-owned persistent data remains keyed to stable profiles,
not connections.

### World drops

Expose existing drop motion/support, pickup delay/radius, merging and expiration
policies through the shared layer. Preserve conservation and deterministic
selection when players or drops compete. Expose existing pop, hover, spin and
pickup-flight presentation through client services; confirmed pickup events and
server ownership remain authoritative.

### Existing helpers

Route storage, machines, creatures, anchored behaviors and actions through shared
services where they currently duplicate mechanics. Preserve useful declarative
recipes and standard containers. Custom behavior must not require adding a new
closed operation variant for every ordinary gameplay combination.

Built-in gameplay can remain Rust where appropriate, but must use the accessible
host contract. This proposal does not require rewriting every built-in in Luau.
Performance-critical engine mechanisms remain native.

## 6. Luau through mlua

Implement a supported scripting adapter, not a demonstration-only bridge.

- Bind startup registration, gameplay contexts, handles, transactions, events,
  scheduling, persistence, generation and client presentation.
- Define package modules, imports/exports, dependency visibility, entry points
  and error reporting. Authors should not need to know Rust types or thread traits.
- Use worker-owned VM instances/pools or another measured ownership model that
  avoids a single shared VM lock serializing all simulation. Keep live handles
  scoped to their valid callback/session and persistent data independent of VMs.
- Make time/random inputs and retries reproducible where the host requires it.
- Apply execution/memory limits with attributable errors and consistent handler
  failure behavior. Downloaded scripts access the documented host services, not
  unrestricted native process internals.
- Client script work must fit the existing off-window-thread work model; send
  bounded UI/presentation updates and requests through the host.
- Include enough Luau examples to author ordinary content and combined gameplay
  operations without rebuilding Rust. Bind the shared layer comprehensively;
  do not expose only the old specialized fixture interfaces.

The host contract remains language-independent even though Luau is the first
implemented adapter.

## 7. Packages and joining modded servers

### Package contract

Define one documented package format with a manifest, identities/versions,
dependencies, host/runtime requirements, module entry points and asset paths.
Distinguish server-only, client-only and shared content. Support a normal local
mod directory for development and a deterministic packaged representation for
distribution. Use content hashes for immutable cache identity and integrity.

Distribute Luau source initially and compile it with the client's supported
runtime, rather than making cross-version bytecode compatibility a requirement.
Include UI documents/styles/fonts, textures, material/shader declarations and
other supported client assets. Leave room for eventual models without selecting
their format now. Do not include server-only code or private save data in the
client bundle.

### Connection flow

1. Negotiate engine/protocol/runtime compatibility and the required package set.
2. Reuse exact cached packages and transfer missing bytes from the server.
   Keep transfers bounded, cancel cleanly on disconnect and allow a clear
   reconnect/retry with visible preparing/error status. An in-process exact-byte
   cache is sufficient for now; disk caching and an elaborate progress UI are
   not acceptance requirements unless reliability testing demonstrates a need.
3. Resolve dependencies and package resources, execute startup registration,
   prepare required assets and build the session's frozen catalog/client runtime.
4. Validate catalog compatibility/readiness, then enter authoritative play.

Implement actual server-to-client delivery. A manifest telling the user to find
files elsewhere is not completion. A CDN, account service or public marketplace
is not required; transport can be extended later.

Refactor assumptions that require all client content to be installed before
connecting. Preserve frozen catalogs during play, but make negotiated catalogs
and mod resources session-scoped. Switching between differently modded servers
must retire old runtime/resources cleanly while retaining reusable cached bytes.
Report missing/incompatible required content rather than entering a broken world.

## 8. Authored UI/GUI using Rust prior art

Provide a real authoring surface for both the game and mods: layout, styling,
text/fonts, images, scrolling, controls, text input, focus, dynamic updates and
Luau event handlers. Existing inventory/action panels remain conveniences.

**Implementation direction:** evaluate Blitz first for native HTML/CSS rendering
inside our existing wgpu/window stack. It already has relevant embedding work.
Do not assume its high-level HTML wrapper supplies our interactive Luau bridge.
If embedding, input support or runtime cost makes it unsuitable, select an
existing Rust UI/component stack using established layout/text/rendering
infrastructure such as Taffy and appropriate text/rendering libraries. Taffy alone
is not a GUI toolkit. Do not write a new HTML/CSS parser or layout engine.

Resolve the library choice early in this phase using one representative dynamic
inventory/menu interface with text input and a game-rendered preview. Record the
choice and proceed without another general design/approval loop. A full browser,
JavaScript runtime or exhaustive web-platform compatibility is not required.

Expose a consistent document/widget and event model to Luau. Package documents,
styles, fonts and images with the mod; resolve their assets through the same
resource system. Migrate built-in game interfaces onto the selected foundation
so authored mod interfaces are not a second-class overlay.

## 9. Mod-authored shaders, textures and effects

Implement custom shader code as a first-class asset. Use **WGSL** with the existing
wgpu renderer as the initial shader language. A fixed list of material presets is
not sufficient.

- Register shader modules, materials, texture/sampler inputs and typed parameters.
- Define stable host bindings for the transforms, geometry attributes, lighting,
  time and other supported inputs appropriate to each rendering context.
- Support custom material vertex/fragment shading for supported geometry and
  registered visual-effect/post-processing passes. Support declared GPU work and
  intermediate resources where those effects require them, through renderer-owned
  scheduling rather than arbitrary window-thread GPU calls.
- Specify pass inputs/outputs, ordering and composition between mods. Handle
  duplicate ownership and incompatible effect chains explicitly.
- Make parameters accessible from client presentation scripts and replicated
  presentation data. Scripts do not need to know internal renderer object types.
- Compile/prepare resources through the asynchronous asset path, with errors
  identifying the package, shader and location. Include required graphics
  capabilities in readiness checks.
- Use the same resource/material interface for built-in rendering where
  applicable, while keeping renderer implementation details private.

Texture delivery/registration must work for real packages, not just embedded
Rust byte arrays. Expose the texture layouts/sampling needed by the supported
material/effect interfaces. Shader and texture changes must preserve the normal
lighting, meshing and response-path responsibilities of the renderer.

## 10. Custom models: explicitly deferred

Custom models are an intended capability, but no importer or authoring format is
selected in this implementation. First settle the game's long-term modeling
workflow: tools, source/interchange formats, block-like versus free-form assets,
rigging/animation, units/pivots/attachments, material association, collision and
selection, and runtime representation.

Keep existing cuboid/sprite/terrain geometry available to mods and custom shaders.
Design package/resource identities to accommodate later model assets. Do not
expand this task into model tooling or accidentally describe the existing cuboid
support as the final model system.

## 11. Developer workflow and maintenance rules

- Document the actual Luau API, package layout, callback contexts, event ordering,
  transaction behavior, persistence, UI and shaders alongside implementation.
- Provide small runnable example packages covering content, combined gameplay
  operations, scheduled behavior, generation, UI and a custom shader.
- Make local iteration straightforward: run a server with a local package set,
  connect locally, inspect errors, then restart/reconnect after changes. Hot reload
  of a live frozen world is not required for this milestone.
- Attribute registration, runtime and resource errors to a package and source
  location where available. Include useful handler timing/resource diagnostics.
- Built-ins use the same public capabilities; tests/benchmarks may use internal
  fixtures, but production gameplay must not need privileged alternatives.
- Keep focused Rust modules. Extend existing authority, persistence, workers and
  response scheduling rather than creating parallel implementations.
- Preserve built-in numeric identities. If a change makes prerelease saves
  incompatible, advance the default world-folder version; do not build converters.

## 12. Implementation order and deliverables

**Status meanings:** “Done” means the complete phase deliverable works end to
end against this proposal, not just that one representative increment compiled,
rendered or was committed. “In progress” includes useful committed slices with
material missing capability. A commit records reviewed incremental work; it is
not by itself an acceptance of the phase or the overall plan. The overall task
is not done until every non-deferred phase and §13 criteria are satisfied.

| Phase | Work to land | Status |
| --- | --- | --- |
| 1 | Shared read/transaction context, general world/entity/item operations, and harvesting migration | Done |
| 2 | Events, persistent scheduling and remaining non-fire world/drop/player/command behavior; consolidate existing helpers | In progress |
| 3 | Public generation context and migration of existing terrain/vegetation | Done |
| 4 | Complete Luau/mlua bindings, local package loading, module lifecycle and persistence integration | In progress |
| 5 | Server package delivery, cache, negotiated session catalogs and join/switch lifecycle | In progress |
| 6 | Select/integrate the Rust UI foundation, expose authored UI to Luau and migrate built-in interfaces | In progress |
| 7 | WGSL shader/material/effect registration and package-delivered visual resources | In progress |
| 8 | Finish authoring documentation/examples, close remaining built-in-only paths and complete integrated verification | Planned |

### Remaining work by phase (living checklist)

This is a scannable view, not a second specification. **Working** describes
committed capabilities; **Remaining** lists acceptance work, not tasks an agent
has merely started. Check a remaining item only after reviewing the integrated
behavior. The full contracts are in §§4–11 and the history is in §14.

#### Phase 1 — shared core · Done

Public read/transaction context, general world/entity/item operations and
harvesting migration are accepted. No phase-level work remains.

#### Phase 2 — built-in gameplay parity · In progress

**Verified increments (not phase acceptance):**

- Owner-world edits atomically include entities and drops, conditional
  one-chunk-neighborhood edits, bounded durable intents and absent-destination
  bootstrap. Public owner edits support burn removal semantics; native fire
  migration itself remains deferred.
- Chained support-loss edits notify neighbors without double loot (`8140f5e`),
  and adjacent targeted edits retain their distinct `WorldEdit` removal cause
  (`85d4ba4`). Both focused WAL/restart regressions pass.
- Registered anchored reaction removal now stages its complete footprint through
  shared block-removal and neighbor decisions in the same WAL transaction as
  lifecycle refunds and handler drops. A support-loss/receipt/restart regression
  covers the former bypass; owner-system destruction of anchored footprints
  remains open and does not count as completed parity.
- Drop falling and support rechecks use a bounded public `FallingContext`
  (`2a238f3`). Production merge-target selection, pickup eligibility and expiry
  decisions now use public policies (`1367b11`). The fill/split count policy
  also passes the integrated **892/892** suite; the host still owns candidate
  capture, allocation, inventory credit, despawns, terrain wakes and WAL
  commits. Public policy methods alone do not close drop parity.
- Terrain-change wake selection is shared host scheduling rather than a
  drop-specific private decision: the bounded chunk index considers suspended
  entities and public mobile terrain-wake opt-ins, then the drop's public
  falling policy rechecks support. Lost hints fall back to suspended rechecks;
  keep the host selection bounded instead of exposing allocator/index internals.
- Stock drop meshes resolve item sprite/cube appearance through the negotiated
  catalog, so registered item art follows the generic pop/spin/pickup-flight
  path. Authored client-side drop animation/effect overrides are not exposed;
  that custom presentation surface remains open alongside Phase 7 visuals.
- **Verified pickup routing slice:** the stock pickup handler now calls a
  public exact-component, capped slot-routing decision. A rejected transfer
  keeps its amount available for later slots; eligibility, inventory filters,
  player credit checks and atomic drop/inventory WAL ownership remain host-owned.
  The real listener pickup/restart test, 892/892 suite, formatting and strict
  Clippy pass.
- **Verified Luau pickup binding:** `collect_drop(entity, max_count)`
  reuses the same public routing and authenticated inventory operations. The
  server still selects automatic candidates and validates credits and WAL work.
  Its real listener/restart test and the 892/892 suite, formatting and strict
  Clippy pass.
- **Verified pickup eligibility follow-up:** `collect_drop` now
  rechecks the server's public range/delay/expiry policy against the captured
  actor position. The wider generic entity-inventory interaction radius is not
  permission to collect a distant drop; generic transfers keep their existing
  host permissions and transaction validation. The policy and scripted
  eligibility regressions, 895/895 suite, formatting and strict Clippy pass.
- Authenticated `give`/`spawn` console requests use registered actions and
  public Luau gameplay operations (`99e8276`); `help` is client-local text.
  Production `InventoryMove` uses public `move_slots`, preserving capped merges,
  swaps and receipts (`80f7ce7`). Its listener retry/restart and main-tree
  **891/891** tests, formatting and strict Clippy pass.
- Storage uses `StorageBlockEntity`, built-in mossbun uses public mobile
  `Behavior`, and anchored placement/refunds call public callbacks. Machine
  placement/break uses public footprint planning with the existing authoritative
  workstation transaction (`2509896`). Further shared-service parity still
  needs a production audit.
- Production entity startup enumerates frozen catalog mobile, machine, anchored
  and container declarations (`src/server/startup.rs`) rather than registering
  the legacy direct kiln/hopper test types. This narrows the helper audit to
  remaining live transaction/planning branches; old test-only planners are not
  evidence of a private production route.
- **Verified storage-face slice:** `StorageBlockEntity` now declares
  optional validated cardinal automation faces; the production storage `Port`
  enforces them, with the existing all-face default. Restricted faces change
  entity catalog/save identity. Machine face policies were already public;
  host-owned slot mutation and WAL are not missing policy.

**Verified binding slice:** `DropStack` now uses a registered action to take the
exact component-bearing stack and spawn its drop in one WAL transaction. Luau
has matching `spawn_stack` authoring; listener/restart, pickup and the 891/891
main-tree suite pass. The remaining normal `Edit` binding already dispatches
public removal, placement and neighbor decisions after host reach, terrain,
inventory and collision checks; its wire-level choice and
selected-stack validation remain a native route. Do not replace those
authority checks with client-supplied action bytes just to remove the branch.

**Player-rule audit:** the fixed public `player::MotionRates` retains the server
movement budget of 10 blocks/s and client intent rate of 8 blocks/s. The
builtin `player::Body` supplies the same collision samples to prediction,
movement and spawn checks, and its bounds to placement validation. This does
**not** offer mod-selectable rules: those need handshake-visible identity and
consistent prediction/reconciliation, not a server-only tuning knob. Semantic
command discovery/rebinding remains open.

**Appearance audit:** the server publishes a four-byte cosmetic-only player
entity payload, but new sessions still create `[0, 0, 0, 0]` and the client
avatar shader uses fixed skin/shirt/pants palettes. Mod-selectable appearance
needs a negotiated palette/model identity and a profile-owned selection path;
the existing cosmetic bytes alone are not an authoring contract.

**Command contract audit:** registered gameplay actions already use the WAL and
authenticated handler path. A negotiated empty-target command facet now freezes
`Player`/`Admin` permission and bounded typed argument schema in catalog and
package identity. Durable dispatch validates both before the handler. Builtin
`give`/`spawn` use registered `EntityInteract` requests, with their old packets
retained as compatibility adapters; `help` is client-local. All players can open
command entry; the grant browser remains local-admin-only, not an authority
check. Persistent namespaced zero-argument shortcuts require exact session
discovery and become inert on another server or for a command needing arguments.
The request codec remains limited to 130 argument bytes within the 256-byte
interaction cap, and inventory controls still require exactly four bytes.
Real-listener typed-command, permission, malformed-request, receipt and restart
regressions pass; the integrated root suite **916/916**, host API **30/30**,
formatting and strict Clippy pass. Command syntax discovery in the client UI
and a full input-rebinding interface are still open beyond config-file editing.

**Verified spawn-search slice:** startup highest-surface and cached
join upward-then-downward candidate ordering use a fixed public `player::SpawnSearch`.
The host still owns authoritative terrain reads, cache misses, collision and
session admission. A known safe lower surface can still be used when an upper
chunk is missing; missing terrain is never guessed as empty. This is not a
mod-selectable spawn rule.

**Integrated verification for these follow-up slices:** the real-listener
item/catalog negotiation and authored block ABA/panel/restart tests, storage
face/identity and player-spawn regressions, root suite **902/902** (two threads),
host API **25/25**, formatting and strict all-target/all-feature Clippy pass.
The first four-thread integrated run had one load-sensitive Luau `TimeLimit`;
that unchanged test passed alone and in the clean two-thread suite. The
generated built-in drop scene was inspected; custom-item cube/sprite selection
is covered by its mesh regression, not that preview.

**Verified player-state slice:** Rust and Luau gameplay snapshots now
expose the authenticated actor's captured feet position; non-player events
receive no player position. This is a read-only state view, not movement or
appearance rule registration. The real listener/restart action test, 892/892
suite, formatting and strict Clippy pass.
**Verified local binding slice:** inventory, kiln input/fuel and drop
resolve semantic client actions through persistent local config keys
(`bind_inventory`, `bind_kiln_input`, `bind_kiln_fuel`, `bind_drop`). Only distinct
non-movement letters are allowed; invalid combinations fall back to defaults.
The 894/894 suite, formatting and strict Clippy pass. This does not yet expose
mod-defined command discovery or a rebinding UI.

**Remaining**

- [ ] Finish the non-fire removal/support audit and complete drop lifecycle
  parity, including mod-facing client presentation. Motion, merge-target,
  pickup-gate and expiry policies are public; host-owned allocation, inventory
  transfer, entity waking and WAL remain authoritative. Verify remaining
  production routes and item conservation before acceptance.
- [ ] Expose consistent player spawn/movement/body, state and appearance rules;
  finish semantic command/input discovery and rebinding. `give`/`spawn` and
  inventory/drop bindings use registered actions; `help` is client-local.
- [ ] Finish the production shared-service audit for storage, machine, creature
  and anchored helpers. Their public startup declarations/behaviors and external
  listener fixtures cover representative placement, processing, interaction,
  refunds and restart, but that evidence alone does not establish complete
  helper parity.

Native fire migration is **deferred outside Phase 2**. Neither its private
production scheduler nor the unverified optional visual cue counts toward this
phase's acceptance; see the explicit deferred note below.

#### Phase 3 — generation · Done

Public deterministic generation and builtin terrain/vegetation migration are
accepted. No phase-level work remains.

#### Phase 4 — Luau authoring · In progress

**Working:** bounded local packages register sprite items and simple placeable
opaque cubes (including own verified PNG textures), semantic actions/decisions,
entity schemas, generators and chunk systems with authoritative radius-one
neighborhood reads/edits, durable same-system payload intents and optional
destination bootstrap. Lua callbacks execute server-side with source-attributed
failures; local UI handlers and a downloaded, verified
client startup module can initialize connection-local authored UI text/state.
Optional bounded `flammable` and `supports_plant` cube flags are verified
through a real listener/catalog join, restart, invalid-registration regressions,
the 896/896 suite, formatting and strict Clippy. The rest of the public
block/material surface remains open.

An optional `sprite=false` item presentation flag is verified: the server and
downloaded client negotiate the same frozen item definition; omitted options
retain the prior default identity. Its existing cube/cutout drop mesh paths are
covered by a registered-item regression. This does not expose arbitrary item
geometry or client-side presentation callbacks.

**Verified drop-size slice:** public Rust `Item` and Luau item options
can select small/normal/large world-drop mesh size. The default retains its
catalog and bundle identity; non-default sizes participate in item save/handshake
identity and a verified version-8 client bundle. Server-owned age, position,
pickup/expiry and finite inventory remain unchanged. This is one data-only
presentation parameter, not general mod-defined drop animation.
The real-listener catalog negotiation, malformed-bundle and invalid-option
regressions, root suite **906/906** (two threads), host API **25/25**, formatting
and strict all-target/all-feature Clippy pass.

For the drop-size rendering slice, the release `perf 300 6` terrain-only check
against the existing release binary kept normal mesh bytes at 17,292,744 and
visible triangles at 88,026; scene setup 2353→2394 ms, steady CPU median
0.322→0.295 ms, GPU median 0.237→0.251 ms. With `bounced`, mesh bytes stayed
17,500,968 and visible triangles 89,218; setup 2720→2752 ms, CPU median
0.316→0.308 ms, GPU median 0.287→0.278 ms. These single offscreen runs exclude
drop presentation and do not measure its frame cost. The release built-in drop
preview was inspected and looks unchanged; custom size ratios and pickup-flight
continuity are covered by mesh/animator regressions, not that preview.

**Remaining**

- [ ] Widen the initial placeable cube binding to the broader public content
  surface: useful block properties/material options, creatures, machines,
  screens and applicable tags/components. One-state opaque cubes are accepted;
  they do not complete general content authoring.
- [ ] Expose combined world/entity/inventory/scheduled transactions and the
  full relevant decision/removal context to Luau, rather than isolated slices.
- [ ] Bind directly authored owner entity/drop operations and broader owner
  services with the same authority/retry contract as Rust mods. Chunk systems
  now have bounded neighborhood reads/edits and durable same-system intents.
- [ ] Run general client presentation/replica callbacks off the window thread;
  retain scoped handles, budgets and reproducible inputs.

#### Phase 5 — packages and joining · In progress

**Working:** real server-to-client verified bundle transfer, negotiated frozen
catalogs, exact in-process byte reuse and package UI/effect/material assets.
A verified client/shared `client_startup` module executes once per connection
on a bounded worker before `ContentReady`; its host currently exposes only
initial authored UI text/state. Real-listener reconnect/switch tests cover
that narrow session lifecycle. Connection workers now retire on failure/exit;
cached reconnect and differently modded switches start with fresh UI, material,
effect, startup and action state. Join stages/errors are printed to stderr,
including package-attributed startup failures. The client now opens a window
first, renders preparing/error states, and offers retry, cancellation and F2
server switching; the retained join worker cannot install a cancelled session.
GPU/window failures propagate instead of looking like successful exits.

**Remaining**

- [ ] Expand the narrow downloaded startup host into the documented client
  presentation/replica services and resource registration model, with bounded
  session-scoped callbacks and explicit compatibility/readiness failures.
- [ ] Complete the remaining client runtime compatibility/readiness checks and
  exercise the whole package flow under mixed live load. Real-listener
  download, failure, retry, cancellation, reconnect and cross-server switching
  are covered; OS DNS/filesystem cancellation is not instantaneous and GPU
  resource installation still runs synchronously on the window thread. Disk
  caching and elaborate progress UI are **not** prerequisites unless testing
  requires them.

#### Phase 6 — authored UI · In progress

**Working:** Taffy-backed verified documents, basic layout/images/fonts/input,
local Luau event handlers and downloaded startup text/state. The client composes
server-authorized item/empty actions (`uidemo` transfers a stick) and block
actions from its current streamed-world ray hit (`uitarget` trades a stick for
a stone-to-glowstone edit), with denial/receipt feedback. A same-type block
observation fence is verified: the client submits its current streamed
chunk version, and the server compares and retains the authoritative chunk read
through WAL admission. Entity targets remain open.

**Remaining**

- [ ] Support useful dynamic documents/state, scrolling, wrapping and robust
  text entry/focus (including appropriate clipboard/IME/accessibility behavior).
- [ ] Extend authorized UI requests beyond the initial block target to entity
  targets, bounded arguments and server-driven updates without giving client
  handlers authority.
- [ ] Migrate built-in screens onto the same foundation rather than leaving mod
  documents as a second-class overlay.

#### Phase 7 — authored visuals · In progress

**Working:** verified package PNG textures can back sprites and simple placeable
world cubes; one WGSL albedo shader can shade their selected catalog layer, and
one fullscreen scene-color effect runs. UI images/fonts also arrive through
packages.

**Remaining**

- [ ] Widen the initial cube/sprite albedo hook to useful supported geometry,
  material inputs and stable shader bindings beyond one selected texture layer.
- [ ] Bind typed shader parameters to client presentation and replicated data.
- [ ] Support explicit effect-pass inputs/outputs, ordering and composition,
  with bounded renderer-owned GPU resources and useful preparation errors.

Imported custom models and live hot reload remain deferred, not hidden boxes
to check in this phase.

#### Phase 8 — examples and integrated verification · In progress

**Working:** `fixtures/combined-mod/` now gives one runnable `verdant` package
with a package-textured cube and WGSL albedo, authoritative stick-for-block
action from authored UI, downloaded client startup text and durable scheduled
growth. Focused real-listener/restart tests cover the original single client
and two simultaneously connected profiles making independent finite-inventory
actions on separate targets; stale action denials preserve each profile's
balance and restart recovers both edits, both inventories and owner state. Its
package UI was inspected at 1280×720 and 640×360 through
`ui-preview <dir> <package-root>`.

The user reports the Jade example working in the live game; this is a useful
manual check, not a GPU timing or mixed-load measurement. Cross-server switching
has automated coverage but the user's live switch test is deferred. The
two-profile test is partial concurrency evidence, not a sustained mixed-load
response or background-progress measurement; those and the broader audit remain open.

- [ ] Ship a runnable combined package with content, gameplay, scheduled work,
  UI and custom visuals; finish reconciling the complete implemented Luau API
  and local workflow documentation with that example.
- [ ] Audit for remaining non-deferred builtin-only production paths and
  reconcile docs with the actual host contract and deferred scope.
- [ ] Exercise real download/join/switch/restart, mixed-load response and live
  release-window visuals; run the §13 tests, formatting, Clippy and relevant
  rendering/performance comparisons. The latest integrated two-thread suite is
  **907/907**. The user reported the Jade example working live, but that does
  not close mixed-load, cross-server visual or other cross-system checks.

### Explicitly deferred outside the phases

Native fire propagation/delivery migration and investigation of its unverified
optional visual cue are parked by user direction. The existing server-owned fire
behavior remains active; it is a **disclosed parity exception**, not a completed
public-system migration. Revisit only on a new scope decision. This exception
does not reduce any other Phase 2 gameplay, WAL or item-conservation requirement.

Land and review usable increments regularly. A completed slice does not check
off its phase; conversely, do not defer a listed deliverable because one example
works. SHA-256 checks bundle bytes against a session offer, not server identity.

## 13. Completion and verification

The work is complete when all non-deferred phases are implemented and reviewed,
and the following are true:

- Ordinary existing non-deferred gameplay is expressible through the public
  surface and built-in production paths use it. Native fire propagation/delivery
  is the explicitly deferred exception, not an undisclosed parity claim.
- A developer can author and run a Luau package without changing engine dispatch
  or recompiling Rust.
- A client without that package can obtain the required client/shared content
  from the server, join, reconnect and switch servers correctly.
- Combined world/inventory/entity changes and persistent scheduled work survive
  conflicts, callback failures and restart without partial changes or duplication.
- Mods can create authored UI and supply functioning custom shader code/textures
  through the supported package and client resource path.
- New modded behavior preserves responsive movement, edits and interactions under
  the representative mixed workload; background simulation continues progressing.
- The docs describe the implemented surface and deferred work accurately. There
  is no undisclosed built-in-only gameplay path; native fire is listed explicitly
  as deferred rather than reported as migrated.

Keep verification practical: meaningful adjacent regression tests for changed
behavior, real nonblocking-listener join/restart/download checks, and direct
inspection of UI/shader output. Run workspace tests, formatting and strict Clippy.
For rendering/meshing changes run the existing normal/bounced benchmarks and
compare setup, mesh size, CPU and GPU separately. Use the response-path test and
trace for interaction latency; a headless frame benchmark is not a display-latency
measurement. Inspect the release game when available, otherwise generated previews.

Do not turn this into a separate certification process or add tests merely to
increase their count. Verification belongs to implementation, not an additional
user approval gate.

## 14. Continuation record — update during implementation

This section exists so compaction or a new session does not restart the design.

- **Authorization:** approved by the user; all non-deferred phases authorized,
  including Luau/mlua selection. Native fire migration was originally included
  but has since been removed from Phase 2 and deferred by the user. The user
  explicitly authorizes parallel, scoped agents with personal review and commits;
  prefer `coder-fast` and reserve `coder-smart` for unusually difficult work.
- **Current work (September 28, 2026):** Phases 1 and 3 are done; phases 2 and
  4–7 remain in progress, and phase 8 has a working combined package but is not
  accepted. Durable owner intents/bootstrap, burn-cause edits, Luau radius-one
  neighborhoods, package cubes/textures, downloaded client startup and
  authorized item/block UI requests work. Native fire still has its private
  scheduler; general Luau content/client services, UI migration, richer visuals
  and final mixed-load/release-window verification remain. Native fire migration
  and its unverified optional visual are deferred, not Phase 2 blockers. The
  latest reviewed 4-thread suite passed **892/892** after public drop stack filling;
  strict Clippy and formatting also pass.
- **Recent reviewed increments:** `c864aeb` adds the single-package Jade garden
  example, `caaaced` binds Luau neighborhood reads/edits, `663bec8` previews
  verified package UI with startup state, `e156f2d` fixes the kiln test harness,
  `533571e` retires failed/closed modded client sessions, and `6c4528b` adds
  window-visible preparing/error/retry/switch controls. A commit is a reviewed
  increment, not a phase acceptance.

**Historical increment notes:** the entries below preserve what was known when
each slice landed; current scope and acceptance are defined by §§1, 12 and 13.

- **Implementation:** `2674f88` records approval. `c946acf` adds
  `host_api::gameplay::{Context, Snapshot, Plan}` with automatic preimage reads,
  read-your-writes, coalesced block edits, explicit item creation and whole-plan
  failure on ignored errors/budget exhaustion. Ordinary and anchored terrain
  preparation use its host adapter. Harvest policy now lives in `src/gameplay.rs`
  and uses only public host contracts.
- **Second increment (`cf61f82`):** registered removal decision owners (break, replacement,
  support loss) now receive the general staged world context. Built-in harvest is
  the fallback; exact target owners replace it explicitly. Handler-added terrain
  edits and drops join the existing command WAL transaction, with unavailable
  reads requesting real chunks, dependencies captured, and player/anchor checks
  preserved. Startup validates ownership; handler version/target metadata is part
  of the content manifest and survives remapping. Dispatch uses a frozen index.
  Anchored placement uses this path for displaced plants. Anchored destruction,
  cascade semantics and broader lifecycle unification remain phase-1 work.
- **Third increment (`8d797ee`):** the shared context now exposes exact component-preserving
  inventory reads, give/take/transfer, and component-bearing drop creation. Full
  destination/insufficient source returns without partial changes; invalid output
  fails the whole plan. The live removal adapter captures the acting player's
  inventory and commits its changes with terrain/drops, including placement's
  already-staged item debit. Other player/entity inventories are explicitly
  unavailable until their host participants are connected; the public overlay
  already supports transfers between captured inventories. No phase completion
  claim is made for that remaining host integration.
- **Fourth increment (`0cc49d6`):** public entity lookup and anchored-footprint lookup now
  capture presence/absence dependencies in the shared planning read set. Entity
  identity/payload/motion reads fence exact record keys; anchored occupancy reads
  fence the exact cell, rather than blocking on unrelated creatures in its chunk.
  Coordinator admission checks freshness and committed publication rechecks it.
  Entity mutations, entity inventory adapters and generic entity definitions are
  still pending; these read operations do not expose private native payloads.
- **Fifth increment:** registered storage and machine inventories are connected
  to shared reads/give/take/transfer. Slot access and machine item/component
  filters are enforced both during planning and when building entity payload
  updates. Those updates combine with drops, terrain and actor inventory in one
  existing entity/WAL batch. The integrated restart test now transfers part of a
  harvest reward into a nearby chest and checks exact recovery of both sides.
  Generic entity definitions/spawn/update/remove and non-actor player inventories
  remain pending, as do complete removal/lifecycle routing and later phases.
- **Phase 1 closing increment:** registered non-archetypal durable mobile entities
  now have canonical bounded private state and a separate bounded public projection.
  Shared handlers can query nearby entities with complete/absence dependencies and
  spawn, update or remove their own namespaced entities. Spawns validate position,
  radius and resident terrain; entity changes share the WAL record with block
  edits, inventory changes, registered chest/machine transfers and component-safe
  drops. Generic entity state is versioned through the content manifest; byte
  schema changes require a new world version or explicit declaration change, not
  an implicit migration. Anchor destruction now calls the registered removal
  decision on the anchor while keeping full-footprint validation and the existing
  single refund of the block/container contents. Default cube harvest does not
  duplicate that refund. Bound entity state does not imply a model importer,
  arbitrary movement/collision, ticks or a script runtime; those are separate
  phases or explicitly deferred.
- **Latest verification:** initial 704-test workspace run; then 64 durable-action
  regressions, 18 content regressions, and 2 focused gameplay tests passed for the
  registered-handler increment. The gameplay tests cover unavailable neighbour
  reads, seam edits plus drops, duplicate receipts, restart, conflicting handler
  ownership and manifest mismatch. The third increment passed 65 durable-action
  tests and 3 focused public gameplay tests; the integrated seam/restart test now
  also checks atomic inventory rewards and no mutation on unavailable terrain.
  Strict workspace Clippy passed. No benchmark.
- **Fourth-increment verification:** entity regressions (56), conflict regressions
  (11), focused gameplay checks (3), and the real nonblocking-listener edit/restart
  test passed. The new read regression checks that an absent entity becomes stale
  on creation while unrelated mobile occupancy does not invalidate an anchored
  cell read.
- **Fifth-increment verification:** 3 engine gameplay tests and 3 public gameplay
  tests passed, including player-to-chest transfer combined with seam edits and
  drops, duplicate requests, unavailable terrain and restart. All-target/all-feature
  workspace checks and strict Clippy passed.
- **Phase 1 close verification:** 67 durable-action tests, a real nonblocking
  listener edit/recovery check and strict all-feature Clippy passed. Focused
  tests cover a non-archetypal entity's creation, state update and removal,
  nearby read and read-your-writes semantics, atomic chest transfer and anchored
  destruction with one refund plus registered loot. No benchmarks were run.
- **Phase 2 first increment:** `BlockPlaced` decisions now use the same registered
  owner/manifest identity, handler context, dependency capture, item/entity
  participants and WAL transaction as removals. Original edits stage first;
  removal owners run before placement owners, both read the staged world; edits
  emitted by a handler do not recurse into new decision handlers. A focused
  cross-chunk placement test verifies unavailable neighbour deferral, one item
  debit and reward despite duplicate requests, and restart recovery.
- **Phase 2 use increment:** a registered `Operation::Gameplay` action resolves
  its matching startup handler rather than a closed recipe/entity operation.
  Item/empty use receives the authoritative player position; block/entity use
  checks current target identity, reach, interest and captured line of sight,
  plus exact entity state/motion revisions. All variants run on the shared
  world/inventory/entity/drop plan with host-owned receipts and WAL admission.
  The current wire envelope permits up to four argument bytes; broader authored
  argument schemas and client action discovery are later work, not implied by
  this host path. A focused item/entity-use test checks chunk deferral,
  compositional updates, stale identity, duplicate receipts, recovery, and
  rejection of remote effects without partial item debit. Edits, drops and
  spawns are limited to eight cells around the original operation/actor;
  positions outside the world no longer silently saturate to valid coordinates.
- **Phase 2 scheduled-entity increment:** general entities can declare an initial
  due tick and register a `EntityTick` decision owner, or start unscheduled and
  schedule themselves later. `Context::tick()` supplies the logical clock and
  `schedule_entity(id, Some(delay))` / `None` sets or suspends an owned entity's
  persisted due time. A due callback without an explicit reschedule suspends;
  ignored early wakes cannot consume its future due time. Scheduled callbacks
  stage owned state, next due time, world edits and drops together using the
  existing entity due index and WAL path. The focused restart test exercises a
  cross-chunk unavailable read, state/edit/drop/schedule commit, restart at the
  persisted due time and subsequent suspension. General callback planning runs
  on the server coordinator for now rather than the existing immutable entity
  worker jobs; a worker-owned Lua VM and bounded callback execution are still
  required before downloaded scripts can run safely. This is not a general
  cross-owner durable scheduler; post-commit advisory observation is separate.
- **Phase 2 advisory-observer increment:** startup-frozen, versioned observer
  declarations receive public committed block/entity projections and an optional
  player inventory revision, only after WAL receipt and application. A bounded
  32-event/256-KiB delivery lane runs callbacks off the server coordinator;
  overflow, oversized groups and observer panics cannot reject or stall an
  authoritative commit. Delivery is advisory: it may be dropped under pressure,
  and recovery deliberately does not replay it. It is not a replacement for
  durable scheduled intent or the client's authoritative replication stream.
  The focused test checks one combined item-use commit, no duplicate callback
  for a duplicate receipt, no callback on restart and a manifest mismatch for
  an observer's version. Native callbacks must remain bounded; Luau delivery
  and panic/time limits remain later runtime work.
- **Shared deterministic input:** registered decision handlers can call
  `Context::random(cell, sequence)`, derived from the world seed and their
  canonical handler key; repeated plans at the same inputs and independent
  handler registrations do not share a mutable RNG stream. This supports
  retryable use and due callbacks without exposing VM-global randomness.
- **Phase 2 support/neighbor increment:** the shared staged gameplay planner
  now dispatches `NeighborChanged` decisions after block edits, including edits
  produced by semantic use and general-entity due callbacks. The built-in plant
  support policy uses the public handler instead of a player-edit-only branch;
  removal/loot and the triggering edit share one WAL receipt. Explicit target
  handlers may own the neighbor decision for a block type; the fallback runs for
  upward support loss. Neighbor reads capture authoritative chunk dependencies,
  missing chunks defer, and a 256-transition bound rejects recursive edit chains.
  A seam-crossing semantic-use test checks missing-neighbor deferral, a targeted
  support handler, one flower refund despite duplicate requests, and recovery.
- **Phase 2 fire-effect increment:** worker-selected fire burns now dispatch the
  public `BlockRemoved` decision with `Burn` cause and the shared neighbor/support
  decisions before admission. The default harvest deliberately creates no
  rewards for a burnt block. Handler edits, safe anchored-footprint destruction,
  refunds, support-loss loot, entity/drop effects and fire frontier/mailbox/cursor
  transitions share one receipted WAL record. Fire delivery and frontier work
  still use the native registered worker policy and its durable bounded mailbox;
  the behavioral owner/scheduler is **not** yet a public authoring service. The
  burn-and-restart and two-anchored-footprint tests exercise the combined path.
- **Phase 2 world-drop inventory increment:** existing world drops now expose a
  single extract-only `InventoryId::Entity(drop_id)` slot through the same shared
  exact-component `take`/`give`/`transfer` operations as storage and players.
  Depletion plans the ordinary drop update/despawn in the same WAL receipt as the
  player inventory credit; no new drop allocator or save format was added.
  Pickup delay and lifetime still gate extraction on the server, and all entity
  inventories read by a gameplay handler must be within eight cells of its
  authoritative edit/event origins. A focused use action exercises a consumed
  item becoming a drop then returning to inventory, duplicate receipts and
  recovery. Built-in merging/motion/expiry still use their native planners;
  their policy and presentation migration remains open.
- **Phase 2 automatic-pickup increment:** server-eligible drop candidates now
  enter a registered `PickupRequested` decision. The default owner transfers
  exact stacks through the shared drop and player inventory overlay in one WAL
  transaction; an exact `bloxgloom:drop` owner may replace or decline it. The
  host rejects a take of an ineligible candidate and requires every reported
  pickup's item/component quantity to be credited to the player, so pickup
  flight cannot present an uncredited removal. Each attempt processes at most
  32 candidates from the existing 256-ID spatial query, rotating its starting
  point by logical tick so low-ID uncollectable stacks do not starve later
  ones. Selection range, delay and expiration remain host-owned for now.
  Focused pickup checks (18) passed, including decline/uncredited overrides,
  a partial-stack remainder and recovery; strict all-feature workspace Clippy
  and format checks passed. Drop motion, merging, expiration, policy authoring
  and client presentation services are still open.
- **Phase 2 owner-world-read increment:** chunk-partitioned registered owner
  systems may opt into an authoritative, immutable owner-chunk snapshot for
  `Context::block` reads on existing owner workers. Only in-chunk cells are
  available; a missing chunk requests async loading and defers the whole wave
  without advancing its durable owner state/deadline/cursor. Captured chunk
  preimages become shared WAL read reservations until the owner receipt, so a
  conflicting terrain edit retries and can commit once the owner wave applies.
  The declaration is fingerprinted only for systems opting in, preserving
  existing owner-only manifests. At most 64 chunk jobs may opt in per tick.
  An independently compiled fixture checks missing/air/out-of-scope reads,
  a conflicting durable edit and restart; a nonblocking-listener join/rejoin
  checks the actual startup path. The focused 88-test owner suite, format and
  strict all-feature workspace Clippy passed. This is **not** neighbor/world
  effect access or a public fire scheduler; those are the next work.
- **Phase 2 owner-wake increment:** public owner plans may emit bounded
  `(system, owner)` wakes. The existing wake-flag WAL domain persists them in
  the producer's owner-state/deadline/cursor record, verifies the destination
  partition, and serves them later from its normal job budget (including
  after restart). An owner re-waking a destination whose prior flag it serves
  now stages one refresh change instead of duplicate clear/set keys. The
  independent `fixture:wake_pair` checks one-shot delivery after restart and
  `fixture:wake_loop` checks same-wave refresh/replay. The focused 42-test
  wake suite, a combined external owner-read/wake loopback join and restart,
  and strict workspace Clippy passed. These flags carry **no
  effect payload**; neither fire propagation nor general owner-local world
  writes are migrated by this increment.
- **Phase 2 owner-neighborhood-read increment:** public chunk systems may opt
  into a captured read-only 3×3×3 neighborhood (`Some(1)`, at most eight jobs
  per tick) without changing the manifest identities of existing owner-only
  or owner-chunk declarations. The whole wave defers while any required chunk
  is unavailable, with asynchronous requests and no procedural fallback.
  Every captured chunk is fenced as a shared WAL read through receipt. An
  independent neighbor fixture verifies missing-chunk deferral, both sides
  of a seam and out-of-scope reads, an adjacent-chunk edit conflict, and
  restart. The combined nonblocking-listener/restart test exercises all three
  external declarations (owner read, neighborhood read and durable wake).
  The focused 91-test owner suite and strict all-feature Clippy passed.
  This remains read-only: owner-world **effects** are next.
- **Phase 2 conditional owner-world-edit increment:** bounded public owner
  plans can propose exact-preimage block edits inside their owner chunk. The
  shared removal/placement/neighbor planner prepares resulting terrain effects,
  and one WAL record commits them with owner state, deadline, cursor and wakes.
  Generated entity/drop/inventory participants are deliberately rejected until
  the owner transaction can reserve and publish them atomically. The separate
  world-writer fixture exercises deferred loading and restart recovery of
  both owner state and terrain. The focused 92-test owner suite and strict
  all-feature workspace Clippy passed. Fire propagation/delivery remains native.
- **Next concrete step:** add atomic entity/drop participants to owner-world
  changes, then durable cross-owner **payload** intent and move
  fire propagation/delivery decisions off the native-only policy. Migrate
  remaining drops, player rules, commands and helper paths before marking
  Phase 2 done.
- **Phase 3 generation groundwork:** added an opt-in public deterministic
  generation context, bounded chunk-local writer, validated contributor
  registration and key-ordered composition over built-in terrain. Contributor
  failures discard the candidate. Live worlds do not activate contributors
  until authoritative chunk loading, edit baselines and persisted generator
  identity share this composition; Phase 3 remains in progress.
- **Phase 3 live generation integration:** contributor identities (ordered
  namespaced key and revision) now live in `world.meta` version 7, with defaults
  moved to `world-v15` / `world-v15-fixture`. Mismatches fail before changing
  save data. Authoritative loader, pending snapshots, journal restoration and
  edit-baseline reconstruction all use the same frozen composed generator;
  contributor worlds compare edits to their actual generated chunks. The
  nonblocking-listener restart test observes contributed terrain. Workspace
  tests (728 application, 10 host API), strict Clippy and format checks passed.
  Native terrain/vegetation still need migration onto the public contributor
  contract; the language runtime and package pipeline remain later phases.
- **Phase 3 builtin migration:** native terrain and vegetation now run as the
  first implementation of the same bounded contributor contract. One dense
  block vector composes built-in and extension writes before palette creation;
  edited-chunk baselines use the same output. Frozen negative-coordinate,
  canopy-seam and palette tests protect existing generation results. Builtin
  per-cell sampling remains a consistency test, not a separate edit baseline.
  Public sampling and documented cross-boundary authoring conventions are
  remaining Phase 3 polish before calling that phase complete.
- **Phase 3 close:** the public generation context now offers host-backed
  bounded terrain-height and base-block samples at absolute coordinates;
  `docs/modding/GENERATION.md` specifies reproducible seed/salt inputs, lexical
  contributor ordering, overlap, chunk-local output, and cross-chunk anchor
  ownership. Seam/coordinate tests and strict Clippy pass. The full suite had
  one intermittent receipt-timing failure that passed when rerun alone. Phase
  3 covers Rust-native generation; exposing it to Luau and packages is Phase 4.
- **Phase 4 Luau foundation:** `mlua` Luau runs bounded, already-loaded local
  source on a dedicated worker using a fresh sandboxed VM per invocation.
  Source, interrupt, wall-time and memory limits and module-attributed errors
  are covered by focused tests. It has no game dispatch, imports, save-state
  binding or package discovery yet; it is not a complete scripting adapter.
- **Phase 4 local package groundwork:** bounded Unix descriptor-relative
  discovery freezes package manifests and Luau source into immutable snapshots.
  Imports are restricted to the module's own package and direct declared
  dependencies, with cycle/depth checks and per-invocation export/failure
  caching. The existing worker runs these modules within one VM budget and
  fresh retry scope. Fourteen focused script tests, strict Clippy and format
  passed; an unrelated full-suite neighbor timing failure needs rechecking.
  No game registration/transaction binding or network delivery is claimed.
- **Phase 4 local startup binding:** `server-packages` explicitly selects a
  local root; package entries with `bloxgloom:content/v1` may register bounded
  namespaced sprite items through the existing public registrar. Source and
  imports execute on the bounded worker; all declarations validate together
  before the save opens. The nonblocking listener/restart fixture exercises the
  frozen catalog and item delivery. No gameplay events, generation scripting,
  custom assets or network package transfer are bound yet.
- **Phase 4 Luau generation binding:** local packages with an explicit
  `bloxgloom:generation/v1` capability can register one chunk contributor
  with a stable key/revision. Frozen source executes in a fresh bounded VM on
  each authoritative loader/edit worker, using exact seed halves, world/terrain
  sampling, coordinate hashing and bounded block output. Listener/restart and
  edited-baseline tests passed; revisions remain an author responsibility.
  Gameplay callbacks and other server/client services are still unbound.
- **Phase 4 semantic gameplay binding:** opt-in Luau packages can register one
  public `ActionRequested` decision owner and semantic action. Fresh bounded
  VMs borrow the shared gameplay context for block read/edit and exact-stack
  same-player transfers; the normal host transaction and request authorization
  own conflicts, rollback, WAL and replication. Loopback/restart and failure
  rollback tests pass. Other events, general entity operations and package
  delivery still require binding; Phase 4 is not complete.
- **Phase 4 durable owner-script binding:** opt-in Luau packages can register a
  bounded chunk-partitioned persistent system with frozen source, byte state,
  deadline, owner-chunk reads, conditional edits and durable wakes. Fresh VMs
  run directly on existing owner workers; the existing owner WAL owns state,
  cursor, effects and recovery. Nonblocking listener/restart and retry tests
  pass. Entity/profile owners, neighboring reads and other gameplay events
  remain unbound.
- **Phase 4 decision-event binding:** packages may register exact decision
  handlers for removal, placement, neighbors and entity ticks. Shared scoped
  operations now include drops and owned entity state/spawn/scheduling, retaining
  the existing host transaction and WAL. A cross-namespace private-state cache
  authorization bug found during review was fixed and regression-tested.
  Loopback/restart, rollback and entity-deadline tests pass. Luau entity schema
  registration, pickup policy, player services and client presentation remain.
- **Phase 4 Luau entity schemas:** local packages can declare versioned
  exact-length canonical entity state, bounded public-byte projection and an
  optional initial due tick through the existing public entity registry. The
  scheduler now includes only gameplay entities with a registered tick owner;
  handlerless passive types neither dispatch nor enter due indexes. Loopback
  spawn, automatic due, suspension, recovery and unauthorized scheduling
  regressions pass. Structured projection and broader event bindings remain.
- **Phase 4 Luau inventory/pickup binding:** scoped public inventory read,
  give/take and exact component-preserving transfer are available to decision
  scripts. `PickupRequested` receives the host's bounded eligible candidates;
  the server still enforces delay, distance, conservation and player credit in
  the existing WAL path. Loopback pickup/restart and rollback tests pass.
  General notifications, fallback owners and client presentation remain.
- **Phase 5 client artifact groundwork:** package format 2 classifies server,
  client and shared Luau modules and texture assets; format 1 remains server-only.
  Bounded secure discovery builds deterministic canonical client-only bytes with
  SHA-256 cache identity and strict decode/verification. Server sources and
  private save data never enter the artifact. This does not yet deliver packages
  over the network or execute client scripts.
- **Phase 5 transport:** wire v9 offers a frozen bundle before the ordinary
  catalog handshake, streams bounded shared frames on the nonblocking listener,
  and requires a verified bundle acknowledgment before admission. The client
  verifies canonical bytes and caches one immutable artifact across reconnects;
  failed transfers do not enter the cache. Downloaded package definitions are
  not yet registered into the client catalog, so modded joins can still reject
  catalog mismatch; no compatibility check was weakened. Listener/protocol/
  client focused tests, strict Clippy and formatting passed.
- **Phase 5 negotiated catalog slice:** the verified artifact now includes
  canonical, data-only metadata for currently supported package sprite items.
  Clients build a fresh per-session catalog, resolve the server's persisted
  numeric manifest, and verify its exact fingerprint before `ContentReady`.
  Real listener joins and modded/plain session switches pass without executing
  downloaded source. Other content registrations fail explicitly rather than
  admitting clients against an incomplete catalog; Phase 5 remains open.
- **Phase 5 full current Luau catalog metadata:** canonical v3 bundles encode
  supported semantic actions, gameplay entity schemas and handler/system
  compatibility identities as bounded, data-only declarations. The client
  resolves saved numeric IDs and verifies the server manifest without installing
  server callbacks, entity codecs or owner state; generation stays a separate
  saved-world identity. Mixed-package loopback and restart tests pass. Native
  extension serialization, client execution and presentation resources remain.
- **Phase 6 UI foundation:** chose Taffy for flex layout embedded in the
  existing wgpu/winit UI rather than Blitz; `docs/modding/UI-FOUNDATION.md` records the
  tradeoff. A focused, keyboard/focus-driven inventory search is now rendered
  with Taffy geometry, without altering server inventory state. Inspected the
  1280×720 and 640×360 `ui-preview` outputs and verified tests, formatting
  and strict Clippy. Unicode/IME text, mod documents, authored handlers and
  migration of other panels are still in progress.
- **Phase 6 authored document increment:** verified format-2 bundles now carry
  package-scoped bounded UI documents, styles, fonts and images. Preparation
  validates/decodes/rasterizes before window creation; the existing renderer
  displays Taffy-laid-out panels, text, images, buttons and locally editable
  inputs. `fixtures/packages/uidemo/` demonstrates the surface; F6, PageDown,
  Tab and mouse focus are local presentation controls. I inspected desktop and
  compact `package` preview images; package event IDs are visibly UNBOUND.
  No Luau event dispatch, scrolling, Unicode/IME or migration of other built-in
  panels is claimed. Focused tests, strict Clippy and formatting passed; two
  unrelated server tests failed in a full run, one of which passed isolated.
- **Phase 6 local Luau UI events:** a document may opt into a verified
  client/shared presentation module. A dedicated bounded worker runs a fresh
  sandbox VM per button/input event and returns at most 16 typed local text,
  visibility or string-state changes. One outstanding event, atomic validation,
  disabled-on-error behavior and reconnect/document reset prevent unbounded
  queues or stale replies. The uidemo package demonstrates real input/button
  reactions in inspected dynamic previews. No gameplay command, network API,
  timer, server authority, imports or general client runtime is bound.
- **Phase 6 authorized UI action slice:** the client now accepts one bounded
  package-owned semantic action key from a presentation callback, composes its
  own item/empty request with current slot, inventory revision and durable
  session identity, and shows the authoritative `ActionResult`. The server
  registered `uidemo:store` transaction transfers exactly one stick or denies
  it. Real nonblocking-listener tests cover receipts, denial, duplicate IDs,
  retry and restart. No new protocol or client-authoritative effect was added;
  block/entity targets, arguments and richer response data remain open.
- **Phase 7 first WGSL slice:** format-2 packages can deliver one validated
  `scene_color` fullscreen fragment effect with fixed HDR source, sampler and
  elapsed-time/viewport bindings. Preparation validates Naga on a worker and
  compiles the wgpu pipeline off the window thread; the renderer owns the pass
  before bloom/display mapping. The sepia example and inspected effect preview
  prove real package shader execution; conflicting owners and invalid shaders
  fail readiness. This is not custom voxel/item material shading, texture
  registration or a composable multi-effect graph; Phase 7 remains open.
- **Phase 7 renderer-only material hook:** a bounded WGSL `custom_albedo`
  function can shade one selected catalog texture layer in the voxel/cutout
  pipeline while the renderer retains lighting, emission, geometry, fog and
  alpha testing. GPU readback checks selected/default tiles, light levels and
  cutout behavior; a tiny material preview was inspected. The package
  integration below turns this previously renderer-only hook into a usable
  modding slice.
- **Phase 7 packaged material slice:** format-2 bundles now carry one bounded
  owned material descriptor and WGSL albedo shader. The client validates the
  shader off-thread and resolves its namespaced texture key against the
  handshake's session catalog before `ContentReady`; renderer setup installs
  the shader or fails with package-attributed errors. A `jade` fixture,
  GPU readback and nonblocking-listener join tests passed. Bundle format v5
  separates material from UI/effect assets. A follow-up adds bounded package PNG
  texture registration in Luau startup, binds exact verified bytes in client
  bundle v6, and uses `jade:tile` on a package sprite item; listener join,
  identity/remapping, invalid-registration and GPU shading tests passed.
  Basic package-authored cubes landed in the follow-up below; multiple material
  owners, typed parameters and broader geometry remain open.
- **Phase 4 authored cube slice:** format-2 Luau startup now accepts
  `register_block(key, name, texture)` for a bounded, one-state opaque cube
  and its same-key placeable item. The package must first register its own
  declared PNG texture. Client bundle v7 carries the declaration into the
  verified session catalog; a real nonblocking-listener test places the block,
  restarts, compares manifest/fingerprint/PNG identity and confirms world-mesh
  faces select its shader texture layer. Invalid namespace, missing texture,
  duplicates and over-budget declarations fail before opening a save. Focused
  tests, formatting and strict Clippy passed on integration; the agent's
  isolated full suite passed 847/847. This is not property-rich block authoring
  or a general material/geometry pipeline.
- **Phase 2 owner participant increment:** owner-world edits now carry generated
  entity spawns/updates/schedules and drops with owner state, terrain and
  deadline in one WAL record. Entity dependencies and mirror capacity are
  reserved before admission; receipt-gated apply/publication and restart tests
  protect against partial results. Focused owner tests, strict Clippy and
  formatting passed.
- **Phase 2 durable intent increment:** opt-in Rust chunk-owner systems with
  declared world reads can send bounded same-system payloads to existing owner
  cells. Mailbox production shares the producer's WAL record; a later-tick
  destination plan atomically acknowledges delivered messages with owner/world/
  entity effects and forwarding. Receipt gating, cancellation, finite capacity,
  rotation and real nonblocking-listener restart tests passed (six durable
  intent and 98 owner-filter tests). Native fire migration and Luau bindings
  remain open. Two test-harness failures were
  diagnosed and fixed: an idle mobile probe must retain its inbound sender,
  and the entity-neighbour test must wait for the destination chunk/retry.
  The integrated suite now passes **845/845** tests; strict all-target Clippy
  and formatting checks also pass. Live fire presentation remains unverified.
- **Phase 2 destination bootstrap increment:** opt-in Rust chunk systems can
  supply one validated, fingerprinted initial-state template. Absent intent
  destinations get revision-zero owner state, a next-tick deadline and their
  mailbox in the producer's WAL record; rejection releases reservations, and
  receipt/restart preserve both. The host snapshots the template at owner-store
  construction so mutable behavior cannot change it later. Real listener,
  conflict, capacity, retry, forwarding, recovery and frozen-template tests
  passed. The integrated suite now passes **857/857** with the frozen-template
  regression test; strict Clippy and formatting pass. The fixed owner
  cap, native fire migration and Luau intent binding remain open.
- **Phase 2 public burn-cause increment:** an opt-in owner system can declare
  immutable burn edits, limited to non-air-to-air transitions under captured
  world reads. The shared removal/neighbor planner applies `RemovalCause::Burn`
  with loot, support effects, owner state and durable intents in one receipt-
  gated WAL transaction. Focused retry, lost-receipt/restart, destination
  bootstrap and rejection tests pass; the isolated agent suite passed 860/860,
  and integration formatting/strict Clippy and three focused burn tests pass.
  **Native fire has not been migrated**: frontier computation, message batching,
  glowstone seeding and replacement of its private scheduler remain open.
- **Phase 5 downloaded startup slice:** a verified format-2 client/shared
  `client_startup` module can run once per connection on a bounded Luau worker
  before `ContentReady`. Exact direct-dependency imports cannot reach server-only
  source; a narrow presentation host initializes owned authored UI text/state.
  Startup failure refuses readiness with package/module attribution, and
  reconnect/switch constructs fresh session state. Focused real-listener,
  failure, import-visibility and caught-limit tests, formatting and strict
  Clippy passed; the integrated suite now passes **864/864**. General client
  services and asynchronous window-visible preparation remain open.
- **Phase 4 Luau owner-intent slice:** opt-in scripted chunk systems with world
  reads accept immutable bounded inboxes and send binary payloads to same-system
  chunk destinations. A validated constant bootstrap state can create absent
  owners. Invalid or over-budget sends poison the whole callback even when
  caught, and exact source revision/ordinal/tick halves survive Luau numeric
  conversion. Real nonblocking-listener cross-chunk recovery, WAL receipt loss,
  retry, forwarding, output poisoning and identity tests passed; six focused
  integration tests, formatting and strict Clippy passed, with 866/866 on the
  isolated agent suite and 870/870 on a 4-thread integrated run. One earlier
  maximally parallel run hit a load-sensitive 50 ms script wall-clock limit in
  an unrelated 4,100-read budget test; it passed alone. Neighborhood reads and
  directly authored owner entity/
  drop effects remain unbound, and native fire has not migrated.
- **Phase 6 targeted UI block-action slice:** an authored UI callback supplies
  only its package-owned action key. The client samples current streamed terrain
  at dispatch, rejecting absent/wrong blocks before allocating an action ID;
  the server checks target, reach, sight, current inventory/cost and commits
  world/inventory effects through WAL. `fixtures/ui-target-actions/` demonstrates
  a real-listener download, changed aim, denial/retry, reconnect and restart;
  focused integration tests, formatting and strict Clippy pass. The isolated
  agent full suite had a drop-motion timing failure that passed on rerun; the
  main-tree 4-thread integrated run passed **872/872**. Entity targets, argument
  schemas remained open at that increment; the same-type terrain-revision fence
  is addressed in the later Phase 6 follow-up above.
- **Phase 8 combined package slice:** `fixtures/combined-mod/` is a single
  format-2 `verdant` package spanning package PNG/cube/WGSL material,
  server-authorized authored UI action with finite stick cost, downloaded client
  startup text and a persisted chunk-owner growth deadline. Its focused real-
  listener test exercises both terrain changes, finite inventory and owner
  recovery; verifies the transferred material resolves to the authored cube's
  catalog texture layer. Formatting, strict Clippy and the integrated 4-thread
  suite (**873/873**) pass. The package UI was inspected in generated 1280×720
  and 640×360 previews, including its downloaded startup heading; in-world
  material and release-window visual checks were pending at this increment;
  the user later reported the Jade example working live. This does not
  close remaining Luau surface, built-in parity, UI or
  shader/effect scope.
- **Phase 4 Luau neighborhood slice:** `read_radius_chunks=1` captures only
  authoritative 3×3×3 chunk input for a scripted owner. `c.block` and
  conditional `c.edit` can span that neighborhood; missing terrain defers
  work, bad preimages and caught out-of-scope calls poison the whole plan,
  duplicate target cells across jobs reject a wave. `fixtures/neighborhood/`
  sends a durable intent, then edits a neighboring chunk on a later tick;
  real-listener/restart, unavailable terrain, read-fence, receipt loss and
  old-fingerprint tests passed. Five focused integration tests, formatting and
  strict Clippy passed; the main-tree 4-thread suite passed **879/879**. An
  isolated agent run hit a stale-entity kiln placement failure, also observed
  with the shared main-tree test binary on a rerun; the main-tree suite passed.
  Native fire and directly authored owner entity/drop operations remain open.
- **Kiln probe timing fix:** a follow-up full run after the UI preview change
  reproduced the old `stale action entity` failure. The real-listener latency
  harness now retries only that expected revision rejection after observing a
  fresh authoritative workstation replica; other denials still fail and
  placement latency samples remain unchanged. Focused kiln, formatting and
  strict Clippy checks passed, followed by **879/879** on the main-tree 4-thread
  suite. This does not change gameplay authorization or prove the test can
  never fail under other scheduling loads.
- **Phase 5 session retirement slice:** `Network` closes cloned sockets and
  queues on failed join or exit so blocked reader/writer workers stop without a
  window-thread join. Session teardown removes package UI, startup, material,
  effect and pending actions; results with a stale action epoch fail before UI
  state changes. Join diagnostics name the preparation stage and propagate
  window/GPU failures. Real nonblocking listeners cover a failing startup,
  healthy retry, cached reconnect, switches between combined and UI packages,
  exact bundle reuse, stale epoch rejection and worker completion while servers
  stay live. Focused tests, formatting, strict Clippy and the integrated
  main-tree 4-thread suite (**882/882**) passed. Joining remains synchronous
  before the window, with console—not window-visible—progress and errors.
- **Phase 5 window-first join slice:** a retained worker performs config load,
  network/package/catalog/startup work while a simple renderer shows preparation
  and attributed errors. Enter/mouse retry, Esc cancellation and F2 live server
  switching retire the old session; a cancelled or expired prepared candidate
  cannot install its catalog or process authoritative snapshots. Focused real-
  listener cancellation/retry/cache/switch tests, UI bounds tests, formatting
  and strict Clippy pass. Preparing/error previews were inspected at 1280×720
  and 640×360; the 4-thread main-tree suite passed **883/883**. GPU installation
  remains on the window thread; OS DNS/filesystem calls are not forcibly
  interruptible.
- **Fire presentation check:** a short, optional cue for committed burns was
  committed with focused tests and an inspected synthetic preview, but the
  user's live glowstone-beside-tree test showed **no visible fire**. Do not
  treat the visual as verified. Park presentation debugging for now; it is
  separate from the confirmed server-side fire spread. Native fire migration is
  explicitly deferred outside the phases.
- **Open implementation blockers:** none established; UI dependency selection is
  delegated to phase 6, not a reason to block the earlier host work.
- **Deferred:** native fire propagation/delivery migration and optional visual
  investigation; custom model workflow/import; live hot reload; marketplace/CDN
  services; additional language runtimes; new gameplay/engine features not needed
  for existing non-deferred capability coverage.

After approval, record it here, update phase status and meaningful decisions as
work lands, and retain the latest verification results and next concrete step.
Before resuming after compaction, read this document, the continuation record,
repository guidance, and current git state. Preserve unrelated user work.
Continue through all approved phases without reopening settled scope or stopping
at an intermediate milestone. Parallel agents are now explicitly authorized for
separate scoped tracks; review and commit each integrated increment personally.
