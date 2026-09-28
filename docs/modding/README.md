# Modding: start here

**Current status:** The [implementation plan](IMPLEMENTATION-PLAN.md) is the
authoritative progress and acceptance record. Only generation (Phase 3) is
marked done. Other committed increments are useful, but do **not** imply full
modding support. See its [phase table and end-to-end gap ledger](IMPLEMENTATION-PLAN.md#12-implementation-order-and-deliverables)
before treating a capability as complete.

## Try authoring now

- [Local Luau packages](LUAU-PACKAGES.md): runnable
  [`uidemo`](../../fixtures/packages/uidemo/) example, manifest, command line,
  and current server/client bindings. Its UI changes are *local only*; buttons
  cannot yet send authoritative server requests.
- [Generation](GENERATION.md): deterministic chunk contributors and seams.
- [Authored UI foundation](UI-FOUNDATION.md): renderer/layout decision and
  current limitations.
- [First WGSL effect example](../../fixtures/effect-packages/sepia/README.md):
  one bounded fullscreen scene-color effect, **not** the complete material or
  multi-pass shader surface required by Phase 7.

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
