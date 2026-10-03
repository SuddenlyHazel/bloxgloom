# Modding: start here

**Current status:** All eight phases of the approved non-deferred modding
implementation are done. The
[Phase 8 acceptance record](PHASE-8-ACCEPTANCE.md) contains the production parity
audit and integrated verification evidence. [SCRIPTING.md](../../SCRIPTING.md)
is the implemented Luau API inventory. Native fire migration remains deferred.
Packaged player/creature GLB models, region hooks, chat, runtime modifiers and
manual development reload are available. The native Rust extension API has
additional interfaces that are not Luau bindings.
[Current Luau gaps](../../LUAU-SCRIPTING-GAPS.md) records the limitations and their
impact on larger mods.

## Try authoring now

- [Read services](READ-SERVICES.md): runtime tags, captured daylight/weather and bounded terrain queries.
- [Commands](COMMANDS.md): bounded text/numeric arguments and canonical aliases.
- [Custom machine processing](MACHINE-PROCESSING.md): atomic exact-stack transformations.
- [Item visuals](item-visuals.md): bitmap icons and per-stack presentation callbacks.
- [Native GLB models](AUTHORED-MODELS.md): authored clips, appearance layers and color controls.
- [Player world rules](PLAYER-WORLD.md): transactional region hooks and bounded moderated chat.
- [Player modifiers](PLAYER-MODIFIERS.md): profile/session movement effects with optional expiry.
- [Packaged player models](PLAYER-MODELS.md): native character selection, named looks and baked clips.
- [Packaged GLB creatures](GLB-CREATURES.md): verified model delivery, replicated
  appearance and clip controls, with a runnable multiplayer fixture.

- [Package composition](PACKAGE-COMPOSITION.md): larger content budgets, multiple
  owner systems and contributors, with the runnable
  [farming-scale package](../../fixtures/farming-scale/README.md).

- [Combined Jade garden package](../../fixtures/combined-mod/README.md): one
  runnable Luau package with a placeable textured cube, authorized action,
  scheduled growth, authored UI, downloaded startup text and WGSL material.
  It also demonstrates the authenticated public daylight command.
- [Local Luau packages](LUAU-PACKAGES.md): runnable
  [`uidemo`](../../fixtures/packages/uidemo/) example, manifest, command line,
  and current server/client bindings. Its button now sends an authorized
   registered action and displays the server's receipt; its text input remains
   local presentation state.
- [Targeted authored-UI action](../../fixtures/ui-target-actions/README.md):
  a client-aimed block request with server-owned reach, cost and receipt.
- [Persistent counter block](../../fixtures/anchored-counter/README.md): general
  anchored placement, private/public state, interaction, neighbor reactions and
  atomic removal/refunds. See the [Luau anchored API](ANCHORED-ENTITIES.md).
- [Neighborhood owner system](../../fixtures/neighborhood/README.md):
  authoritative cross-chunk reads/edits and a durable intent relay in Luau.
- [IDE setup](IDE.md): opt-in Luau LSP settings and editor-only types for the
  currently bound callbacks; runtime validation remains authoritative.
- [Runtime tools](RUNTIME-TOOLS.md): standard libraries, deterministic native
  randomness, coroutines and structured attempt diagnostics on server and client.
- [VM lifetime](VM-LIFETIME.md): reused execution infrastructure, isolated
  authoritative attempts, retained client/observer module state and reset rules.
- [Generation](GENERATION.md): deterministic chunk contributors and seams.
- [Player rules](PLAYER-RULES.md): startup-selected, negotiated body, movement,
  spawn and eye contract.
- [Dynamic UI and input](DYNAMIC-UI.md): runtime collections, new controls,
  stable focus and declared rebindable actions, with a real recipe browser.
- [Typed client replicas](TYPED-REPLICAS.md): readonly local inventory, installed
  block observations, sampled world time and package-owned action receipts.
- [Authored UI foundation](UI-FOUNDATION.md): renderer/layout decision and
  current limitations.
- [First WGSL effect example](../../fixtures/effect-packages/sepia/README.md):
  the compatible version-1 fullscreen scene-color effect.
- [Jade material example](../../fixtures/material-packages/jade/README.md):
  package-delivered WGSL albedo for an existing voxel texture layer.

- [Authored visuals](AUTHORED-VISUALS.md): versioned material hooks, typed Luau
  parameters, effect graphs, and the runnable Prism example.

## Rust extension and host reference

[Content](REGISTERED-CONTENT.md) · [Actions](REGISTERED-ACTIONS.md) ·
[Inventories](REGISTERED-INVENTORIES.md) · [Machines](REGISTERED-MACHINES.md) ·
[Owner systems](REGISTERED-SYSTEMS.md) · [Host lifecycle](HOST-LIFECYCLE.md) ·
[Dynamic entities](DYNAMIC-ENTITIES.md) · [Anchored behaviors](ANCHORED-BEHAVIORS.md).

These references describe specific supported surfaces. Use the implemented
Luau inventory and current gaps document to distinguish native Rust interfaces
from Luau bindings; older slice-level completion notes describe their own scope.

## Historical design and audit

The [archive](../archive/README.md) contains the completed implementation plan,
superseded proposals, foundation plans and original baseline parity audit.
They preserve design decisions and historical results; they are not active
implementation instructions or a current list of missing capabilities.

[Package development and persistent caching](PACKAGE-DEVELOPMENT.md) describes the native manual reload command and validation. [Save compatibility](SAVE-COMPATIBILITY.md) explains behavior-only restarts, generation protection and rejection diagnostics.
