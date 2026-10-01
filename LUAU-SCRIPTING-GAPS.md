# Luau scripting gaps

Current assessment: October 1, 2026, after Phase 8, runtime tools, player services,
dynamic UI, anchored entities, typed client replicas and the VM lifetime work
recorded in section 6.

All eight phases of the approved non-deferred modding plan are complete. That
delivers a substantial baseline for content, gameplay, generation, persistent
scheduled work, UI and authored visuals. It still leaves gaps that limit larger
mods and new game modes. This document records those gaps and their practical
impact. Sections 1–4 record the completed runtime tools, player-services,
dynamic UI and general anchored-entity goals. Typed client observations are also
complete within their stated scope. VM reuse and retained advisory/client state
are implemented in section 6; save continuity remains open. Their limitations
and the other sections remain open gaps.

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

**Status: proposed scope, ready for review; not implemented.**

#### Proposed goal

Let one Luau package contain a substantial content set and several independently
named simulation systems and terrain contributors. Preserve deterministic
registration/generation, frozen catalogs, finite inventories and transactional
owner state. Deliver a runnable farming package, measured capacity limits,
actionable admission diagnostics and real-listener acceptance coverage.

The intended author experience is one package with separate `farm:irrigation`,
`farm:growth` and `farm:seasons` systems, plus separate terrain contributors for
wild crops and groves. Authors should not have to combine those callbacks into
one dispatcher or invent extra packages solely to bypass registration limits.

#### Actual problems and existing foundation

- Startup currently allows 32 blocks, 32 items and 32 textures per package.
  Every block consumes an item slot, so 32 blocks leave no room for standalone
  seeds, tools or produce. These are Luau adapter bounds; the native catalog has
  much larger installation-wide ceilings.
- `Pending` stores one optional system and one optional generator. Their Luau
  declarers reject a second registration, although the native system registry
  supports 256 systems and generation supports 256 contributors.
- Discovery allows 64 modules and 64 assets per package, 256 of each across the
  installation, 64 KiB per source, 256 KiB per general asset and 4 MiB of total
  discovered files. Manifests are capped at 16 KiB.
- Client declaration decoding repeats the small content caps, bundle decoding
  repeats the file/count caps, and the current bundle byte ceiling is 7 MiB.
  Raising only server discovery or registration would leave valid server content
  impossible to deliver or install on clients.
- More declarations increase startup work, compiled modules, decoded assets,
  owner-job demand and generation cost. Larger files alone do not establish that
  a package fits execution, memory, renderer or network admission budgets.

#### Registration and composition contract

Keep the existing Lua signatures. Repeated calls with distinct owned keys become
valid; each declaration retains its own callback, schema/revision, seeds and
resource-access contract. For example, this proposed registration form uses
existing API fields:

```luau
host.register_generator("farm:10_wild_crops", 1, "farm:wild_crops")
host.register_generator("farm:20_groves", 1, "farm:groves")

-- Each table also supplies the existing required schema, revision, seeds,
-- state and resource fields; these are separate complete declarations.
host.register_system(irrigation)
host.register_system(growth) -- growth.after = {"farm:irrigation"}
host.register_system(seasons)
```

Implementation decisions:

- Replace singleton pending fields with bounded collections, reject duplicate
  keys, and canonicalize registrations before installing them. Manifest line
  order and startup call order must not determine numeric IDs or execution order.
- Resolve system `after` edges for both same-package and permitted direct-
  dependency systems using the existing native phase planner. Reject missing
  targets, undeclared dependency access, self-edges and cycles before world open.
  Do not infer an execution edge from the order of `register_system` calls.
- Keep owner state, deadlines, receipts and intents isolated by system key and
  owner partition. Sharing a callback module must not merge two systems' state.
  Existing wake routing remains available; durable intents remain same-system
  messages. This goal does not add arbitrary cross-system state access or RPC.
- Keep native generation semantics: builtin terrain first, then contributors in
  lexical namespaced-key order; later contributors win overlapping writes.
  `10_`/`20_` names make order deliberate within the example package. Contributors
  still sample builtin terrain, not another contributor's partial output. A
  failed contributor discards the whole candidate chunk. No new priority or
  generator dependency API is proposed for this milestone.
- Authoritative callbacks keep isolated attempts and deterministic RNG. Reuse
  physical VMs/compiled code from the completed lifetime work; do not allocate a
  persistent VM, thread or queue for every new declaration.
- Retain native installation-wide catalog and system/contributor ceilings.
  Validate the combined installation, including builtin declarations, rather
  than assuming every package can simultaneously consume its local maximum.

#### Capacity targets for review

These are **initial implementation targets**, not measured promises. Establish
baseline costs, exercise the target installation, and record any revised values
with the reason before declaring the goal complete. Put authoritative limits in
shared policy definitions consumed by discovery, registration, encoding and
client verification rather than copying literals between those paths.

| Resource | Today | Proposed target |
| --- | --- | --- |
| Blocks / package | 32 | 256 |
| Total items / package, including block items | 32 | 512 |
| Textures / package | 32 | 256 |
| Owner systems / package | 1 | 8 |
| Generation contributors / package | 1 | 8 |
| Modules / package / installation | 64 / 256 | 256 / 1,024 |
| Assets / package / installation | 64 / 256 | 256 / 1,024 |
| Manifest bytes / package | 16 KiB | 64 KiB |
| Source bytes / module | 64 KiB | 64 KiB |
| General asset bytes / file | 256 KiB | 2 MiB |
| Total discovered file bytes / installation | 4 MiB | 32 MiB |
| Encoded client bundle bytes | 7 MiB | 40 MiB, including metadata |

The item target permits 256 block items plus 256 standalone items. Keep both the
local total and actual installation-wide item/state/texture capacities explicit
in diagnostics. Asset-kind-specific source, image dimension, decoded pixel,
font expansion and shader validation limits remain separately enforced; the
2 MiB general ceiling does not automatically enlarge those limits.

Keep the 64-package bound, direct exact dependencies, frozen installation model,
128-item stack cap and current per-call gameplay limits. Increasing module count
also does not increase import depth or retained client realm admission limits.

#### Execution, delivery and memory budgets

- Give declaration-heavy startup its own bounded initialization policy instead
  of silently increasing every gameplay callback's limits. Initial proposal:
  250 ms, 50,000 periodic instruction checks and 16 MiB per package startup,
  plus a 10-second installation initialization deadline. Reuse the aggregate VM
  reservation ledger; startup authority still ends at completion.
- Keep existing per-system owner-job/effect/read/state limits and native summed
  phase admission checks. Eight systems cannot each obtain an unlimited share
  of a tick. Measure contention, conflict retries and unrelated player progress.
- Add a shared budget across scripted contributors for each candidate chunk,
  initially 100 ms of script execution with each invocation bounded by its
  remaining allowance and existing instruction/output limits. Check all error
  paths against whole-candidate rollback. Measure this separately from builtin
  terrain, queue delay, meshing and delivery before accepting the target.
- Audit bundle offer lengths, transfer chunking, queue admission, cancellation,
  hashing, verified in-memory caching and all decode limits together. Preserve
  server/client catalog agreement and fail unsupported budget/codec profiles
  during negotiation, before a partial download or `ContentReady`.
- Bound preparation and decoded resources independently of encoded file size.
  Initially allow 10 seconds for client source validation, while retaining its
  16 MiB compiler-VM reservation and off-window-thread execution. Account for
  temporary download/verification buffers and cached bundle bytes so increasing
  the payload limit does not multiply untracked copies per joining player.
- Record encoded bytes, peak transfer/verification memory, decoded asset memory,
  GPU resource use, VM reservations/cache occupancy, preparation latency and
  steady movement/action latency. Reuse existing bounded delivery machinery;
  introduce additional admission only where the audit shows a missing bound.

#### Author diagnostics and compatibility

For count, byte, schema and phase-budget failures report the owning package and
module/declaration or asset path, the resource, attempted usage, maximum and
whether the limit is local or installation-wide. Example:

```text
farm@1.0.0:main: register_block farm:crop_257 rejected
blocks/package: attempted 257; maximum 256
```

For dependency errors show the involved system keys and a missing edge or cycle;
for delivery/preparation failures show the stage and verified bundle identity.
Caught startup errors must still reject the complete installation, with no
partial catalog or world/save mutation.

Keep existing single-registration packages valid. Canonical ordering preserves
stable identities when only startup declaration order changes. Added or changed
persistent declarations still follow existing compatibility checks. Improve those
error explanations in touched paths without broadening into installation-wide
fingerprint redesign, hot reload or save converters. If a required format change
is incompatible, bump the prerelease format/world target and explain it.

#### Runnable example and implementation sequence

1. **Inventory and measurement:** map every server/client limit and enclosing
   wire/renderer budget. Capture the existing combined mod's cold startup, join,
   memory and steady latency. Build deterministic generated boundary fixtures
   to exercise count/byte admission without presenting them as ordinary gameplay.
2. **Multiple declarations:** implement bounded system/generator collections,
   ownership/duplicate checks, canonical ordering and dependency validation.
   Land a small same-package example first, preserving existing callback APIs.
3. **Coordinated capacity:** implement shared limit policy, matching encode/decode
   and negotiation changes, startup/generation/preparation budgets and measured
   memory accounting. Verify old-size packages and all boundary rejection paths.
4. **Representative mod:** add `fixtures/farming-scale/` with at least 128 blocks,
   192 total items, several texture families, 96 modules, three independent
   systems and two contributors. Irrigation wakes crop growth; seasons use their
   own state/deadlines. Include a client panel and finite harvest/planting actions.
   Separate generated pressure fixtures must cross the old module/asset/4 MiB
   installation ceilings and exercise the proposed limits with valid content.
5. **Acceptance and author docs:** run the real listener with isolated saves,
   inspect client presentation, publish measured results and final limits, update
   `SCRIPTING.md`, package/system/generation references and editor definitions,
   analyze Luau fixtures, run relevant workspace/fmt/Clippy checks, refresh the
   code graph and commit the finished work incrementally.

#### Completion criteria

- One package registers at least three independently keyed systems and two
  contributors. Same-package and direct-dependency phase edges work; duplicate,
  missing, forbidden and cyclic declarations fail before world open.
- Systems sharing a module retain separate owner state and durable scheduling.
  Cross-partition conflicts retry without partial edits, duplicate effects or
  inventory creation. Restart recovers each system's state and deadlines.
- Reordered declaration calls produce identical catalog identities and generator
  output. Overlap precedence, negative/chunk-seam coordinates, worker assignment,
  cold/warm attempts and failed-contributor rollback are covered.
- Target-size content passes discovery, encoding, negotiation, download, client
  verification and restart. Maximum-plus-one counts/bytes, metadata overflow,
  decoded expansion, aggregate resource excess and corrupted transfers reject
  atomically with actionable source-attributed diagnostics.
- Multiple joining/cancelled/reconnecting clients cannot starve an established
  player's movement or durable actions. Measure cold/cached join p50/p95/p99 and
  action/movement tails, memory and queue behavior against the recorded baseline.
  Regressions require explanation and correction before acceptance; no universal
  hardware-independent latency promise is inferred from a microbenchmark.
- Inspect the farming panel and representative registered art through the actual
  release client when available, otherwise production GPU previews. Run renderer
  performance checks if textures/materials/meshing paths change, separating
  scene setup, geometry, CPU and GPU time from VM/join measurements.

Motion/audio, imported models, tag-query services, command-schema expansion,
persistent disk bundle caching, marketplaces/CDNs and hot reload remain separate
projects. No implementation goal is active for this proposal until reviewed.

Evidence: [startup collection](src/server/script/startup.rs),
[system binding](src/server/script/system.rs),
[generator binding](src/server/script/generation.rs),
[native phases and budgets](src/server/registry.rs),
[native generation ordering](src/world/generation.rs),
[snapshot limits](src/server/script/package.rs),
[client bundle verification](src/server/script/package/client.rs) and
[client declaration decoding](src/server/script/package/client/declarations.rs).

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

1. Larger-package composition, driven by real mods.
2. Additional motion, audio and richer presentation contracts.

VM lifetime and retained runtime state are implemented in section 6.

Compatibility diagnostics should improve alongside those changes. Imported
models, hot reload and native fire migration remain deferred; save conversion
remains excluded during this prerelease. Filesystem/HTTP access, raw GPU access
and marketplace/CDN infrastructure are separate product decisions, not assumed
requirements for closing the gaps above.
