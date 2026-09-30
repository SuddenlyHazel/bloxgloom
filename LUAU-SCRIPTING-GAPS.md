# Luau scripting gaps

Current assessment: September 30, 2026, after Phase 8.

All eight phases of the approved non-deferred modding plan are complete. That
delivers a substantial baseline for content, gameplay, generation, persistent
scheduled work, UI and authored visuals. It still leaves gaps that limit larger
mods and new game modes. This document records those gaps and their practical
impact; it does not expand the approved implementation scope.

See [SCRIPTING.md](SCRIPTING.md) for the implemented Luau API and
[Phase 8 acceptance](docs/modding/PHASE-8-ACCEPTANCE.md) for verification and the
production parity audit. Some gaps are missing Luau bindings to existing Rust
services; others require new engine capabilities.

## Major gaps

### 1. Basic runtime tools

These are everyday authoring tools: calculations, diagnostics and enough
execution context to understand failures. Their absence affects mods that
already fit the supported gameplay contracts. Ordinary calculations and useful
logging should be available across server and client callback contexts.

#### What exists today

The common runner initializes Luau with its base operations and table/string
libraries. It omits the entire `math` library and explicitly removes `print`.
There is no replacement script logging API. Scripts can use `assert`, `error`
and protected calls; host failures carry package/module identity, and runtime
limits reject runaway callbacks. Editor types and Luau LSP provide static
authoring feedback.

The engine already has structured `tracing` output and a bounded background
writer configured through `RUST_LOG`. That infrastructure reports engine
activity and script failures, but scripts cannot submit their own diagnostic
records. A successful callback can therefore make a wrong decision without
giving its author a practical way to inspect why.

#### Ordinary math

The runner excludes math because its random state is not a captured host input.
That decision also excludes unrelated helpers such as rounding, minima/maxima,
square roots and trigonometry.

| Mod task | Missing convenience and consequence |
| --- | --- |
| Aim a creature or orient an effect | Trigonometric helpers and angle calculations require handwritten replacements |
| Normalize a direction or compare distances | Square-root helpers are unavailable; squared comparisons work for some tasks but do not supply normalization |
| Sample procedural patterns | Sine/cosine and rounding helpers are missing even when generation uses host-owned random samples |
| Clamp and interpolate presentation values | Authors repeat small utility implementations instead of using familiar library functions |

**Closure direction:** expose the ordinary Luau math surface, including its
useful constants, while giving randomness an explicit host contract. Keep
`math.random` / `math.randomseed` unavailable unless they are replaced with a
documented deterministic service. Use captured host random inputs where those
services already exist; a fresh VM's private random state must not determine a
retryable gameplay decision.

Document supported functions in the runtime inventory and editor definitions.
Test both a simulation calculation and a client presentation calculation through
the common runner. Math results still pass through existing host validation:
NaN, infinity or out-of-range coordinates must not enter authoritative state.
Do not claim that ordinary floating-point math provides bit-identical results
across all platforms without establishing that requirement separately.

#### Script-authored diagnostics

An author should be able to record a branch decision, recipe selection, state
transition or unexpected input without deliberately failing the callback.
Restoring native `print` alone would bypass the existing structured logging and
background writer.

**Closure direction:** provide a familiar logging interface backed by `tracing`,
with trace/debug/info/warn/error severity, a message and optional simple typed
fields. A `print`-style convenience could route through the same implementation.
The host should attach package/version, the executing module, callback kind and
client/server side. Attach an action or owner identity when available. Imported
helper diagnostics should retain the helper's source identity and the invoking
package's context.

Formatting and emission belong on the callback worker path, using the existing
background writer. Define bounds for record count, message/field bytes and field
types so logging does not become unbounded formatting or queue work. Avoid
implicit deep serialization of tables or mutable engine objects. Report
suppressed diagnostics without flooding the output, and let normal filtering
control verbosity. These are diagnostics budgets, not gameplay authority.

#### Retries, failure and commit semantics

A gameplay callback may run again after a stale read or admission deferral.
It may also stage effects and then fail. Logging needs an explicit policy for
both cases:

- Diagnostic records describe an execution attempt. They do not prove that an
  inventory transfer, world edit or other staged effect committed.
- Retain useful attempt diagnostics when execution fails. Buffering everything
  until gameplay success would discard the messages needed to diagnose failure.
- Include correlation and outcome information when available, and document that
  retries can repeat messages. Do not promise exactly-once logging.
- If authors need notification that an effect committed, provide that through
  a committed-outcome service; ordinary logging must not impersonate it.
- Log filtering or a saturated diagnostic queue must not change the gameplay
  plan. Logging cannot become a hidden source of world/inventory mutations.

The same distinction matters in client presentation: a callback producing a
visual command does not prove the command was installed or displayed.

#### Useful failure reporting

Preserve existing source attribution and expose the difference between a script
error, invalid host operation, unavailable input, stale dependency and resource
limit where the host knows it. Include source locations/tracebacks when
available. Caught invalid host operations already poison gameplay plans; the
resulting rejection should remain diagnosable even if the script used `pcall`.

Keep timing and budget diagnostics in host tooling. A script-visible wall clock
is not required for profiling and would introduce another uncontrolled input to
retryable simulation. Existing interrupt checks are not an exact bytecode
instruction count and should not be labeled as one.

#### Completion criteria and implementation order

1. Enable ordinary math in the shared runner, with the randomness policy
   documented and checked.
2. Add the logging bridge and editor definitions, using existing filtering and
   worker output. Exercise it in one server and one client fixture.
3. Verify the meaningful edge cases: a helper module's attribution, a failed
   callback retaining diagnostics, a retried action repeating attempt records,
   and logging saturation leaving authoritative results unchanged.
4. Document the supported API, limits and execution-versus-commit semantics.

This is the highest-priority gap because it improves nearly every mod without
requiring a new game mechanic. It does not depend on a live debugger, hot reload,
filesystem access or imported models. The API shape above is a proposed closure
direction; math and script logging remain unimplemented today.

Evidence: [common runner and sandbox](src/server/script.rs),
[runtime regressions](src/server/script/tests.rs),
[engine logging](src/logging.rs) and
[runtime inventory](SCRIPTING.md#runtime-delivery-and-save-compatibility).

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
