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

The gameplay event contract covers block removal/placement, neighbor changes,
actions, entity ticks and pickup requests. There are no general join/leave or
chat hooks, player enumeration service, or broad runtime player-state API.
Player rules select an immutable contract at startup rather than providing
per-player runtime movement overrides.

**Impact:** server modes, quests, moderation and permissions systems cannot
fully integrate with player lifecycle through Luau. Persistent profile-owned
state exists, but does not supply these missing hooks or services.

**Closure direction:** add explicit player lifecycle events, public player
queries and authorized player operations. Separate retryable gameplay decisions
from notifications of committed outcomes. Chat and additional player mechanics
need engine contracts as well as bindings.

Evidence: [event contract](crates/host-api/src/gameplay/handlers.rs),
[gameplay services](SCRIPTING.md#gameplay-context-services) and
[player rules](SCRIPTING.md#player-rules-and-appearance).

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
