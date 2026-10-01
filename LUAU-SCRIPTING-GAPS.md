# Luau scripting gaps

Current assessment: October 1, 2026, after Phase 8, runtime tools, player services,
dynamic UI, anchored entities, typed client replicas and the VM lifetime work
recorded in section 6, plus the package composition work in section 7.

All eight phases of the approved non-deferred modding plan are complete. That
delivers a substantial baseline for content, gameplay, generation, persistent
scheduled work, UI and authored visuals. It still leaves gaps that limit larger
mods and new game modes. This document records those gaps and their practical
impact. Sections 1–4 record the completed runtime tools, player-services,
dynamic UI and general anchored-entity goals. Typed client observations are also
complete within their stated scope. VM reuse and retained advisory/client state
are implemented in section 6; save continuity remains open. Their limitations
and the remaining sections remain open gaps. Section 7 records the completed
composition/capacity scope.

See [SCRIPTING.md](SCRIPTING.md) for the implemented Luau API and
[Phase 8 acceptance](docs/modding/PHASE-8-ACCEPTANCE.md) for verification and the
production parity audit. Some gaps are missing Luau bindings to existing Rust
services; others require new engine capabilities.

## Major gaps

### 1. Basic runtime tools — closed

The shared runtime now exposes ordinary Luau libraries: table/string, math,
utf8, bit32, buffer, vector, integer and in-invocation coroutines. Native
`math.random` is seeded from stable host inputs before module initialization;
`math.randomseed` remains available to authors. Server and client startup and
presentation use the same setup. `debug.info`/`debug.traceback` and `os.difftime`
are available; uncaptured OS clocks, wall time and timezone helpers remain absent.

Scripts can call `log.trace/debug/info/warn/error` with a message and simple typed
fields, or use `print`. The bridge attaches entry package/version, executing
module, callback, side and invocation correlation, then emits through the
existing bounded background tracing writer. Per-invocation record/byte limits
suppress excess diagnostics without changing random draws or staged effects.
Imported helpers retain their source identity. Failed invocations retain their
attempt diagnostics; retries can repeat them. `evaluated` records do not prove
that a transaction committed or a client command was displayed.

Host rejection semantics remain intact, including caught invalid operations and
retryable unavailable inputs. Coroutine execution shares latched VM limits;
protected calls cannot turn a limit violation into successful execution.
Editor definitions and the Jade garden demonstrate server/client logging,
buffers and deterministic randomness. Client host contract 3 advertises the new
runtime on unchanged wire version 13.

The [runtime tools reference](docs/modding/RUNTIME-TOOLS.md) documents exact
library support, seed inputs, diagnostic budgets and attempt-versus-commit
semantics. The subsequent VM lifetime work in section 6 adds interpreter reuse
and retained manually resumed coroutines. Live debugging and hot reload remain
separate work.

Evidence: [shared runtime](src/server/script/runtime.rs),
[diagnostics bridge](src/server/script/runtime/diagnostics.rs),
[runtime regressions](src/server/script/runtime/tests.rs),
[action retry regression](src/server/net/tests/script_startup/gameplay/runtime_tools.rs)
and [combined fixture](fixtures/combined-mod/README.md).

### 2. Player and lifecycle hooks

This gap is about integrating mods with people playing the world: identifying
players, responding to their arrival and departure, storing their progress,
querying their current state, and applying server-owned player operations.
It affects ordinary quests and multiplayer modes as well as administration.
The following directions define the authorized implementation goal. Completion
status below distinguishes planned work from bindings already landed.

#### What exists today

The gameplay event contract covers block removal/placement, neighbor changes,
actions, entity ticks and pickup requests. Player services now add admission,
joined/spawned/leaving/left hooks, durable profile state, temporary session state
and logical profile/session timers. Action callbacks have an exact captured player
directory and typed player command targets. The host still owns actor selection
and inventory effects; a profile/session handle is an identity value, not authority.

The server already owns session admission, movement, public player entities,
profile-keyed inventories, saved positions and cosmetic selections. Luau can
select world-wide player rules and appearance palettes at startup, but those
rules are immutable during play. The player directory does not itself grant
runtime movement or cosmetic-change authority.

| Existing surface | What it does not yet provide |
| --- | --- |
| Exact profile/session/avatar handles, captured directory, typed player commands, targeted notices and session kicks | Account authentication remains separate |
| Package-owned lifecycle/gameplay profile state, atomic profile inventory transactions and profile/session timers | Global account/role databases remain separate |
| Validated admission/reconnect spawn proposals, runtime teleport and cosmetic replacement, frozen player rules/palettes | Per-player physics remains deferred |
| Client lifecycle callbacks, selected local public state and targeted status notices | General chat transport and hooks |
| Native and Luau readonly post-commit observers | Exactly-once notification delivery; critical rewards use durable decisions |

The agreed player-services implementation is complete. Native and Luau post-commit observers remain advisory:
they can be dropped under pressure and are not replayed after restart. They
cannot safely be the only mechanism awarding a quest reward or recording a
player's first visit.

#### Player identity and discovery

**Closure direction:** expose exact, host-created player identities and a
useful player query service. Ordinary server mods should be able to identify
their actor, list online players and look up a known profile. These should be
normal supported operations rather than a collection of fixture-specific hooks.

Keep three identities distinct:

- **Profile:** stable across reconnects and restarts; owns inventory and durable
  mod progress. Use an exact handle, never a floating-point ID or display name.
- **Session:** one admitted connection. A reconnect creates a new session; an
  old delayed operation must not accidentally act on the replacement session.
- **Player entity:** the current public avatar. Its lifetime does not determine
  ownership of the profile's saved state.

A proposed player view would expose profile/session/entity handles, online
status, display name, authoritative position and public appearance where
available. An offline profile lookup must distinguish saved profile data from
live session state; it must not fabricate a position or inventory from a client
replica. Define which records can be looked up without creating save data.

Queries need captured revisions and clear consistency rules for retryable
callbacks. Online enumeration should have deterministic ordering and pagination
or an explicit result limit. Callbacks need explicit results for an offline,
unknown or departed player. Document which operations target a stable profile
and which require a particular live session.

#### Lifecycle events and admission

**Closure direction:** support lifecycle registration with defined transition
points. Illustrative event names below describe behavior, not final API spelling.

| Proposed hook | Meaning and useful context |
| --- | --- |
| `PlayerJoining` | Optional admission decision after the host establishes the profile and validates the content handshake, before admitting gameplay; can accept or deny with a reason |
| `PlayerJoined` | A session was admitted; identifies profile, session, current avatar and authoritative initial position |
| `PlayerLeaving` / `PlayerLeft` | Departure preparation versus completed departure; retains identity and a captured final view without pretending the connection is still usable |
| `PlayerSpawned` | An avatar entered play at a validated position; distinguish initial admission from later relocation or respawn |
| World startup/shutdown | Optional world-lifecycle hooks for initialization and cleanup, with explicit recovery and graceful-shutdown semantics |

Define reconnect, duplicate-profile rejection, cancelled bundle downloads,
failed client startup, admission retries, timeout, explicit disconnect and server
shutdown separately. A socket connection is not a successful player join.
Failed or cancelled joins must not produce a successful `PlayerJoined` event.

Joining and departure also need a clear ordering relative to profile loading,
mod-state initialization, spawn validation, outstanding durable actions, public
avatar publication and removal. A departed session cannot veto disconnect or
hold network cleanup hostage. Hooks should run on appropriate callback workers;
file I/O and network cleanup must not wait on arbitrary script execution.

Multiple packages should be able to subscribe to lifecycle notifications.
Admission decisions need a separately defined composition rule, such as all
registered gates accepting, stable evaluation order and a predictable denial
reason. Do not reuse the current singular block-decision ownership rule as an
accidental restriction on lifecycle subscribers.

#### Persistent player state and authoritative decisions

A quest, first-join kit or permission record needs a way to connect the host's
profile identity to package-owned durable bytes. A fixed list of profile seeds
in a startup manifest cannot discover everyone who may join later.

**Closure direction:** provide lifecycle-driven profile-state initialization
and reads/updates of a package's own profile state in relevant server callbacks.
Define schema, size limits, absent-state behavior and revision fencing. Joining
must not initialize the same state twice, and reconnecting must not reset it.
Namespaced ownership allows multiple mods to maintain independent progress.

Separate two callback contracts explicitly:

- **Retryable decisions** read captured inputs and propose authoritative effects.
  State changes, first-join flags and inventory rewards must commit together when
  they form one gameplay decision. Give attempts stable transition identities
  and deterministic random inputs; stale dependencies or unavailable inputs may
  require another attempt.
- **Committed notifications** report what actually happened and cannot veto or
  amend it. Define whether delivery is best-effort or durable, its ordering,
  replay behavior and deduplication identity. Binding the existing advisory
  observer is useful, but does not supply durable follow-up work by itself.

For example, setting `starter_kit_received` and granting the kit should be one
transaction. An advisory `PlayerJoined` notification that grants a kit without
a durable guard can lose or duplicate rewards. Critical follow-up work needs an
atomic decision or a durable scheduled job, not an assumed exactly-once event.

A player's inventory remains keyed by profile, with the existing 128-item stack
cap and exact components. Existing profile-owner scheduling should be reused
where it fits, rather than introducing a second unrelated persistence system.
Crash recovery must not manufacture live connections or replay an advisory join
notification as though the player were currently online.

#### Player operations and permissions

**Closure direction:** add useful typed, server-owned operations alongside the
queries. Candidate operations include validated teleport/spawn placement,
public appearance changes, targeted messages and authorized inventory operations
on an identified profile. Administrative kick, admission bans and role/permission
management need explicit host contracts and persistence policies too.

Ordinary mods need practical authority to run their game modes. Define granted
server-package capabilities and command-caller permissions separately: a trusted
server rule acting on another player is not the same operation as a client
request asking to promote itself. The host validates targets and grants authority;
a script-supplied admin flag or player handle is not an authorization token.

Player movement changes must update the authoritative movement state, collision
checks and client prediction/replication together. Define how teleport interacts
with queued input, chunk availability and saved position. Per-player movement
modifiers require a runtime contract understood by both server and client; changing
one number in Lua cannot safely implement them under the current frozen rules.

Cross-player inventory services must retain conservation, exact components and
transaction validation. Missing/offline targets and stale sessions need explicit
outcomes. Disconnects are session operations, while a persisted ban or profile
state update has a different lifetime and durability requirement.

#### Chat and additional player mechanics

There is no bound chat message or general message-delivery service to intercept.
A useful chat feature first needs server/client transport, text presentation,
authoritative sender identity and delivery semantics. Luau can then provide
formatting, routing, moderation and command integration. Define accepted text,
size/rate limits, recipients and whether moderation decides before delivery or
observes an already delivered message. A command argument is not a substitute
for a complete chat pipeline.

Health, damage, death, respawn, teams and per-player movement modifiers are
separate mechanics to specify where the engine does not already support them.
The player API should make those future additions straightforward, but adding
an event named `PlayerDied` does not create authoritative health or combat.
Initial player spawning already exists; configurable respawn and combat rules
need additional engine behavior. Custom player geometry remains part of the
separately deferred model work.

#### Additional scope accepted for implementation

- **Player-targeted commands:** typed player arguments with name completion,
  resolving to exact profile/session identities. Ambiguous names, missing players
  and stale session targets must produce useful errors.
- **Per-player scheduling:** periodic work, cooldowns and delayed actions tied to
  profiles or sessions. Profile timers use persisted logical deadlines and continue
  while offline; session timers end on disconnect. Neither silently interprets
  server downtime as elapsed wall time.
- **Spawn selection:** mods may propose validated first-join and reconnect
  positions, enabling lobbies, team spawns and checkpoints. The host retains
  terrain loading, collision validation and authoritative placement.
- **Client lifecycle and player-state delivery:** session-ready and disconnect
  callbacks, selected package-owned state snapshots/updates for the local player,
  and reset on reconnect/server switch. Private profile bytes are not implicitly
  exposed to other players or the client.
- **Session-local state:** temporary package-owned participation state, distinct
  from durable profile progress, with explicit disconnect cleanup.
- **Identity trust:** identify what the host verified before invoking admission
  hooks. The current protocol claims a profile identity; it does not prove account
  ownership cryptographically. Local operator-selected admin identity must not be
  advertised as general remote account authentication. Authentication credentials
  and an account service remain separate work.

Region enter/leave hooks remain follow-up work. Chat, health/combat/respawn,
per-player movement modifiers, custom geometry, VM reuse and save converters
are outside this goal. Admission policy can implement package-owned bans/roles
through durable profile state; this goal does not prescribe a separate account
or global role database.

#### Completion criteria and implementation order

1. Specify profile/session/avatar identity, actor lookup, online queries and
   package-owned profile-state initialization. Bind the existing foundations.
2. Add join/leave lifecycle decisions and notifications with explicit admission,
   retry, commit and reconnect semantics. Demonstrate a persistent first-join
   reward and reconnect-safe progress in one runnable package.
3. Add the agreed player operations and permission model. Verify movement and
   inventory effects through their real authoritative paths.
4. Expose the useful native committed-observer surface with honest delivery
   guarantees; add durable follow-up support where critical mod behavior needs it.
5. Scope chat and additional player mechanics as explicit engine contracts,
   with client support, rather than claiming lifecycle bindings close them all.

Verify real nonblocking-listener joins and disconnects with isolated saves:
cancelled joins emit no successful event; duplicate/retried admission does not
duplicate rewards; reconnect retains state but replaces session identity; stale
session work cannot affect a new connection; failed hooks do not publish partial
inventory/state changes or prevent cleanup; restart preserves committed progress;
and multiple mods compose without silently replacing one another's hooks.

Document event ordering, allowed operations, failure behavior, delivery limits
and server/client availability in the runtime inventory and editor definitions.
These are review criteria for this gap, not authorization to change VM lifetime,
add save converters, or implement every proposed player mechanic at once.

#### Execution status

The agreed goal is complete. Identity/directory queries, typed player commands,
admission/spawn and join/leave hooks, package-owned profile/session state,
profile/session scheduling, committed observers, client lifecycle/public state,
notices/kicks, appearance, teleport and cross-profile inventories are implemented.
Roles and admission bans use package-owned durable cells; they do not promote a
profile to native operator or authenticate an account.

Final verification passes **1,175 workspace tests** (1,139 game and 36 host API),
`cargo fmt --all -- --check`, strict all-target/all-feature Clippy and typed Luau
analysis of the runnable welcome package. Real nonblocking-listener tests use
isolated saves and exercise joins, cancelled admission, rollback, reconnect,
restart and client retirement. The public-state panel and native appearance
previews were rendered and inspected. Graphify was refreshed after the code edits.

| Accepted requirement | Regression evidence |
| --- | --- |
| Exact identities, captured discovery, claimed-profile trust and typed/stale command targets | `src/server/net/tests/script_startup/gameplay/players.rs`, `src/client/lifecycle/tests.rs` |
| Admission composition, atomic first-join progress/rewards, reconnect and cleanup | `src/server/net/tests/script_startup/gameplay/players.rs`, `src/server/net/tests/script_startup/join_lifecycle.rs` |
| Offline logical profile timers and cancelled session timers | `src/server/net/tests/script_startup/gameplay/players.rs`, `src/server/net/tests/script_startup/gameplay/player_inventory/lifecycle.rs` |
| General owned profile state, roles/bans, revision/existence fences and restart | `src/server/net/tests/script_startup/gameplay/profile_state.rs` |
| Authorized session operations, validated spawn/teleport and prediction reset | `src/server/net/tests/script_startup/gameplay/player_operations.rs`, `src/server/net/tests/script_startup/gameplay/player_teleport.rs`, `src/server/net/tests/script_startup/appearance/operations.rs` |
| Conserved live/offline inventory transactions, concurrent writers, read reservations, corruption and WAL size rejection | `src/server/net/tests/script_startup/gameplay/player_inventory.rs`, its adjacent `concurrency.rs`/`lifecycle.rs`, `crates/host-api/src/gameplay/tests.rs` |
| Advisory readonly observations and isolated client lifecycle/public projections | `src/server/net/tests/script_startup/gameplay/observers.rs`, `src/client/player_services/tests.rs`, `src/client/lifecycle/tests.rs` |

The runnable example is `fixtures/player-lifecycle/`; the
[active player-services reference](docs/modding/PLAYER-LIFECYCLE.md) records
bindings, operation limits, event ordering, retry/receipt semantics and remaining
engine boundaries. Region hooks, chat/combat/respawn, per-player physics, custom
geometry, authentication, VM reuse and save converters remain follow-up work.

### 3. Dynamic UI and input — closed within agreed scope

Version-2 authored documents now support dynamic subtree creation, deletion and
reordering, scroll panels and tables, checkboxes, finite sliders, selects and
multiline inputs. Stable widget identities retain compatible values and focus;
stale renderer intents cannot target replacement widgets. Version-1 documents
retain their original contract.

Callbacks receive readonly typed control values and emit bounded, atomic widget
updates from the presentation worker. Declared physical-letter actions support
persisted rebindings, game/UI scope, focus and modifier handling. Gameplay
requests still pass through server authorization and durable receipts.

The runnable recipe browser exercises search, category and selection filtering,
dynamic rows, quantity and notes, and finite server-owned crafting. Real listener
tests cover output-capacity rollback, component rejection, duplicate requests,
inventory conservation and save/restart recovery. Regression tests cover whole
reply rollback, bounds, stale intents, focus/value retention and package-private
local UI state.

Evidence: [dynamic UI contract](docs/modding/DYNAMIC-UI.md),
[recipe browser](fixtures/recipe-browser/README.md),
[runtime tree implementation](src/ui/authored/dynamic.rs),
[regressions](src/ui/authored/tests/dynamic.rs) and
[real-server acceptance](src/server/net/tests/script_startup/recipe_browser.rs).

#### Accepted implementation scope

Implemented: extend verified authored documents with a version-2 widget contract
while preserving version-1 documents. Add bounded dynamic subtree replacement
for creating, deleting and reordering data-driven collections; stable widget
identities and retained edit/focus state; scrolling containers and table layouts;
checkboxes, finite sliders, selects and multiline text. Expose current control
values and events to bounded presentation workers, and atomically validate whole
reply batches before changing widgets or submitting gameplay requests.

Declare package-owned input actions with defaults, persisted local rebindings,
explicit game/UI scope and focus/conflict handling. Keys invoke presentation
callbacks; existing semantic actions still require server authorization. No
Luau evaluation, network I/O or config writes may run in the draw pass.

Deliver a searchable recipe-browser package with dynamic rows, selection and a
finite server-authorized recipe action. Verify schema/ownership/bounds, structural
rollback, state/focus retention, input routing and actual server receipts. Inspect
production egui screenshots at desktop and compact sizes, run workspace tests,
formatting, strict Clippy and fixture Luau analysis, update Graphify and commit.
Animation authoring, direct egui/HTML access, imported models, VM reuse and save
converters remain follow-up work outside this implementation scope.

#### Verification

The workspace suite passes 1203 tests (1167 game and 36 host API tests).
Formatting, strict all-target/all-feature Clippy and strict recipe-fixture Luau
analysis pass. Production egui previews were inspected at 1280×720 and 640×360,
including the populated and search-filtered browser.

The release `perf 300 6` scene retains 18,573,688 mesh bytes and 88,026 visible
triangles. Compared with the recorded phase-8 run, scene setup is 2391 ms versus
2329 ms, steady CPU median 0.310 ms versus 0.313 ms, and steady GPU median
0.286 ms versus 0.289 ms. These are separate historical measurements, not a
controlled UI benchmark; this scene excludes live gameplay and package callbacks.

### 4. General persistent block entities — closed

Luau now binds the native general anchored-behavior lifecycle through
`register_anchored` and `bloxgloom:anchored_entities/v1`. Mod developers can define
own-state devices with durable private binary state, explicit public projection,
initialization, validation, interaction, neighbor/support reactions and bounded
cause-specific refunds. Storage and machines retain their specialized contracts.

The host owns atomic placement costs, footprint reservation, durable scheduling,
complete removal/invalidation and refund publication. Client declarations are
inert and preserve the server's catalog/action identities.

See the [Luau anchored API](docs/modding/ANCHORED-ENTITIES.md) and runnable
[counter package](fixtures/anchored-counter/README.md).

#### Accepted implementation scope

Completed scope: expose the existing general Rust anchored contract through
`register_anchored`, with a dedicated package capability and immutable server
callback modules. This is a general own-state device lifecycle, alongside the
existing storage and machine specializations.

- Register owned entity/block identities, placement item/cost, immutable footprint,
  schema/private/public bounds, polling interval, observed terrain and default
  interaction bytes before the catalog freezes.
- Initialize private binary state at placement; validate saved/proposed bytes and
  project explicit bounded public bytes without exposing private state to clients.
- Interact through the existing exact entity/revision/reach-checked action path.
  Interaction updates own state; it does not grant arbitrary inventory authority.
- React to captured neighbor/support terrain on the worker path, with keep/update/
  remove decisions. Polling deadlines persist across restart; terrain wakes are
  advisory and callback invocation counts are not elapsed game time.
- Preserve atomic placement debit, footprint occupancy/ownership, complete
  removal/invalidation, and cause-specific refunds capped by the placement debit.
  Unload is not removal. Callback failures publish no partial state or economy.
- Deliver closed inert declarations to the client, with matching catalog/action
  identities and public projections, without shipping executable server behavior.

Use the native bounds: at most 64 footprint/observed cells, offsets within ±16,
private bytes at most 64 KiB, public bytes at most 4 KiB, placement cost 1..128,
refund no greater than cost, and interaction bytes at most 239. Per-package
registration is bounded and the existing target-action budget still applies.

Land a persistent counter example and focused integration tests for placement,
secondary-cell interaction, replay, captured neighbor reactions, support loss,
removal/refund, rollback, reconnect and restart through the real nonblocking
listener with isolated saves. Verify malformed declarations/callbacks and client
bundle round trips, inspect a production block preview, run workspace tests,
formatting, strict Clippy and typed Luau analysis, refresh Graphify and commit.
General dynamic UI, imported models, VM reuse and save converters remain outside
this goal.

Acceptance on September 30, 2026: all 1,185 workspace tests passed (1,149 game
and 36 host API), including real nonblocking-listener placement, interaction,
replay, support removal, refund conservation, rollback and restart. Formatting,
strict all-target/all-feature Clippy and the counter's Luau analysis passed.
The production block preview was rendered and inspected; Graphify was refreshed.

The binding preserves the native own-state limits: interactions do not grant
arbitrary inventory/world authority, observations capture current terrain rather
than every edit, and public bytes require authored presentation. This closure
does not add dynamic UI, imported models, VM reuse, hot reload or save converters.

Evidence: [Rust anchored contract](crates/host-api/src/anchored.rs),
[Luau adapter](src/server/script/anchored.rs),
[real-listener tests](src/server/net/tests/script_startup/anchored.rs) and
[client codec tests](src/server/script/package/client/declarations/anchored/tests.rs).

### 5. Flexible entities, motion and presentation

Script creatures support terrain-aware ground movement toward horizontal
targets and models built from colored cuboids. Moving entities now add free
velocity/acceleration controls, swept collision and rigid cuboid presentation;
the approved scope is implemented. A [native audio foundation](docs/audio/FOUNDATION.md)
now provides device output, mixing and procedural weather synthesis, but has no
Luau sound bindings or package sound delivery yet.
[Game weather](docs/weather/FOUNDATION.md) now drives this native presentation,
with script weather queries, hooks and controls still unbound. There are no imported
models or custom player geometry. Client presentation offers bounded replica windows, pose/tint overrides, sparks and embers rather than a general
scene/entity renderer.

The native player kit now has authored character styles, first/third-person
body rendering, walking and tool animations, and server-owned crouch stance.
These improve the builtin player experience; they do not expose general Luau
model imports, animation controllers, arbitrary motion or per-player physics.

**Remaining impact:** vehicles, per-player physics, rich animation and audio
need additional engine services. Simple projectiles and guided flying objects
use the moving-entity contract rather than private-state position emulation.

**Closure direction:** introduce public motion/physics contracts for specific
supported behaviors, richer public projections and audio. Imported model
authoring remains explicitly deferred and needs a separate scope decision.

Evidence: [gameplay entities](SCRIPTING.md#persistent-gameplay-entities-and-exact-handles),
[creatures](SCRIPTING.md#mobile-creatures) and
[replica presentation](SCRIPTING.md#public-replica-presentation).

#### Accepted goal: authoritative moving entities and projectiles

**Status: implemented and verified (2026-10-01).**
This closes the simple moving-object/projectile portion of section 5. Audio,
vehicles, general animation controllers and imported models remain separate
work. The author contract is in [Moving entities](docs/modding/MOVING-ENTITIES.md)
and the runnable example is [moving-projectiles](fixtures/moving-projectiles/README.md).

Acceptance includes swept-collision regressions, real nonblocking TCP gameplay,
atomic profile/inventory/drop rewards, failed-reaction cancellation, cold terrain
loading, dormancy, chunk seams and restart. Production GPU previews verify cuboid
motion; the renderer comparison preserves geometry with no observed regression.
See [measurements](docs/modding/moving-entities/PERFORMANCE.md) for the 1/64/256-body
workloads: the hard 256-body bound is safe but exceeds a 20 ms tick budget on the
measured machine, including at ten-tick callback cadence. It is not a 50 Hz guarantee.


**Goal:** let a Luau package declare, launch, steer and render a server-owned
moving entity, receive reliable terrain/entity impacts, and commit impact effects
without duplication across retries or restart. Deliver a runnable throwable
fixture, native and Luau contracts, editor definitions, measured capacity limits,
and real-client acceptance evidence.

##### Actual problems to solve

- Generic gameplay entities have private/public state and durable identity, but
  changing bytes does not move their authoritative position or supply a model.
- Creature declarations combine models with ground locomotion. Horizontal route
  targets and fixed falling behavior cannot express an arrow, a thrown seed or
  a freely steered flying object.
- Existing public entity replicas carry position and motion revisions, but do
  not offer a general velocity/orientation contract for these objects.
- There is no script-facing projectile collision/impact lifecycle. Polling a
  final position misses thin obstacles and does not provide an atomic impact
  receipt tied to the motion that caused it.

Reuse exact identities, owner transactions, captured terrain, startup model
validation and replica transport. Keep the new motion solver and impact lifecycle
separate from creature AI/pathfinding; existing ground creatures retain their
current behavior.

##### Author surface and ownership

Use `host.register_moving_entity(declaration)` through native/Luau contracts.
Declare an own-package key, schema/revision, private/public state limits, behavior
module, optional existing colored-cuboid model, and a motion body. The body uses
an axis-aligned box with an explicit position origin, dimensions, collision mask,
terrain response (`stop`, `bounce`, or `slide`), restitution and gravity scale.
Registration also declares maximum speed, acceleration, lifetime and behavior
cadence. Shape coordinates and units must be documented consistently between
collision, spawn and rendering; they cannot inherit an ambiguous feet/center
convention from another entity family.

Gameplay and declared owner-system services:

| Operation | Intended behavior |
| --- | --- |
| `c.spawn_moving_entity(key, options)` | Stage an owned entity with position, velocity, orientation, initial state and optional exact launch-source identity; return a transaction-local spawn reference |
| `c.motion(id)` | Read captured position, velocity, orientation, grounded state and exact motion revision |
| `c.motion_contact(id)` | Read the owned captured contact target and normal at that motion revision |
| `c.set_motion(id, expected_revision, options)` | Stage velocity, acceleration or orientation changes for a captured owned entity; no arbitrary position teleport |
| Existing owned state/removal services | Update state, publish public bytes or remove a compatible moving entity in the same transaction |

A spawn reference is usable only within its originating transaction. Client
action receipts expose `{ordinal, entity}` mappings for exact allocated IDs;
duplicate delivery in the same action epoch repeats those mappings. Receipt
storage survives crash recovery, while reconnecting starts a new epoch and
rejects stale requests. Autonomous owners have no client action result: native
prepared transactions expose allocations at commit, and owner scripts observe
exact IDs through captured owned entities and callbacks, using durable authored
correlation tags when needed.
Source identity can suppress immediate self-collision and attribute a hit; it
confers no authority over another entity, profile or inventory.

Behavior receives typed tick, impact and expiry inputs with captured state and
motion. A script can guide a flying object through repeated velocity/acceleration
changes. The host integrates continuously at a fixed logical cadence independent
of behavior callback cadence. State lives in durable records, not retained Lua
locals. Cross-package motion mutation requires a separately declared contract;
ordinary motion services operate on owned entities.

Existing validated inventory, terrain, drop and profile operations remain the
way to apply gameplay effects. Do not introduce an implicit health/damage model:
impact exposes exact targets, and damage requires an available explicitly
registered gameplay contract. Only callback capabilities declared and captured
by the host may be used. Moving callbacks have no implicit actor or admin
permission. They capture the player directory and logical clock; packages with
`players/v1` may target explicit profiles through existing finite inventory and
registered profile-state services. Profile rewards join the same atomic reaction
commit, and an errored callback publishes none of its staged effects.

##### Simulation and collision contract

Use logical ticks and a fixed integration step, with specified integration and
axis/tie-breaking order. Replay determinism means the same captured inputs yield
the same proposal within the supported runtime; this does not promise identical
floating-point results across all architectures.

Sweep the body's volume over its entire displacement against authoritative
solid voxel shapes. Checking only endpoints or using unchecked coarse substeps
is insufficient. Support explicit masks for terrain and eligible public player/
creature colliders. Generic entities without a declared collider are not solid.
Dynamic targets use captured collider poses/motion and revisions; the design must
account for relative motion, including two objects crossing during one step.
Choose the earliest contact; equal-time contacts use a documented stable order
based on target kind, cell coordinates and exact entity identity.

Impact input includes the exact moving entity ID, motion revision, logical tick,
contact position, normal, incoming velocity, and either block cell/state or exact
entity target with its captured revision. Restrict launch-source exclusion to a
bounded declaration/launch interval. Body overlap at spawn is rejected; a later
terrain edit embedding the body produces a defined blocked contact rather than
letting it tunnel out. Expiry and world-boundary removal have distinct reasons.

Host collision response resolves stop/bounce/slide before the script reaction.
Contact processing has a bounded iteration count; exhausting it stops remaining
movement safely and reports a reason instead of tunneling or looping. Persistent
resting contact does not emit an impact every tick. Define re-contact using
separation/contact state and test corner bounces, sliding and zero-speed bodies.

Collision and movement publish through the existing owner commit boundary.
Terrain reads, target revisions, destination chunk ownership and motion revisions
are validated together. A stale target or edited obstacle forces recapture and
retry; scripts never resolve collision against client fallback terrain.

##### Impact transactions, unload and restart

Store a durable pending-impact record atomically with the motion/contact result.
Suspend further integration for that object until its reaction is resolved; this
bounds outstanding records and prevents later motion from overtaking an impact.
The reaction transaction consumes the record together with owned state/removal
and authorized terrain/inventory/drop effects. Re-evaluation may repeat callback
execution and diagnostics; committed effects occur once per consumed record.
Client observers and sparks remain advisory and cannot grant rewards.

A callback rejection or execution-budget failure leaves the impact pending,
with bounded retry/backoff and rate-limited diagnostics. Provide an explicit
admin cancellation/removal path for a permanently failing object; silently
consuming a failed impact or repeatedly applying only some effects is unacceptable.
Cancellation removes the object/pending record atomically. Packages can omit an
impact handler when only the declared host response is needed.

Moving bodies remain owned by their current chunk and transfer atomically across
chunk seams. Capture a bounded swept neighborhood. If required terrain is not
available, suspend movement and request bounded normal server terrain preparation;
do not substitute air or synchronously generate arbitrary chunks in a planner.
Dormant objects do not keep unlimited chunks active. Resume from the last committed
pose when their owner/neighborhood becomes eligible again, without simulating an
unbounded backlog or advancing through unseen terrain.

Persist position, velocity, orientation, contact state, remaining simulated
lifetime, scheduled behavior and pending impact identity. Lifetime counts active
simulation ticks; dormancy and server downtime do not age an object or trigger
catch-up movement. Restart resumes the committed state and pending reactions.
Tests must cover crashes before and after impact consumption. If record/wire
schemas change incompatibly, bump the prerelease world target and handshake
contract as appropriate; preserve catalog identities and add no save converter.

##### Replication and presentation

Extend native/client metadata and entity replicas with validated moving-entity
models, pose/orientation and velocity where needed. Render the existing cuboid
model independently of ground-creature stride animation. Interpolate committed
poses using server tick/motion revision; permit only bounded extrapolation and
clamp it at an authoritative stop, impact, despawn or correction. A client cannot
apply collision outcomes or create gameplay effects.

Existing bounded public replica windows and visual effects remain bounded.
An entity leaving a window is not proof of despawn. Clear retained overrides on
session replacement and entity removal, reject stale motion updates, and test
reconnect while objects are moving. Preserve existing creature/player rendering.
Full mesh imports, arbitrary scene nodes and authored animation graphs are deferred.

##### Capacity, performance and implementation slices

Centralize declaration, per-world/per-chunk active-body, pending-impact, collider,
sweep-cell and contact-iteration limits. Reject invalid/nonfinite values and
out-of-range vectors at declaration/service boundaries. Do not silently truncate
colliders or skip a contact when a capture exceeds its budget. Capacity failure
must reject the spawn or safely suspend the affected step with a typed diagnostic.

Set final numeric limits from measured solver/capture and real-listener workload
costs; document them before acceptance. Measure 1, 64 and 256 concurrent bodies
and a deliberate overload case, including clustered launches and chunk seams.
Record server step/capture/commit time separately from retry rate, pending impact
count, memory, replica bytes and client CPU/GPU frame time. Preserve incremental
builds and commit each completed slice.

1. **Contracts and records:** native host types, validated declarations, Luau
   adapters/editor definitions, durable motion/contact/impact records and explicit
   compatibility changes. Add the solver in focused server entity submodules.
2. **Motion and collision:** fixed-step sweeps, supported responses, dynamic target
   fencing, source exclusion, capacity accounting and chunk ownership transfer.
3. **Transactional reactions:** spawn/control services, behavior scheduling,
   durable impact consumption, expiry, cancellation, suspension and restart.
4. **Client presentation:** metadata validation, pose replication/interpolation,
   cuboid rendering, corrections and cleanup on reconnect/despawn.
5. **Fixture and acceptance:** public author documentation, runnable examples,
   real-listener/restart tests, visual inspection and measured limits.

##### Acceptance evidence

Deliver a throwable seed fixture with two modes: a gravity-driven bouncing object
and a guided object whose script updates velocity. A terrain impact can perform a
conditional planting edit; a declared player/creature contact records the exact
hit identity without inventing health. Launch consumes one finite inventory item
atomically; only a committed authored recovery/drop path returns it. Demonstrate
state/public projection, source exclusion, expiry and an impact-driven spark.

Acceptance requires:

- Solver regressions for thin walls, high speed, grazing/corners, relative-motion
  crossings, terrain edits, body overlap, repeated resting contacts and bounds.
- Atomicity/conservation tests for failed launch, retry, stale collision targets,
  impact rejection/cancellation, duplicate delivery and restart around commit.
- Real nonblocking-listener loopback clients and isolated saves exercising launch,
  chunk crossings, missing terrain, cold/cached joins and restart mid-flight or
  with a pending impact. Assert replicated poses and inventory/world outcomes.
- Inspection of a release client window, or a production preview supporting the
  moving-entity renderer, showing launch, arc, bounce, guided motion and impact.
  Static compilation alone cannot establish animation/interpolation correctness.
- Relevant workspace tests, formatting and strict all-target/all-feature Clippy;
  production renderer performance comparison when rendering changes; Graphify
  update after implementation. Publish measured motion workload results and any
  remaining limits alongside the completed author contract.

Completion closes the motion/projectile portion only. Audio, vehicle controls/
rigid-body constraints, per-player physics, imported models and general animation
controllers remain visible section-5 gaps for separate goals.

### 6. Development iteration and save continuity

**VM lifetime and retained state are implemented.** Execution lanes now reuse
bounded Luau interpreters and an immutable compiled-code cache. Authoritative
attempts keep fresh mutable environments, module exports and closures, preserving
retry and generation determinism. Supported client and readonly observer realms
retain module locals, imported exports and manually resumed coroutines.

The complete author contract, runnable example and measurements are in
[VM lifetime](docs/modding/VM-LIFETIME.md). Save continuity remains a separate gap.

#### Implemented runtime ownership and state policy

| Callback class | Implemented lifetime/state contract |
| --- | --- |
| Startup registration | Bounded initialization per world installation/client connection; registration authority ends after startup |
| Generation and authoritative actions, admission, owner jobs, machines and entities | Execution-lane VM/compiled-code reuse with fresh mutable state and imports for each attempt |
| Readonly committed observers | Serialized entry-scoped ephemeral state on the world observer lane; advisory delivery is neither durable nor exactly once |
| Client UI, replica and visual callbacks | Retained entry-scoped state on connection-owned presentation workers |
| Client player-service callbacks | Retained handler-entry state on the connection's separate player-service worker |

Different entry modules, callback worker families, worlds and connections do not
share mutable dependency exports. Worker assignment cannot define authoritative
state. Client code continues to execute off the window/network threads. Closing
an authored UI panel alone does not retire its connection worker; reconnect and
server switching replace the connection's realms.

Existing host-owned profile, entity, scheduled-owner and package session state
remain the authoritative state mechanism. Mutable Lua heaps are not transactional
snapshots, durable progress or commit receipts. Additional world/owner transient
stores still require their own representation, revisions and atomic commit design.

#### Initialization, randomness and host lifetime

Retained entry modules initialize lazily once per realm, with a stable identity
seed. Each callback reseeds native randomness independently from captured inputs.
Imports evaluated during entry initialization use its stable initialization
stream. A dependency first imported inside a later callback instead initializes
from that callback's current stream and then retains its exports. Authoritative
attempts preserve the previous initializer/callback shared stream and retry seeds.
Import visibility, source attribution, cycle detection and bounded depth remain.

Contexts and staged outputs are invocation-bound. Saved host methods are revoked
at completion; resuming an old coroutine cannot regain an expired context or act
on another player. Copied readonly data can remain as historical data. Cached
logging helpers resolve the current callback's diagnostics, correlation and
executing source. No background task scheduler is implied by a retained thread.

Manually resumed coroutines can survive supported retained callbacks. Each resume
uses the current invocation's limits. Authoritative attempts cannot carry a
coroutine across a transaction boundary; durable delayed gameplay still uses the
host scheduler. Automatic waits, signals, promises and wakeups remain deferred.

#### Resource accounting, failure and cleanup

Every invocation resets instruction, wall-time, diagnostic and output counters
and failure latches. Default Lua heaps reserve 8 MiB each against a process-wide
256 MiB reservation ceiling. Client worker families admit eight entry realms;
observer lanes admit 32. Admission fails visibly rather than silently evicting
state. Immutable compiled caches independently bound source/code to 128 entries
and 4 MiB per engine; their eviction only causes recompilation.

Ordinary errors and limit failures retire retained state and discard partial
host outputs. Subsequent calls initialize fresh state, with reset diagnostics;
the shared retained engine increments its generation. Successful isolated
attempts release imports, registry references, environments and temporary state
before returning infrastructure to their lane. Uncertain failed runtimes are
released. Collection runs on workers; teardown releases references without
requiring successful script finalizers. Frozen installations remain frozen.

#### Acceptance and examples

The [welcome fixture](fixtures/player-lifecycle/README.md) demonstrates a retained
client label cache and manually resumed coroutine, plus an ephemeral readonly
observer counter. These illustrate runtime state rather than replacing durable
first-join rewards, profile revisions or existing action receipts.

The opt-in VM benchmark records cold setup, reusable interpreter and retained
callback p50/p95/p99, initializer counts and post-collection heap; production
runner measurements include per-call budget rebinding and cleanup. The
[measurement reference](docs/modding/VM-LIFETIME.md#runnable-example-and-measurement)
states exactly what each benchmark includes. Static world-render timings do not
measure VM reuse. Regression coverage exercises attempt isolation, retained
locals/imports, context expiry, deterministic RNG, budget failures, realm resets
and worker lifecycle. Final verification evidence is recorded with the runtime
implementation rather than treating a microbenchmark speed ratio as completion.

#### Save continuity remains a separate follow-up

Package installations freeze at startup; there is no hot reload or script schema
migration. Several schema/handler identities fingerprint the entire frozen
installation, so even a dependency source edit can make a save incompatible.

**Impact:** developing a larger mod can require restarts and fresh test worlds
for changes that authors would like to test against existing progress.

**Save-compatibility direction:** improve compatibility diagnostics and distinguish
behavior changes from actual persistent-schema incompatibilities where sound.
Hot reload remains deferred. Save converters are expressly excluded during
this prerelease; this gap does not authorize implementing them now.

Evidence: [shared invocation runner](src/server/script.rs),
[runtime setup](src/server/script/runtime.rs),
[invocation-local imports](src/server/script/imports.rs),
[client presentation](src/client/presentation.rs),
[client player-service worker](src/client/player_services.rs),
[save compatibility](SCRIPTING.md#runtime-delivery-and-save-compatibility)
and [repository guidance](AGENTS.md).

### 7. Content-pack scale and composition limits

**Status: closed within the approved composition and capacity scope.**

The approved goal removes the one-system/one-generator Luau adapter restriction
and coordinates larger packages across discovery, startup, delivery, verification
and resource admission. The full contract, runnable examples and measured
acceptance record live in
[Package composition and capacity](docs/modding/PACKAGE-COMPOSITION.md).

#### Registration and deterministic composition

- A package may repeatedly call the existing `register_system(table)` and
  `register_generator(key, revision, module)` APIs with up to eight distinct
  owned keys of each kind. Duplicate keys and invalid declarations reject the
  whole installation, including errors caught by `pcall`.
- Startup canonicalizes blocks, items, textures, systems and contributors before
  assigning identities. Reordering declaration calls preserves numeric block,
  item and texture assignments and ordered generation identities. Existing
  source-sensitive owner compatibility fingerprints still change with source
  edits; this work does not redesign those fingerprints or convert saves.
  Existing single-registration packages remain valid.
- System `after` edges may reference the same package or an explicit direct
  dependency. Missing targets, foreign references, self-edges and cycles fail
  before world open, with the involved keys and cycle path.
- Systems sharing a callback module still have independent owner bytes,
  deadlines, cursors, receipts and durable intents. Existing cross-system wakes
  remain scheduling hints; durable intent payloads remain within one system.
  Aggregate native phase/job/effect/read/state admission is preserved.
- Builtin terrain runs first, followed by lexical namespaced contributor keys;
  later keys win overlaps. Every contributor samples builtin terrain, without
  reading earlier contributors' partial output. Any failure discards the whole
  candidate. Revision identity remains the author's compatibility contract.
- Execution workers reuse interpreter/cache infrastructure while authoritative
  attempts stay isolated. Registration does not create a VM or thread per system
  or contributor, and changes no captured gameplay input or random sequence.

#### Implemented capacity profile

| Resource | Limit |
| --- | --- |
| Packages | 64 |
| Blocks / total items / textures per package | 256 / 512 / 256 |
| Systems / generators per package | 8 / 8 |
| Modules / assets per package | 256 / 256 |
| Modules / assets across installation | 1,024 / 1,024 |
| Manifest / module source bytes | 64 KiB / 64 KiB |
| General asset bytes per file | 2 MiB |
| Discovered file bytes, including manifests | 32 MiB |
| Encoded client bundle, including metadata | 40 MiB |

Block registration consumes an item slot, so 256 blocks leave room for 256
standalone items. These are coordinated admission ceilings, not permission to
multiply every local maximum. Existing native catalog and phase budgets still
apply to the combined installation. The owner catalog's actual ceiling is 128,
while its separate phase registry supports 256 systems; registered generation
contributors remain limited to 256. Trusted preinstalled registrations count
against their respective ceilings. Builtin terrain is a separate first stage.

#### Execution and resource budgets

- Package startup has 250 ms, 50,000 periodic interrupt checks and a 16 MiB VM.
  Installation initialization, from discovery through catalog preparation, has
  ten seconds. Startup uses the existing process-wide VM reservation ledger.
- Gameplay callbacks retain their 50 ms/10,000-check/8 MiB defaults. Scripted
  contributors share 100 ms per candidate chunk; each callback is also bounded
  by the smaller of its normal allowance and the remaining chunk allowance.
  Builtin/native execution and queue delay are outside the scripted allowance.
- Client source preparation has ten seconds and a reserved 16 MiB compiler VM,
  off the window thread. Existing UI/font/shader/image expansion bounds remain.
- Concurrent download/verification buffers have a 128 MiB process budget.
  Retained verified artifacts have a separate 256 MiB/32-artifact process budget
  covering canonical/payload bytes and estimated owned content declarations,
  held once across shared cache/session references and released with the artifact.
  Nested compatibility wrappers retire inner canonical buffers before copying.
- Repeated texture bindings charge actual copied PNG bytes before allocation;
  shared native declaration estimates retain the 64 MiB/4,096-declaration bound.
  Startup, installation merging and delivered metadata use that admission policy.
- PNG decoding remains serial with a maximum 16 MiB scratch image. Material
  arrays use 128² RGBA8 tiles and eight mip levels: 87,380 bytes per layer,
  bounded to 128 MiB/1,536 layers and the actual adapter limit. The renderer
  requests supported capacity and rejects unsupported catalogs before allocation.

#### Diagnostics, compatibility and examples

Touched admission paths identify the package/module, declaration or asset path,
resource, attempted usage, maximum and scope. Delivery failures identify the
bundle and preparation stage. Caught startup/resource errors remain latched;
no partial catalog or save mutation is published. Client runtime profile **8**
rejects unsupported clients before bundle payload or content acknowledgement.
The existing count grammar already supports the new capacities, so no bundle
format conversion or world migration was introduced.

[The farming fixture](fixtures/farming-scale/README.md) contains 128 blocks,
192 total items, 96 useful modules, eight textures across four families, three
independent systems and two contributors. Irrigation wakes growth ahead of its
own deadline, seasons have separate state, and the downloaded panel requests
finite planting/harvesting operations. A separate valid pressure generator
exercises 1,024 modules and assets without padding the gameplay example.

Acceptance covers real nonblocking-listener joins/downloads/cancellation,
conservation and restart, canonical identities, dependency rejection,
maximum-plus-one admission, decoded expansion, memory release/retry, actual GPU
allocation at the texture target, chunk rollback and production previews.
The workspace suite passed 1,285 game tests and 37 host API tests; formatting
and strict all-target/all-feature Clippy passed. Explicit real-listener pressure
and load probes, typed editor checks and inspected release GPU previews passed.
Cold/cached join, movement/action, memory and paired renderer measurements are
recorded in the composition reference.

Motion/audio, imported models, tag-query services, command-schema expansion,
persistent disk bundle caching, marketplaces/CDNs and hot reload remain separate
projects. Save conversion remains excluded during this prerelease.

## Smaller composability gaps

- Tags can be declared, but there is no general Luau runtime tag-query service.
- Command schemas support player targets, item keys, entity keys and counts,
  without general text/numeric arguments or aliases.
- Daylight reads are available in gameplay actions and moving callbacks. Admin
  clock control remains an authorized action; other scheduled planners and
  callbacks use logical ticks.
- Typed local inventory/block/world/action observations are implemented with
  readonly bounded snapshots, real-listener acceptance and production UI previews;
  see the
  [typed replica scope and verification](docs/modding/TYPED-REPLICAS.md).
  General world queries and remote-player inventory access are outside that scope.
- Machine callbacks select declared transformations; they do not expose a
  general arbitrary inventory-processing planner.
- Native item-icon callbacks and per-stack render callbacks are not Luau bindings.
- Package delivery has an in-memory cache, but no persistent disk bundle cache.
- Native fire propagation/delivery still has its explicitly deferred migration.

## Suggested priority

1. Additional motion, audio and richer presentation contracts.
2. Development iteration and the remaining smaller composability gaps.

VM lifetime and retained runtime state are implemented in section 6; larger-package
composition is implemented in section 7.

Compatibility diagnostics should improve alongside those changes. Imported
models, hot reload and native fire migration remain deferred; save conversion
remains excluded during this prerelease. Filesystem/HTTP access, raw GPU access
and marketplace/CDN infrastructure are separate product decisions, not assumed
requirements for closing the gaps above.
