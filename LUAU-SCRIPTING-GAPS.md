# Luau scripting gaps

Current assessment: September 30, 2026, after Phase 8 and basic runtime tools closure.

All eight phases of the approved non-deferred modding plan are complete. That
delivers a substantial baseline for content, gameplay, generation, persistent
scheduled work, UI and authored visuals. It still leaves gaps that limit larger
mods and new game modes. This document records those gaps and their practical
impact. Section 1 records the subsequently completed basic runtime tools goal;
the remaining sections describe open gaps.

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
| Exact profile/session/avatar handles, captured directory, typed player commands, targeted notices and session kicks | Cross-profile inventory operations |
| Package-owned lifecycle state, atomic first-join rewards and profile/session timers | Ordinary gameplay access to another profile's progress and inventory |
| Validated admission/reconnect spawn proposals, runtime teleport and cosmetic replacement, frozen player rules/palettes | Per-player physics remains deferred |
| Client lifecycle callbacks, selected local public state and targeted status notices | General chat transport and hooks |
| Native and Luau readonly post-commit observers | Exactly-once notification delivery; critical rewards use durable decisions |

The remaining player-service work centers on authorized runtime operations and
gameplay access to profile state. Native and Luau post-commit observers are advisory:
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

Goal active. Identity/directory queries are committed. Lifecycle registration,
admission/spawn decisions, durable profile and session state, atomic rewards and
profile/session deadlines are implemented and verified. The second increment
passes 1,132 workspace tests, strict Clippy, formatting and typed Luau analysis.
Typed command targeting and a reconnect-scoped client roster now pass all 1,137
workspace tests, strict Clippy, formatting and typed Luau analysis, including
real-listener stale-session rejection. Committed Luau observers now pass all 1,141
workspace tests, strict Clippy, formatting and typed Luau analysis. Readonly public
changes run on the native bounded advisory lane, with inert client identities;
real-listener coverage verifies an exhausted callback cannot block a commit or
later observers. Local public profile-state transport and client ready/change/
disconnect callbacks now pass all 1,147 workspace tests, strict Clippy, formatting
and typed Luau analysis. Real-client coverage verifies panel updates/reconnect
cleanup; the authored panel preview was rendered and inspected. Additional player
operations and ordinary gameplay access to profile state remain.
Land the work in reviewable increments:

The first increment exposes exact actor profile/session identities and captured
online-player queries in gameplay action callbacks, with honest profile-claim
trust metadata. Its real-listener regression checks readonly views and rollback
after a caught forged-session operation. See the
[active player-services reference](docs/modding/PLAYER-LIFECYCLE.md) for landed APIs.

1. Player identities, captured directory, command targeting and trust metadata.
2. Lifecycle registration, profile/session state and atomic first-join effects.
3. Spawn/teleport, targeted inventory/appearance/message operations and scheduling.
4. Committed observations, client lifecycle and selected player-state delivery.
5. Integrated example, editor/runtime documentation, real-listener/reconnect/restart
   regressions, full workspace tests, formatting and strict Clippy.

Existing VM lifetime and package installation rules remain in force. Any changed
save or wire contract is versioned; no prerelease save converters are introduced.

Evidence: [gameplay events](crates/host-api/src/gameplay/handlers.rs),
[Luau gameplay bindings](src/server/script/gameplay/bindings.rs),
[gameplay services](SCRIPTING.md#gameplay-context-services),
[profile owner systems](SCRIPTING.md#durable-owner-systems),
[player admission and removal](src/server.rs),
[session player entities](src/server/entities/player.rs),
[player rules](docs/modding/PLAYER-RULES.md),
[native committed-observer contract](crates/host-api/src/gameplay/observations.rs)
and [advisory observer delivery](src/server/notifications.rs).

### 3. Dynamic UI and input

Authored UI consists of fixed JSON trees containing panels, labels, images,
buttons and single-line inputs. Callbacks can change text, visibility and local
state, but cannot create, remove or rearrange widgets. There are no authored
sliders, checkboxes, selects, tables, multiline inputs or UI animations, and no
general custom keybinding API.

**Impact:** dynamically sized inventories, recipe browsers, quest lists,
configuration screens and management interfaces are awkward or blocked. The
existing native inventory screens do not provide a general dynamic UI toolkit
for Luau authors.

**Closure direction:** support data-driven collections and widget updates,
additional controls, and declared input bindings with focus handling. Keep
callback work on presentation workers and preserve server authorization for
gameplay requests.

Evidence: [authored UI](SCRIPTING.md#client-startup-and-authored-ui).

### 4. General persistent block entities

The Rust host API has a general anchored-behavior contract. Luau exposes the
storage and machine specializations, but cannot register arbitrary anchored
behavior with its own initialization, state projection, reaction, interaction
and removal/refund policy.

**Impact:** custom devices and persistent structures must fit a storage/machine
declaration or assemble behavior from lower-level world/entity services. They
lack direct access to the general host-managed anchored lifecycle.

**Closure direction:** bind general anchored declarations and callbacks while
retaining atomic placement cost, footprint ownership, invalidation and bounded
refunds.

Evidence: [Rust anchored contract](crates/host-api/src/anchored.rs) and
[unbound interfaces](SCRIPTING.md#features-requiring-engine-work-or-native-extensions).

### 5. Flexible entities, motion and presentation

Script creatures support terrain-aware ground movement toward horizontal
targets and models built from colored cuboids. Generic gameplay entities do not
provide arbitrary motion or a creature model. There are no bound sound APIs,
imported models or custom player geometry. Client presentation offers bounded
replica windows, pose/tint overrides, sparks and embers rather than a general
scene/entity renderer.

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

Callbacks use fresh VMs and explicit persistent state. Package installations
freeze at startup; there is no hot reload or script schema migration. Several
schema/handler identities fingerprint the entire frozen installation, so even a
dependency source edit can make a save incompatible.

**Impact:** developing a larger mod can require restarts and fresh test worlds
for changes that authors would like to test against existing progress.

**Closure direction:** improve compatibility diagnostics and distinguish
behavior changes from actual persistent-schema incompatibilities where sound.
Hot reload remains deferred. Save converters are expressly excluded during
this prerelease; this gap does not authorize implementing them now.

Evidence: [save compatibility](SCRIPTING.md#runtime-delivery-and-save-compatibility)
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
- Command schemas support item keys, entity keys and counts, without general
  text/numeric arguments or aliases.
- Daylight reads and admin clock control are available only in gameplay action
  callbacks; scheduled planners and other callbacks use logical ticks.
- Client inventory/block/world/action observations are bounded ASCII summaries,
  rather than general typed replica queries.
- Machine callbacks select declared transformations; they do not expose a
  general arbitrary inventory-processing planner.
- Native item-icon callbacks and per-stack render callbacks are not Luau bindings.
- Package delivery has an in-memory cache, but no persistent disk bundle cache.
- Native fire propagation/delivery still has its explicitly deferred migration.

## Suggested priority

1. Ordinary math helpers and structured script logging.
2. Player/lifecycle hooks and public player services.
3. General anchored entities and dynamic UI/input.
4. Larger-package composition and typed replica access, driven by real mods.
5. Additional motion, audio and richer presentation contracts.

Compatibility diagnostics should improve alongside those changes. Imported
models, hot reload and native fire migration remain deferred; save conversion
remains excluded during this prerelease. Filesystem/HTTP access, raw GPU access
and marketplace/CDN infrastructure are separate product decisions, not assumed
requirements for closing the gaps above.
