# Modding: start here

**Current status:** The [implementation plan](IMPLEMENTATION-PLAN.md) is the
authoritative progress and acceptance record. Phases 1–7, including server
packages/joining, authored UI and authored visuals, are marked done. Integrated
verification (Phase 8) remains open; completed phases do **not** imply full
modding support. See its [phase table and living checklist](IMPLEMENTATION-PLAN.md#12-implementation-order-and-deliverables)
before treating a capability as complete.

## Try authoring now

- [Combined Jade garden package](../../fixtures/combined-mod/README.md): one
  runnable Luau package with a placeable textured cube, authorized action,
  scheduled growth, authored UI, downloaded startup text and WGSL material.
  It is an integrated example, **not** a claim that all phases are done.
- [Local Luau packages](LUAU-PACKAGES.md): runnable
  [`uidemo`](../../fixtures/packages/uidemo/) example, manifest, command line,
  and current server/client bindings. Its button now sends an authorized
   registered action and displays the server's receipt; its text input remains
   local presentation state.
- [Targeted authored-UI action](../../fixtures/ui-target-actions/README.md):
  a client-aimed block request with server-owned reach, cost and receipt.
- [Neighborhood owner system](../../fixtures/neighborhood/README.md):
  authoritative cross-chunk reads/edits and a durable intent relay in Luau.
- [IDE setup](IDE.md): opt-in Luau LSP settings and editor-only types for the
  currently bound callbacks; runtime validation remains authoritative.
- [Generation](GENERATION.md): deterministic chunk contributors and seams.
- [Player rules](PLAYER-RULES.md): startup-selected, negotiated body, movement,
  spawn and eye contract.
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

These references describe narrower supported surfaces. Where an older page
calls its own slice “complete,” that does **not** override the current plan's
phase status or imply all built-in behavior is available to mods.

## Historical design and audit

The [history index](history/README.md) contains superseded proposals, the
earlier surface plan and the original baseline parity audit. They explain why
the system was designed this way, but are **not** current implementation or
completion status. General game/renderer/performance docs remain in `docs/`
outside this folder.
