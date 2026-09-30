# Proposal: one coherent gameplay API

> **Archived September 30, 2026.** Historical snapshot, not an active plan or
> current API reference. Status, limits and instructions below reflect the time
> of writing. Start at [current modding docs](../../../docs/modding/README.md),
> [Phase 8 acceptance](../../../docs/modding/PHASE-8-ACCEPTANCE.md) and
> [current Luau gaps](../../../LUAU-SCRIPTING-GAPS.md).

**Consolidated successor:** [current implementation plan](IMPLEMENTATION-PLAN.md)
is the single proposal for approval and full implementation. This document is
retained as design background; use the successor's scope and continuation record.

Status: **Proposed**. This document records the proposed direction; it does not
claim implementation or approval to begin implementation. The likely runtime is
**Luau embedded through `mlua`**, but that selection is not final. The product
requirements below are user-specified directions for this proposal.

The existing implementation and completion tracker remain in
[`MODDING-SURFACE-PLAN.md`](MODDING-SURFACE-PLAN.md). This proposal reframes the
remaining capability gaps around a shared gameplay layer rather than a separate
specialized API for every subsystem.

## Goal

Let mod developers build gameplay—not assemble declarations for a growing list
of narrowly defined engine features.

The current surface is useful, but leans too heavily toward configuring something
the engine already understands. To build the game for real, we need a general way
to **read the world, make decisions, change it, and react to what happens**.

Make that the foundation. Keep the existing storage, machine, creature, and UI
APIs as convenient building blocks.

## 1. A small, powerful set of concepts

### Define

Register blocks, items, entities, assets, commands, generation functions, and
screens. Use familiar definitions with sensible defaults. A simple block should
be simple to register; custom behavior should not require implementing an
inventory machine or another unrelated abstraction.

### Read

Query blocks, entities, players, inventories, and mod-owned data through a
consistent world context. Use stable handles and namespaced identifiers.

Make common queries straightforward: inspect a block, find nearby entities, read
an inventory, or determine whether a location is clear.

### Change

Provide general operations for:

- Setting block states.
- Spawning, updating, and removing entities.
- Giving, taking, and transferring items.
- Creating and collecting world drops.
- Updating mod-owned state.

These operations must compose. An interaction that consumes an item, changes a
block, and spawns an entity should be **one ordinary operation**, not three
disconnected APIs with different persistence behavior.

### React

Register handlers for meaningful gameplay events: use, break, placement,
interaction, support changes, player lifecycle, and so on.

Clearly distinguish:

- **Decision handlers**, which can change or reject an operation.
- **Notifications**, which run after something has happened.

### Schedule

Run behavior on ticks, after a delay, or when a relevant event occurs. Persistent
scheduled work should survive restart without the mod author implementing
recovery machinery.

Most gameplay should fit naturally into these five concepts.

## 2. Atomic operations are a convenience, not a burden

The engine requires coordinated changes because inventories, world state,
persistence, and replication must agree. That responsibility belongs to the host.

A future scripting interface could feel approximately like this:

```lua
function onUse(ctx)
    local inventory = ctx.player.inventory

    if not inventory:contains("example:seed") then
        return
    end

    inventory:take("example:seed", 1)
    ctx.world:setBlock(ctx.target, "example:seedling")
    ctx:schedule("example:grow", 200, {
        position = ctx.target,
    })
end
```

This is illustrative, not a commitment to Luau or these exact names.

The author writes a coherent gameplay operation. The host records the reads and
proposed changes, validates them, and commits them together. If something
conflicts, the host handles the retry or returns a useful failure.

**Mod developers should never manually construct journal records, collect chunk
revision stamps, or orchestrate replication.**

## 3. Specialized APIs are optional conveniences

Keep the current machine, storage, and creature contracts as useful shortcuts:

- A declarative recipe is easier than writing a processing loop.
- A standard storage definition is easier than building a container.
- Host movement and navigation are easier than implementing collision.

These helpers should sit **on the shared gameplay surface**, not define the outer
limit of what a mod can do.

Making a machine with unusual behavior should not require another `Operation`
enum variant every time. It should be able to use the same world, entity,
inventory, and scheduling operations available to other handlers.

## 4. Built-in gameplay uses the same boundary

Adopt this architectural rule:

> Engine code implements mechanisms. Game code—including our built-ins—uses the
> public gameplay surface.

The engine owns storage, networking, collision execution, rendering resources,
scheduling, and transaction mechanics.

Gameplay code decides what leaves drop, how a creature behaves, what a command
does, how a block grows, and which rules apply to a player.

Migrate the remaining privileged gameplay paths onto that boundary. If an
ordinary built-in feature requires an internal shortcut, improve the public
surface or explicitly identify the capability that is still engine-only.

Future game development should expand the modding surface naturally, rather than
requiring a separate "make this moddable" project afterward.

## 5. Distinguish genuinely different execution contexts

| Context | Purpose |
| --- | --- |
| Startup | Register definitions, assets, dependencies, and handlers |
| Server gameplay | Read authoritative state and perform gameplay operations |
| World generation | Generate bounded regions from deterministic inputs |
| Client presentation | Read replicated state and produce UI, animation, and supported visual effects |

These contexts should share naming, handles, data conventions, and error behavior.

A rendering callback cannot own an inventory change. A generation callback cannot
depend on which player happens to be connected. These distinctions make the API
understandable; they should not become dozens of unrelated mini-frameworks.

## 6. Developer experience is part of the implementation

A capable API that is unpleasant to use is not finished. Include:

- Consistent names and behavior across blocks, entities, players, and items.
- Clear ownership and composition rules when multiple mods affect the same
  operation.
- Errors that identify the mod, handler, content key, and relevant operation.
- Straightforward versioned persistence for mod-owned data.
- Small examples taken from actual built-in gameplay.
- Timing and budget diagnostics that explain expensive handlers.
- Documented limits and explicit failure results—never silent truncation or
  dropped effects.

Custom shaders, textures, and authored UI are part of the intended surface, not
exceptions reserved for engine developers. The host must expose useful rendering
integration points rather than restricting mods to a fixed list of visual presets.
New physics models or facilities such as audio still require engine support.
Ordinary gameplay combinations should not require engine changes merely because
we did not anticipate them.

## 7. Server-delivered mod packages

Joining a modded server should give the client everything required to play on
that server. Players should not have to manually assemble a matching mod folder.
Design for Luau scripts embedded through `mlua`, while keeping the gameplay host
contracts independent of the eventual language binding.

A package should distinguish server-only, client-only, and shared modules/assets.
The server advertises the exact required client package set and makes the required
bytes available: client/shared scripts, content definitions, UI documents/styles,
textures, shaders, and eventually models. Server-only implementation and private
world data do not belong in that client bundle.

The intended connection flow is:

1. Exchange engine/protocol and runtime compatibility information and a package
   manifest containing identities, versions, dependencies, and content hashes.
2. Reuse matching cached content and obtain missing package data from the server's
   delivery mechanism. Show download/preparation progress and useful errors.
3. Resolve the client package set, run startup registration, prepare required
   assets, and construct the session's frozen catalog and client runtime.
4. Complete catalog compatibility/readiness before entering authoritative play.

Downloaded code runs through the embedded runtime's explicit host API. Package
resources resolve through the package namespace/cache. Client scripts handle
presentation and requests; authoritative gameplay continues on the server.
Disconnecting or switching servers must retire the session's runtime and resources
without mixing incompatible package sets; immutable cached bytes can be reused.

This requires extending the current handshake. Today's preinstalled Rust fixture,
catalog fingerprint check, and manifest remapping do **not** implement package
delivery or dynamic client registration. Startup freezing remains useful, but the
client's modded catalog must be constructed for the negotiated server session.
Delivery transport, package format, and Luau source/bytecode compatibility are
design choices to settle when implementing this milestone.

## 8. Build UI on existing Rust work

Both our game and mods need a real UI/GUI authoring surface: layout, styling,
images/fonts, scrolling, controls, text input, focus, and script event handlers.
The existing label/button/inventory descriptors remain conveniences, not the
ceiling for authored interfaces.

Investigate established Rust UI/rendering work before building another layout,
text, or styling engine. HTML/CSS or a familiar CSS-style authoring model is a
candidate, not a selected dependency.

Initial candidates checked against their project documentation:

- [Blitz](https://github.com/DioxusLabs/blitz): a modular HTML/CSS rendering
  engine with an existing wgpu texture integration example. Its project currently
  describes it as beta. The high-level HTML wrapper does not itself provide the
  full interactive scripting bridge we need; DOM events and Luau integration
  would need evaluation. Using its native renderer need not imply adopting a
  JavaScript gameplay runtime.
- [Taffy](https://github.com/DioxusLabs/taffy): Rust block/flexbox/grid layout,
  used by other UI systems. It is a layout component, not a complete HTML/CSS
  renderer, text system, widget library, or input implementation.

Choose based on embedding into the existing renderer/window, authoring experience,
text/input quality, dynamic updates, custom game widgets and performance. The
same authoring/event path should serve built-in and modded interfaces. UI files,
styles, fonts, images and scripts belong in server-deliverable client packages.

## 9. Custom shaders and visual assets

Mods must be able to ship their own shader code, textures and material definitions.
Existing PNG and lighting-property registration is a foundation, not completion of
this requirement.

Define shader/material registrations with explicit texture/uniform inputs and
stable host-provided interfaces for relevant geometry, transforms, lighting and
time. Provide a path for custom material shading and registered visual effects /
render passes, including how multiple mods' effects compose. WGSL is a natural
candidate for the current wgpu renderer; the exact shader interface and supported
stages still need design.

The renderer owns GPU resources and scheduling. Mods supply shader programs and
their data through this surface, with useful compilation errors attributed to the
package and asset. Shader/pipeline preparation must fit the asynchronous asset
path so joining or loading effects does not stall the window. Required graphics
capabilities participate in package readiness and compatibility.

Custom visual code remains presentation: an effect cannot determine whether an
item was picked up or a world edit committed. This separation should not restrict
the author to recoloring a fixed collection of built-in shaders.

## 10. Custom models: required direction, deferred work

Mods should eventually ship their own models. **Do not implement a model-import
API now.** First decide the game's long-term modeling and authoring workflow:

- How block-like, free-form and animated objects are authored.
- Tools and source/interchange formats.
- Coordinate conventions, scale, pivots and attachment points.
- Material/shader association and animation/rigging needs.
- How visual geometry relates to collision and selection.
- Runtime representation, batching and asset preparation.

Choose these for the game and mods together. Then expose the resulting model
pipeline through the same package and asset registration path. The existing
registered cuboid models remain useful, but are not the final custom-model system.

## Implementation sequence

1. **Design the shared gameplay context and operation model.** Inventory existing
   operations and unify their semantics before adding more specialized public
   APIs.
2. **Implement general world/entity/item mutations over the existing transaction
   path.** Reuse authority, persistence, replication, and fairness. Avoid a second
   execution architecture.
3. **Add coherent event handling and persistent scheduling.** Connect existing
   callbacks and owner systems to that shared model.
4. **Migrate remaining built-in gameplay.** Harvest/loot, drop policies, applicable
   world behavior, player rules, and commands. Fire remains separately deferred
   until authorized.
5. **Finish generation and client-facing integration.** Expose their genuinely
   different execution contexts without inventing a separate vocabulary for each.
6. **Close usability gaps and document the boundary.** Review actual gameplay
   implementations for awkward workarounds, missing operations, and built-in-only
   access.

The host work must accommodate the server-delivered client packages, richer UI,
and shader interfaces described above. Runtime/package delivery is a separate
implementation milestone, not an optional part of the intended player experience.
UI-library selection and shader interface design require focused evaluation;
custom models remain deferred until the modeling workflow is decided.

## Intended outcome

**A public gameplay layer on which both we and mod developers can build the game.**
