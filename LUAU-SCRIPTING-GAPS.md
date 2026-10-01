# Luau scripting gaps

Current assessment: October 1, 2026, after Phase 8, runtime tools, player services,
dynamic UI, anchored entities and typed client replicas. VM lifetime work below
is a proposal for review; implementation has not started.

All eight phases of the approved non-deferred modding plan are complete. That
delivers a substantial baseline for content, gameplay, generation, persistent
scheduled work, UI and authored visuals. It still leaves gaps that limit larger
mods and new game modes. This document records those gaps and their practical
impact. Sections 1–4 record the completed runtime tools, player-services,
dynamic UI and general anchored-entity goals. Typed client observations are also
complete within their stated scope. Their limitations and the other sections
remain open gaps.

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
semantics. VM reuse, cross-invocation coroutine persistence, live debugging and
hot reload remain separate work; this closure does not change VM lifetime.

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
targets and models built from colored cuboids. Generic gameplay entities do not
provide arbitrary motion or a creature model. There are no bound sound APIs,
imported models or custom player geometry. Client presentation offers bounded
replica windows, pose/tint overrides, sparks and embers rather than a general
scene/entity renderer.

The native player kit now has authored character styles, first/third-person
body rendering, walking and tool animations, and server-owned crouch stance.
These improve the builtin player experience; they do not expose general Luau
model imports, animation controllers, arbitrary motion or per-player physics.

**Impact:** projectiles, vehicles, flying creatures, rich animation and audio
need additional engine services. A mod cannot build those features solely by
changing an entity's private state bytes.

**Closure direction:** introduce public motion/physics contracts for specific
supported behaviors, richer public projections and audio. Imported model
authoring remains explicitly deferred and needs a separate scope decision.

Evidence: [gameplay entities](SCRIPTING.md#persistent-gameplay-entities-and-exact-handles),
[creatures](SCRIPTING.md#mobile-creatures) and
[replica presentation](SCRIPTING.md#public-replica-presentation).

### 6. Development iteration and save continuity

This area contains two related projects: runtime lifetime/state retention and
save-compatibility diagnostics. VM lifetime is the proposed next project, ahead
of larger-package composition. It does not require hot reload or save conversion.

#### Current VM lifetime and practical problems

The shared runner creates a Luau VM for each invocation. It installs libraries,
deterministic random state, diagnostics and limits, constructs an invocation-local
import cache, evaluates the entry module, invokes the returned callback, and
then drops the VM. Imports share cached exports within that invocation, but the
next invocation evaluates those modules again. Reusing the immutable package
snapshot does not reuse its Lua globals, exported closures or heap.

Some surrounding Rust workers and session objects already live across calls.
Their lifetime should not be confused with the lifetime of the Lua interpreter.
Gameplay adapters also borrow their host context through `mlua::scope`; moving
the VM into a long-lived field does not make those borrowed contexts reusable.

This has several costs for mod authors and the engine:

- Module-local tables, closure state and globals disappear after each call.
  Authors cannot naturally retain an animation controller, compiled lookup
  table, UI controller or coroutine between callbacks in those Lua objects.
- VM setup, module compilation/evaluation and allocation repeat. Their actual
  share of callback cost needs measurement; reuse is not a measured speedup yet.
- A module initializer is currently part of each invocation. Changing its
  lifetime changes observable behavior, including random draws and diagnostics.
- Long-lived workers do not currently provide a clear script-level initialization,
  reset and teardown contract for a retained runtime.

Persistent gameplay state is already supported through host-owned entity,
profile and scheduled-owner state; package-owned session state also exists.
The missing feature is useful Lua/runtime state retention and efficient execution,
not the ability to save quest progress at all.

#### Proposed goal and author experience

Replace invocation-owned VM allocation with bounded runtimes owned by the
appropriate execution lanes. Give authors documented retained module state in
supported non-authoritative callback realms, while preserving isolated,
deterministic attempts for authoritative gameplay. Define initialization,
reset, resource accounting, host-context lifetime and coroutine behavior before
making retained state part of the public runtime contract.

Two improvements must be distinguished explicitly:

| Improvement | Author-visible promise |
| --- | --- |
| VM and compiled-code reuse | Calls can reuse interpreter infrastructure without inheriting another attempt's mutable state |
| Retained module state | Locals, tables and closures survive successive callbacks in a named runtime realm, until a defined reset |

For example, a client presentation module could retain its selected recipe,
animation tracks or a lookup cache in ordinary locals. Closing its UI should not
silently destroy connection-scoped module state. Reconnect or switching servers
must start a new realm. Existing explicit UI state remains available; authors
should know when it is preferable to ordinary module locals.

For authoritative callbacks, `local requests = 0` followed by incrementing it
inside a retryable action cannot be treated as committed state. A rejected
attempt might increment it, and a retry would then see a different value. Gameplay
progress and decisions continue to use captured host-owned state with revision
checks and atomic effects. A reused VM alone does not solve that transaction.

#### Runtime realms, ownership and execution lanes

A **realm** is a documented lifetime and isolation boundary for a package's
execution state. Its identity must include the server world or client connection,
side, frozen installation identity, entry package and callback lifetime policy.
An owner/session identity belongs in the key only when the contract actually
offers state with that scope.

Recommended ownership rules:

- Keep each physical VM confined to its owning execution lane. Serialize calls
  that share mutable module state; do not share one global VM behind a lock.
  Client Lua continues to execute off the window/network threads.
- Separate startup, generation, authoritative decisions, committed observers and
  client presentation/lifecycle contexts. A server capability must not become
  available merely because its module was loaded into a client or readonly realm.
- Define state by logical realm, not by whichever worker happens to receive a
  job. Worker-local caches are an implementation detail, not a package-wide state
  API. Scheduling an owner on another worker must not change authoritative output.
- Keep dependency exports isolated by consuming realm unless sharing is an
  explicitly supported immutable contract. A mutable dependency singleton must
  not accidentally connect otherwise independent packages or sessions.
- Bound both runtime count and total resident memory. A VM for every entity,
  player/package combination or callback is not an acceptable default allocation
  strategy without a corresponding admission and memory model.

The implementation should retain focused modules for realm ownership, imports,
attempt bindings, accounting and lifecycle. Reuse the existing execution lanes
where their ownership fits; identify adapters that need routing changes rather
than adding an independent scheduler around every script call.

#### Recommended state policy by callback class

| Callback class | Proposed lifetime/state contract |
| --- | --- |
| Startup registration | Bounded server initialization per world installation and client initialization per connection; registration authority ends when startup finishes |
| Terrain generation | Reuse VM/compiled code, with isolated mutable attempts; output cannot depend on chunk request order or worker assignment |
| Actions, decisions, owner jobs, entity/device callbacks and admission decisions | Reuse execution infrastructure; preserve attempt isolation and host-owned transactional state |
| Readonly committed observers | Retained ephemeral locals may support diagnostics/caches; advisory delivery is not durable or exactly once |
| Client UI, replica, visual and player-service callbacks | Retain module state in defined connection/package realms, with serialization and explicit teardown |

Client UI and player-service callbacks currently use different workers. Before
promising that they share a package's globals, decide whether to consolidate
their execution or document separate realms with explicit communication. Two
independent copies of a module must not be advertised as one shared singleton.

Reuse existing package-owned session state where authoritative transient state
is needed. If a demonstrated world/owner scope needs an additional host-owned
transient store, specify its data representation, bounds, revisions, commit
ordering and restart reset behavior separately. It must not publish updates from
failed attempts or become a second, unrelated persistence system.

An arbitrary mutable Lua heap is not a transaction snapshot. Freezing an export
table does not freeze scalar upvalues in its functions; copying tables does not
restore closures, aliases, threads or host userdata. Do not claim general mutable
server module state is rollback-safe without a design that covers those cases.

#### Module initialization, imports and deterministic randomness

Cache source or compiled chunks before attempting to cache live authoritative
exports. Isolated callback environments and invocation-local import results can
preserve the current retry semantics while the physical VM remains alive. Live
export/closure caching belongs in retained-state realms unless a sound immutable
module contract has been established.

Stateful realms should initialize a module once per realm and retain its exports.
Specify lazy/eager initialization, import cycles, failed-initializer caching,
dependency ownership and reset ordering. Initializers should not capture the
first player's action context or gain authority to submit effects for that action.
Startup registration is its own explicit initialization contract.

Preserve deterministic seeding for authoritative attempts, including retries,
recaptured inputs and generation. Reset random state at the defined attempt
boundary rather than allowing a previous callback to advance the next one's RNG.
Budget failures, logging and unrelated package work must not perturb its draws.

Retained-state module initialization needs a separate stable realm/module seed
and documented callback random behavior. Today an initializer may consume random
draws before the callback; evaluating it once changes both its values and the
callback's draw sequence. Treat this as an author-visible contract decision,
not an incidental consequence of adding a cache. Decide defaults and opt-in/version
behavior before migrating existing callback classes to retained state.

Keep the current import namespace, dependency visibility, source attribution and
bounded cycle/depth checks. Cache keys must include the frozen source identity;
an old server connection or installation cannot supply another realm's exports.

#### Host-context lifetime and coroutine behavior

Per-invocation host contexts, staged effects, captured snapshots and diagnostic
correlation must be rebound for each call. A cached function must never retain a
live borrowed Rust context from an earlier invocation. Define a revocable context
generation: saved context objects/methods must fail after their invocation ends,
rather than access an expired borrow or act on the next player's context.

Authors may retain copied readonly data as historical data, within memory limits.
Such data is not a fresh world view or an authorization token. Logging helpers
that survive initialization must resolve the current invocation's correlation
and executing module rather than retain the initializer's identity forever.

Keeping a VM alive makes retaining a coroutine object possible; it does not
create a task scheduler. The recommended initial scope is manually resumed
coroutines in supported retained-state realms, with the current callback's
instruction/time budget covering every resume. Suspending one cannot extend a
borrowed host context's lifetime.

Authoritative attempts must not yield across a transaction/receipt boundary.
Durable delayed work continues to use the existing host scheduler. Host-managed
`wait`, signals, promises and automatic coroutine wakeups require a separately
specified scheduler, cancellation and budget contract; they are not implied by
this first milestone. On realm teardown, retained threads and references are
released and can no longer submit commands for the departed session.

#### Limits, failures, garbage collection and reset

Retain per-call instruction, wall-time, diagnostic and staged-effect budgets.
Every invocation starts with fresh counters and failure latches, including calls
made after a previous limit violation. Protected calls and coroutine resumes
must not turn a host rejection or exhausted budget into accepted effects.

Long-lived heaps also need resident-memory and aggregate process budgets.
Separate retained module/cache/state accounting from temporary callback pressure;
do not let garbage accumulate until the next unrelated call takes the blame.
Measure garbage-collection cost and schedule bounded work on execution workers,
with no collector work in the window draw path.

Specify which ordinary errors leave a realm usable and which failures require a
reset. Instruction exhaustion, memory pressure and worker failure must release
attempt contexts and partial effects. If retained module state can be partially
mutated before an error, document whether it survives or the realm resets; output
batch rollback does not automatically roll back ordinary Lua tables.

Attempt cleanup must release environments, import registry references, temporary
threads and scoped host bindings before reuse. A runtime whose isolation cannot
be restored after a failure should be discarded and rebuilt, with an explicit
reason, rather than returned to a pool in an uncertain state.

Stateless compiled-code caches may be evicted and rebuilt. Stateful realms must
not silently lose module locals through an arbitrary cache eviction. Define
admission rejection or a visible reset notification with a reason, new realm
generation and deterministic reinitialization. Reconnect, server switch, world
shutdown, package replacement and worker recovery all need explicit policies.
Cleanup must be bounded and cannot depend on a script successfully running a
finalizer. Frozen packages remain frozen during play; no hot reload is added here.

#### Proposed implementation order and acceptance

1. Inventory all shared-runner adapters and execution lanes. Record present VM
   creation/module-evaluation counts, callback latency, allocation/heap behavior
   and contention under representative action, generation and client workloads.
2. Define the realm identity, callback-class state policy, initialization/random
   contract and reset behavior. Resolve whether client callback families share
   one realm and how readonly observer state is owned.
3. Introduce worker-owned VM/compiled-code reuse while preserving authoritative
   attempt isolation. Centralize reusable setup and fresh attempt bindings; keep
   unrelated gameplay adapters in their focused modules.
4. Add retained modules to the supported client/readonly realms. Demonstrate a
   client controller/cache and manually resumed coroutine surviving callbacks,
   while reconnect and reset produce a fresh realm.
5. Audit host-context revocation, imports, RNG, budgets, memory/GC and failures.
   Deliver actionable diagnostics identifying realm, package/module, callback,
   attempt and reset reason. Finish editor definitions and the runtime reference.
6. Verify behavior, measure the resulting tradeoffs, update this document with
   exact acceptance evidence and refresh Graphify. Further server transient-state
   APIs or automatic task scheduling require their own explicit scope decision.

Acceptance needs useful regressions and runnable examples covering:

- Equivalent authoritative inputs producing equivalent effects despite unrelated
  calls, failed/retried attempts, different worker assignments or cold/warm caches.
- Generation remaining independent of chunk request order and parallel scheduling.
- Retained module locals and exports surviving the documented callbacks, with
  package/dependency/session isolation and predictable initializer counts.
- An expired saved context, stale session handle or resumed old coroutine never
  acquiring authority over the next invocation or a replacement connection.
- Fresh instruction/time/diagnostic budgets after ordinary errors and limit
  failures, and no partial host effects escaping failed callbacks.
- Heap pressure, eviction/reset and shutdown releasing registry keys, threads and
  borrowed resources without unbounded growth or hanging cleanup.
- Real nonblocking-listener actions, joins, reconnects and disconnects with an
  isolated save, proving durable rewards/inventories and existing receipts still
  behave correctly. Client examples exercise actual presentation workers.

Compare cold startup and warm callback latency separately, including p95/p99,
module evaluations, memory high-water marks, GC cost and worker contention.
Exercise mixed gameplay load so a faster microbenchmark cannot conceal worse
movement/action latency. The static world-render benchmark does not measure VM
reuse. Run relevant workspace tests, formatting, strict Clippy and fixture Luau
analysis, and inspect production UI/visual examples where behavior changes.

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

Current bounds include 32 blocks and 32 items per package; each block also
consumes an item declaration. A package can register one generator and one
owner system. The frozen snapshot permits 256 modules and 256 assets across
packages, 256 KiB per asset, and 4 MiB of aggregate discovered file bytes.

**Impact:** content-heavy mods and packages containing several independent
simulation systems encounter packaging limits quickly. Splitting packages can
work around some per-package bounds, but does not remove aggregate limits.

**Closure direction:** measure representative larger packages, support multiple
independent declarations where appropriate, and revise limits with corresponding
delivery, memory and admission budgets. Blanket removal of resource bounds is
not required to support larger mods.

Evidence: [declaration limits](SCRIPTING.md#startup-host-and-required-capabilities)
and [snapshot limits](SCRIPTING.md#runtime-delivery-and-save-compatibility).

## Smaller composability gaps

- Tags can be declared, but there is no general Luau runtime tag-query service.
- Command schemas support player targets, item keys, entity keys and counts,
  without general text/numeric arguments or aliases.
- Daylight reads and admin clock control are available only in gameplay action
  callbacks; scheduled planners and other callbacks use logical ticks.
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

1. VM lifetime and retained runtime state, using the proposed scope in section 6.
2. Larger-package composition, driven by real mods.
3. Additional motion, audio and richer presentation contracts.

Compatibility diagnostics should improve alongside those changes. Imported
models, hot reload and native fire migration remain deferred; save conversion
remains excluded during this prerelease. Filesystem/HTTP access, raw GPU access
and marketplace/CDN infrastructure are separate product decisions, not assumed
requirements for closing the gaps above.
