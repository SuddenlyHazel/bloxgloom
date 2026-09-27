# Proposal: one coherent gameplay API

Status: **Proposed**. This document records the proposed direction; it does not
claim implementation or approval to begin implementation. Runtime selection,
including possible Luau adoption, remains undecided.

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

This does not promise arbitrary access to every engine capability. New rendering
techniques, physics models, or audio facilities still require engine support.
Ordinary gameplay combinations should not require engine changes merely because
we did not anticipate them.

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
   access. Runtime selection and loading remain separate.

## Intended outcome

**A public gameplay layer on which both we and mod developers can build the game.**
