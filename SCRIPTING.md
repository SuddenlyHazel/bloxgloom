# Scripting capabilities for mod developers

This guide describes the implemented Luau package surface as of September 29,
2026. It covers server scripting, client presentation, and the assets scripts
can use. It does not imply complete parity with built-in gameplay or the broader
native Rust extension API. The [implementation plan](docs/modding/IMPLEMENTATION-PLAN.md)
tracks remaining integration work.

## Start a package

Put each package in its own immediate subdirectory of a package root. The
directory name must match `package` in its `package.txt`.

```text
my-packages/
  garden/
    package.txt
    server/main.luau
    assets/textures/brick.png
```

```text
format 2
package garden
version 1.0.0
entry main
requires bloxgloom:content/v1
module server main server/main.luau
asset texture brick assets/textures/brick.png
```

```luau
return function(host)
    host.register_texture("garden:brick", "brick")
    host.register_block("garden:brick", "Garden Brick", "garden:brick")
    host.register_item("garden:token", "Garden Token", "garden:brick")
end
```

Start a local game, using a fresh save directory for this package set:

```sh
cargo run --release -- local-packages my-packages ./world-garden-doc-example
```

Or start a server and client in separate terminals:

```sh
cargo run --release -- server-packages my-packages 127.0.0.1:4000 ./world-garden-doc-example
cargo run --release -- client 127.0.0.1:4000
```

The optional final server argument is `max-clients`, from 1 through 256.
The server runs every package's entry, including dependency entries, before
opening the world. Library packages can use a no-op entry and export helpers
from other modules. Content registration is frozen before play.

## What packages can build

| Area | Available to Luau packages |
| --- | --- |
| Content | PNG textures, opaque cubes, cutout foliage, explicit block states, items, item components, block/item tags |
| Player | Startup movement/body/spawn/eye rules and additional cosmetic palette colors |
| Generation | Deterministic contributors that write the current chunk after built-in terrain |
| Gameplay | Registered item/block/entity/empty actions, optional typed commands, block decisions, entity ticks, pickup decisions |
| Inventories | Read authorized slots; create or consume stacks explicitly; move exact stacks; collect eligible drops |
| Entities | Fixed-byte gameplay entities, private state, public projections, scheduling, bounded public queries |
| Creatures | Colored cuboid models, server movement targets, terrain queries, interactions, child spawning and despawning |
| Storage | Placeable persistent containers, small footprints, grouped inventory screens |
| Machines | Finite recipes, fuel, active states, placement variants, automation ports and scheduled processing |
| World systems | Durable chunk/entity/profile owner state, scheduling, conditional terrain edits, wakes, same-system messages and declared entity/drop effects |
| UI | Fixed JSON widget trees, packaged styles/fonts/images, local event callbacks and requests for registered server actions |
| Client replicas | Bounded public observations, UI/parameter updates, creature pose/tint and attached visual effects |
| Rendering | Versioned WGSL material hooks, scene-color effect graphs and typed parameters updated from Luau |

## Package format and module imports

The required manifest lines are `format`, `package`, `version`, and `entry`.
Declarations use whitespace-separated tokens, one declaration per line. Blank
lines are accepted; comments and unknown or duplicate declarations are errors.
Versions are exact three-component unsigned integer versions such as `1.2.0`;
ranges, prerelease suffixes and leading zeroes are unsupported.

- Format 1 uses `module main scripts/main.luau`; all modules are server-only.
- Format 2 uses `module server|client|shared name side/path.luau`. The path must
  start with the named side directory. The entry must be server or shared.
- `dependency arithmetic 1.2.0` requires that exact installed package version.
- `import("arithmetic:operations")` loads a declared module by identity. A module
  sees its own package and exact direct dependencies, subject to side rules.
  Servers cannot execute client modules; clients cannot import server modules.
- Imported modules can export non-nil tables, functions or scalars. Initialization
  is lazy and cached within one invocation. Cyclic initialization fails. Import
  nesting is limited to 32 modules.
- Each invocation uses a fresh VM and import cache. Module globals do not persist
  gameplay state between calls or retries. Imports share the invocation budget.

Identifiers are 1–64 lowercase ASCII letters, digits, underscores or hyphens.
Registered keys use `package:local_name` and normally belong to the declaring
package. The `bloxgloom` package namespace is reserved. File paths are bounded
relative paths without parent traversal; format-2 files cannot have dot-prefixed
components. Undeclared files are ignored. Discovery currently requires Unix and
rejects symlinks/special files, including symlinked path ancestors.

Format-2 asset declarations use `asset kind name path`:

| Kind | Required location and extension |
| --- | --- |
| `texture` | `assets/textures/*.png` |
| `ui-document`, `ui-style` | `assets/ui/*.json` |
| `ui-font` | `assets/fonts/*.ttf` |
| `ui-image` | `assets/ui/*.png` |
| `shader`, `material-shader` | `assets/shaders/*.wgsl` |
| `effect` | `assets/effects/*.json` |
| `material` | `assets/materials/*.json` |

Asset names are unique across kinds within a package. There are no wildcard,
arbitrary data-file or save-file declarations.

## Startup host and required capabilities

Startup modules return `function(host)`. Call services with a dot, such as
`host.register_item(...)`. Declare each required capability with a `requires`
line. Unsupported capabilities fail startup, even if a broader Rust API supports
them. The currently accepted manifest capabilities are the eight names below.

| Registration | Required `bloxgloom:` capability suffixes | Limit per package |
| --- | --- | --- |
| `register_texture`, `register_block`, `register_item`, `register_tag` | `content/v1` | 32 textures, 32 blocks, 32 items, 32 tags |
| `register_player_rules`, `register_player_appearance` | `content/v1` | One rules selection and one appearance declaration across the entire package set |
| `register_generator` | `generation/v1` | One |
| `register_action`, `register_handler`, `register_entity` | `actions/v1` | One action, 32 handlers, 32 entity definitions |
| `register_system` | `owner_systems/v1` | One |
| `register_storage` | `content/v1`, `storage/v1`, `inventory_screens/v1` | Eight |
| `register_creature` | `content/v1`, `mobile_entities/v1` | Eight |
| `register_machine` | `content/v1`, `machines/v1`, `inventory_screens/v1` | Eight |

Each registered block also creates a same-key placeable item and consumes one
of the 32 item declarations. Register referenced textures and blocks before
their consumers. Invalid declarations abort the candidate installation;
catching a rejected registration with `pcall` cannot publish partial content.

## Content

### Textures and blocks

`host.register_texture(key, asset_name, options?)` binds a declared local PNG.
Its optional `alpha_cutout` boolean enables cutout texture preprocessing.

`host.register_block(key, display_name, top_texture, options?)` creates a block
and its placeable item. Defaults are a solid, opaque cube, one legal state,
uniform faces, no emission, no flammability and no plant support. All face
textures must already be registered by this package.

| Block option | Supported value |
| --- | --- |
| `flammable`, `supports_plant`, `solid`, `replaceable` | Booleans |
| `side`, `bottom` | Package texture keys; omitted faces use the third argument |
| `emission` | Integer 0–15 for voxel lighting |
| `reflectance` | Three channels in 0–255 |
| `geometry` | `cube`, `crossed_plant`, `narrow_crossed_plant` |
| `material` | `opaque`, `cutout` |
| `properties` | Up to eight property names, each with 1–16 possible string values |
| `states` | 1–32 explicit legal property combinations; optional per-state emission and textures |

Crossed plants require `solid=false` and cutout material. Every face of a cutout
block must use a cutout texture. Partial collision boxes and arbitrary geometry
are unavailable.

```luau
host.register_block("garden:lamp", "Garden Lamp", "garden:brick", {
    properties = {lit = {"off", "on"}},
    states = {{lit = "off"}, {lit = "on", emission = 12}},
})
```

State keys are canonical, with sorted properties, for example
`garden:lamp[lit=on]`. Only listed combinations are legal; no Cartesian product
is generated. The block item places the lexicographically first legal state.
Each state may override all faces with
`textures={top="garden:top",side="garden:side",bottom="garden:bottom"}`.
The existing `axis=x|y|z` convention rotates cube cap textures.

### Items, stack components and drops

`host.register_item(key, display_name, texture_key, options?)` creates an
independent non-placeable item using a built-in or registered package texture.
Stacks remain capped at 128 items.

| Item option | Purpose |
| --- | --- |
| `sprite` | Defaults to true; false selects textured item-cube presentation |
| `drop_size` | `small`, `normal` (default), `large`; client visual size only |
| `drop_animation` | Bounded pop, hover/spin and pickup-flight settings |
| `drop_policy` | Server-owned gravity, collision, pickup/merge range and expiry |
| `components` | Versioned opaque per-stack byte schema |

The current parser accepts at most five option entries in one item declaration.
Omitted animation/policy fields retain stock defaults. Animation fields are
`pop_duration` (0.05–4 seconds), `pop_height` (0–2), `hover_amplitude` (0–0.5),
`hover_speed` (0–16), `spin_speed` (0–20), `pickup_duration` (0.05–4 seconds),
`pickup_arc` (0–2), and `pickup_turn` (0–20). Distances use blocks.

Policy fields are `gravity` (0–96 blocks/s²), `terminal_speed` (0–60 blocks/s),
`radius` (0.01–0.49 blocks), `pickup_range` and `merge_range` (0–8 blocks), and
integer `lifetime_ms` (1,000–86,400,000). Zero merge range disables merging.
The server controls drop ownership, age, motion, merges, eligibility and expiry;
client animation never decides when an item enters inventory.

```luau
host.register_item("garden:token", "Token", "garden:brick", {
    components = {
        version = 1, fingerprint = "000000070000002a",
        max_bytes = 64, required = true,
    },
})
```

Component versions are 1–65535; fingerprints are nonzero 16-digit hexadecimal
schema identities; maximum payload length is 1–1024 bytes. Requiredness defaults
to false. The host validates the envelope, while scripts interpret the binary
payload. Exact bytes participate in stack equality and survive supported
inventory/drop transfers and persistence. Default script items forbid components.
Placement stores block state rather than component bytes, and ordinary refunds
create plain items; do not assume components survive placement/harvest.

### Tags

`host.register_tag(key, "block"|"item", members)` declares up to 32 unique
members. A member is a definition key or `"#namespace:tag"`. Forward references
resolve during installation. Missing references, wrong kinds and cycles fail.
Machine item filters can use item tags. Block tags provide validated composition
metadata; no general Luau tag-query service is bound.

## Player rules and appearance

`host.register_player_rules(key, revision, fields)` selects one immutable
world/session contract. All nine fields are required:

| Field | Range |
| --- | --- |
| `half_width` | 0.125–0.5 |
| `foot_inset` | 0–0.25 |
| `middle_height` | Above foot inset, gap at most 1 |
| `head_height` | Above middle height, gap at most 1, at most 2 |
| `intent_blocks_per_second` | At least 0.1, at most movement budget |
| `budget_blocks_per_second` | At most 16 |
| `headroom` | Integer 2–128 |
| `max_rise` | Integer 1–1024 |
| `eye_height` | Between foot inset and head height |

Revision is an integer from 1 through 4294967295. The server uses these rules
for movement and spawn admission; clients install the matching prediction,
camera, reach and placement contract. Without a declaration, built-in rules
apply. This is not a per-player runtime movement override.

`host.register_player_appearance(key, revision, "bloxgloom:humanoid/v1", palettes)`
appends up to 24 finite linear RGB colors in 0–1 to each optional `skins`,
`shirts`, and `pants` list. Existing indices remain stable. The server validates
and saves a profile's selected indices; this declaration supplies colors,
not custom player geometry or physics. See [player rules](docs/modding/PLAYER-RULES.md).

## World generation

`host.register_generator(key, revision, module)` selects an own-package module
returning `function(c)`. Revision is 1–4294967295. Contributors run in lexical
key order after built-in terrain; later writes to a cell replace earlier writes.

| Generation input/service | Meaning |
| --- | --- |
| `c.chunk_x`, `c.chunk_y`, `c.chunk_z` | Current chunk coordinates |
| `c.world_position(lx, ly, lz)` | Returns absolute x, y, z for a local cell |
| `c.builtin_terrain_height(x, z)` | Built-in column ground height |
| `c.builtin_base_block(x, y, z)` | Built-in base state, including caves; excludes decorations, contributors and saved edits |
| `c.random_at(x, y, z, salt?)` | Deterministic sample in `[0,1)` using the host's exact world seed; salt is an integer 0–4294967295, default zero |
| `c.set_block(lx, ly, lz, state_key)` | Writes a legal state inside the current chunk |

Local coordinates are integers 0–15. Each invocation can issue at most 4096
writes, including repeated writes to one cell. Cross-chunk features must
recompute the same absolute anchor decision in each intersecting chunk and
write only that chunk's portion. Use stable ordering and salted absolute
coordinates, rather than table iteration order or per-VM object identity.

Bump the generator revision whenever its output algorithm, dependencies or
configuration change. `world.meta` rejects incompatible restarts. Generation
failure does not silently supply fallback air. See [generation](docs/modding/GENERATION.md).

## Actions, commands and gameplay decisions

Register an action with:

```luau
host.register_action("garden:store", 1, "Store", "item",
    "bloxgloom:stick", "garden:store")
```

The positional arguments are key, revision (1–65535), display label, target
kind, target key, own-package module, and an optional command descriptor.
Target kinds are `item`, `block`, `entity`, or `empty` (with a nil target key).
The module returns `function(c, event)` and stages effects through the server's
gameplay transaction. The host validates session, target, reach, line of sight,
observed revisions, permissions and commit dependencies.

An empty-target action can also expose a command under its action key:

```luau
host.register_action("garden:grant", 1, "Grant", "empty", nil,
    "garden:grant", {
        permission = "Admin",
        arguments = {{kind="item_key", max_bytes=64}, {kind="count", default=1}},
    })
```

Permission is `Player` or `Admin`. The ordered argument schema supports
`item_key`, `entity_key` (with `max_bytes` 3–128), and `count` (1–128, optional
default). There are at most eight fields and 130 encoded bytes. Defaults can
make trailing text arguments optional; the binary callback input includes their
explicit values. Keys use a one-byte length followed by namespaced ASCII bytes;
counts use one byte. Scripts must interpret and validate argument semantics.
No command aliases or arbitrary text argument schema are bound.

Register an exact gameplay decision owner with
`host.register_handler(key, revision, event_kind, target_key, module)`:

| Event | Callback fields beyond `kind` |
| --- | --- |
| `ActionRequested` (from `register_action`) | `action`, `position`, selected zero-based `slot`, binary `arguments`, optional `cell` and `entity` |
| `BlockRemoved` | `cell`, `previous` block descriptor, `cause`, deterministic `random` sample |
| `BlockPlaced` | `cell`, `previous`, `placed` |
| `NeighborChanged` | `cell`, `changed` cell, `previous`, `current` |
| `EntityTick` | `entity` ID, `position`, exact `tick` |
| `PickupRequested` | `position`, up to 32 host-eligible `drops` records with `id` and `count` |

Removal causes are `Break`, `Replacement`, `SupportLoss`, `WorldEdit`, `Burn`,
and `AnchoredBreak`. Block event targets are block types; an `EntityTick` target
must be an own-package gameplay entity. Pickup decisions target `bloxgloom:drop`.
Decision ownership is singular: duplicate event/target bindings fail startup.
There is no script-defined fallback decision owner or unrestricted event bus.

### Gameplay context services

`c.tick` is exact simulation time. `c.player_position` is a readonly feet-position
triple when the invocation has an acting player. Event tables, descriptors and
host coordinate sequences are readonly. A block descriptor contains `state`,
`block_type`, optional `primary_item`, `plant`, and `supports_plant`.

| Service | Supported operation |
| --- | --- |
| `c.random(x,y,z,sequence?)` | Deterministic `[0,1)` sample; optional integer sequence 0–4294967295 |
| `c.block(x,y,z)` | Reads an authoritative block descriptor |
| `c.set_block(x,y,z,state)` | Stages a legal world-state edit |
| `c.entity(id)` | Reads public entity view or nil: `id`, `entity_type`, `position`, optional `anchor`, public `data` |
| `c.nearby_entities(x,y,z,radius)` | Reads up to 128 public entities, with host visibility/dependency checks; excess results reject the query |
| `c.anchored_entity_at(x,y,z)` | Returns the exact anchored entity ID or nil |
| `c.entity_state(id)` | Reads private bytes only when host ownership permits |
| `c.spawn_entity(key,x,y,z,state)` | Stages a package-owned registered gameplay entity with validated state bytes |
| `c.update_entity(id,state)` | Stages owned private-state replacement; returns boolean |
| `c.remove_entity(id)` | Stages owned removal; returns boolean |
| `c.schedule_entity(id,delay?)` | Schedules an entity callback in 1–100000 ticks; nil suspends; returns boolean |
| `c.spawn_drop(x,y,z,item,count,delay_ms)` | Explicitly creates a plain-item world drop |
| `c.spawn_stack(x,y,z,stack,delay_ms)` | Explicitly creates a drop with exact component bytes |
| `c.inventory(owner)` | Reads authorized slot records |
| `c.give(owner,stack)` | Explicitly creates/inserts a stack; returns boolean |
| `c.take(owner,slot,count)` | Consumes an exact count; returns the removed stack or nil |
| `c.transfer(from_slot,to_slot,count)` | Moves an exact count within the acting player's inventory |
| `c.transfer_inventory(from,source,to,destination,count)` | Moves an exact count between authorized owners |
| `c.move_slots(owner,source,destination,count)` | Moves an exact count within an authorized owner |
| `c.collect_drop(id,max_count)` | Credits an eligible drop to the acting player's finite inventory |
| `c.admin_give(item,count)` | Host-authenticated admin-only item grant; returns boolean |
| `c.admin_spawn(key)` | Host-authenticated admin-only creature spawn |

Gameplay inventory slots are zero-based; returned sequences are one-based.
An owner is `"player"` or an exact entity ID handle, never an arbitrary profile.
A slot contains optional `stack` and boolean `insert`/`extract` permissions.
A stack is `{item="package:item",count=1,components={version=1,bytes="binary"}}`;
omit components for a plain stack. Counts are 1–128; component payloads are
1–1024 bytes subject to the item's schema. Drop delays are integer milliseconds
0–4294967295.

Transfers preserve exact components and never create items. False/nil from an
ordinary unsuccessful inventory operation means no change. Creation through
`give`, drop spawning or declared recipes is explicit server-script authority;
the client cannot invoke it directly or nominate another actor. Admin services
recheck the authenticated actor even when called by a player-permission action.

World/entity operations remain bounded by the host's captured reads, interaction
scope, ownership and schema rules. The production gameplay context has a shared
4096-operation budget. A rejected host operation poisons the plan even if caught
with `pcall`. Successful staged effects still require host transaction admission;
errors, stale reads or failed commits do not partially apply the script's effects.
Retries get a fresh VM. See [bindings](src/server/script/gameplay/bindings.rs)
and [inventory services](src/server/script/gameplay/inventory.rs).

## Persistent gameplay entities and exact handles

`host.register_entity(key, schema_version, state_bytes, public_bytes, delay?)`
declares a package-owned gameplay entity. Schema versions are 1–65535; private
state must have exactly the declared 1–65535-byte length. The public projection
is its first `public_bytes` bytes, from zero through `min(state_bytes,4096)`.
An omitted initial delay suspends ticks; otherwise it is 1–100000 ticks.
Scripts encode their own bytes without host padding or field conversion.
Use an `EntityTick` handler plus context services to implement its behavior.
This declaration does not supply a creature model or arbitrary motion API.

IDs and counters are immutable host-created handles:

| Handle | Use |
| --- | --- |
| `BloxEntityId` | Entity identity; pass unchanged to entity/inventory services |
| `BloxProfileId` | Entity-independent profile owner identity in persistent systems |
| `BloxRevision` | Captured state/motion version; supports equality and `:is_initial()` |
| `BloxTick` | Simulation time; supports equality, `:before(other)` and `:elapsed_since(earlier)` |

IDs support equality, table keys and exact diagnostic `tostring` labels. There
is no runtime ID constructor. Numeric IDs, printed labels, lookalike tables and
revision tokens cannot substitute for a handle. Elapsed tick intervals reject
reversed time or differences greater than `2^53`. Handles grant identity, not
access permissions. Legacy validated numeric-word forms remain compatibility
paths; new authoring should use handles. Binary state and component strings
may contain zero bytes and are distinct from presentation text.

## Durable owner systems

`host.register_system(declaration)` defines one persistent system per package.
Its module returns `function(c)` returning `(binary_state, delay_ticks)`.
The host persists state and deadlines with world effects and wakes through the
owner write-ahead log. No VM state is retained.

```luau
host.register_system {
    key="garden:growth", schema=1, revision=1, module="garden:growth",
    max_state_bytes=64, max_jobs_per_tick=2,
    read_world=true, read_radius_chunks=1,
    seeds={{x=0,y=0,z=0,data=""}},
}
```

Required fields are `key`, `schema` (1–4294967295), `revision` (1–65535),
own-package `module`, `max_state_bytes` (1–4096), `max_jobs_per_tick` (1–8),
and `seeds` (at most 32). Returned delay is 1–4294967295 ticks.

| Optional declaration | Effect |
| --- | --- |
| `partition` | `chunk` (default), `entity`, or `profile` |
| `read_world` | Enables captured terrain reads and conditional edits |
| `read_radius_chunks` | 0: owner chunk; 1: its 3×3×3 neighborhood; requires world reads |
| `after` | Up to 16 existing system keys defining phase ordering; cycles fail startup |
| `edit_cause` | `world_edit` (default) or `burn`; burn requires world reads and only removes cells to air |
| `accepts_intents` | Enables bounded same-system durable messages; requires world reads |
| `intent_bootstrap` | Constant validated initial bytes for absent message destinations |
| `creates_drops` | Enables direct plain drop creation within captured world cells |
| `creates_entities` | Enables direct own-package gameplay entity creation |
| `reads_entities` | Captures up to 128 package-owned mobile records in the neighborhood |
| `mutates_entities` | Enables captured entity state updates/removals; requires entity reads |

Drop/entity creation and entity reads require `read_world=true`. Entity/profile
partitions cannot declare world reads or world effects. Chunk seeds are
`{x,y,z,data}` with chunk coordinates; entity/profile seeds are `{id,data}` with
exactly 16/32 hexadecimal ID digits respectively.

Contexts expose readonly `data`, `tick`, `revision`, `owner_kind`, and `owner`.
A chunk owner is a coordinate triple; entity/profile owners are exact handles.

| Service | Operation/budget per plan |
| --- | --- |
| `c.block(x,y,z)`, `c.block_info(x,y,z)` | Captured state key or descriptor; shared 64-read budget |
| `c.edit(x,y,z,before,after)` | Up to 16 conditional edits against captured preimages |
| `c.wake(system,x,y,z)` | Chunk-owner wake; up to 32 wakes |
| `c.wake_entity(system,id)`, `c.wake_profile(system,id)` | Partition-checked wakes using exact identities |
| `c.send(x,y,z,payload)` | Up to eight messages, 512 bytes each, to chunk owners in the same system |
| `c.spawn_drop(x,y,z,item,count,delay_ms)` | Up to 16 declared direct drops at integer cell centers |
| `c.spawn_entity(key,x,y,z,state)` | Up to 16 declared own-package gameplay entities at integer cell centers |
| `c.update_entity(id,revision,state)`, `c.remove_entity(id,revision)` | Up to 16 conditional mutations of captured records, with no duplicate IDs |

With entity reads, `c.entities` contains readonly `id`, `revision`, `key`,
`position`, and authorized private `state` records. With intents, `c.inbox`
contains up to eight readonly deliveries: binary `payload`, exact `produced_tick`,
and `id={source={x,y,z},revision,ordinal}`. Preserve every identity field when
comparing deliveries. Successful plans acknowledge the inbox atomically with
state, outgoing messages and other effects; delivery occurs on later ticks.

Reads see captured preimages, including after proposing an edit. Missing chunks
defer the job while the host loads authoritative terrain. Out-of-capture reads,
invalid sends and stale revisions reject the plan; catching errors does not
make it valid. Duplicate edits across jobs reject the owner wave. Waking an
absent owner sets a bounded pending flag; message bootstrap is the explicit
absent-owner creation mechanism. See [the neighborhood fixture](fixtures/neighborhood/README.md)
and [system bindings](src/server/script/system.rs).

## Mobile creatures

`host.register_creature(declaration)` combines a bounded model, server-owned
movement and a Luau behavior module. Required fields are `key`, own-package
`module`, `schema`, `revision` (both 1–65535), `max_state_bytes` (1–256 private
bytes), `interval` (1–1000), `body`, and `model`. Optional `initial_state` defaults
to an empty binary string.

- `body` requires `half_width` (0.05–1), `height` (0.1–3), and `speed` (0–4).
- `model` contains 1–16 colored cuboids with three-component `min`, `max`
  coordinates in −4–4 and linear RGB `color` in 0–1. Optional part `motion` is
  `body` (default), `left_foot`, or `right_foot`.
- Optional `animation` fields are `stride_rate` (0–40), `stride_amplitude`
  (0–3), `idle_rate` (0–20), `idle_bob` (0–0.1), `walk_bob` (0–0.2),
  `fall_stretch` and `landing_squash` (0–0.5). Omitted fields keep stock defaults.
- `read_radius` is 0 or 1 chunk. `reads_neighbours=true` requests a bounded
  public neighboring-entity view. `wakes_on_terrain_change` defaults to true.
- Optional `interaction` supplies a fixed 1–128-byte request for the creature's
  registered interaction.

The behavior module returns `function(c)`. On ticks, `c.event` is `"tick"` and
inputs include private `data`, exact `id`, `tick`, and feet `position`.
`c.neighbours` contains public `{id,key,position,public}` records when requested;
it never exposes neighbors' private state.

| Tick query | Purpose |
| --- | --- |
| `c.route(x,z)` | Bounded next route point, or nil |
| `c.solid(x,y,z)` | Checks an integer terrain cell |
| `c.clear(x,y,z)` | Checks body clearance at a feet position |
| `c.grounded(x,y,z)` | Checks grounding at a feet position |
| `c.walk_edge(x1,y1,z1,x2,y2,z2)` | Checks a traversable step between feet positions |

Ticks have a shared limit of 16 world queries, including at most eight routes.
Invalid or unavailable queries reject the tick even when caught. Return
`(binary_state, delay_ticks, target_x, target_z, lifecycle?)`, with delay 1–100000.
Return nil for both target coordinates to omit a horizontal movement target.
The host advances movement with terrain collision and gravity; scripts do not
teleport the body or own its replicated motion.

An optional fifth result is `{despawn=true,spawns={{x,y,z},...}}`. It may create
at most four children within eight blocks of the parent's feet. A child can use
`{key="garden:other_creature",position={x,y,z}}` to choose another creature
declared by the same package; otherwise it uses the parent's declaration.
Children start with their declaration's initial private state. The host checks
terrain and commits parent/child lifecycle effects together.

On interaction, `c.event` is `"interact"`; inputs are `data` and binary `request`.
Return the replacement private binary state. The server validates interaction
admission. The stock script creature projection exposes only yaw and grounded
pose, not arbitrary authored private bytes. Public pose plus client replica
callbacks can drive presentation. See [the creature/machine showcase](fixtures/phase4-showcase/README.md)
and [creature declarations](src/server/script/startup/creature.rs).

## Storage and inventory screens

`host.register_storage(entity_key, registered_block, title, slots, columns, options?)`
binds a previously registered package block to a persistent container. Slots
are 1–54 and columns are 1–9. The same-key block item places it. The server owns
finite contents, transfers, placement/removal and refunds.

Optional `options` accepts `hint`, `footprint`, and `groups`. Footprints contain
1–8 unique integer offset triples in −2–2, including `{0,0,0}`; all cells use
the block's default placement state. The default is a single cell. Groups are
a dense list of `{label,count}` covering every slot in order. Storage groups
allow insertion and extraction. Storage declarations do not expose arbitrary
inventory initialization, automation ports or a custom Rust screen renderer.
These host inventory screens are separate from authored JSON UI documents.

## Machines and automation

`host.register_machine(declaration)` defines a host-owned finite process and
inventory screen. Required fields are `entity`, previously registered `block`,
own-package behavior `module`, `schema`, `revision` (1–65535), `interval`
(1–60000), `title`, and either `recipe` or `recipes`. Optional `hint` supplies
screen guidance. Machine plans retain up to 1024 private bytes.

Recipes are a dense list of 1–8 records, or a single record:

```luau
recipe = {
    key="garden:crush", input="bloxgloom:stone", input_count=1,
    output="bloxgloom:gravel", output_count=2, pulses=12,
}
```

Counts are 1–128 and pulses are 1–60000. Optional
`fuel={item="bloxgloom:stick",pulses=30}` enables a fuel slot, with fuel pulses
1–240. Unfueled machines have input/output slots; fueled machines have
fuel/input/output slots. Output insertion is blocked by the host screen.

Recipe `input_components` and fuel `components` accept `"empty"` (default),
`"any"`, or `{version=1,bytes="exact binary"}`. Recipe `output_components`
accepts `"empty"`, `"preserve_input"`, or that exact-value form. Exact bytes
are 1–1024 with a positive 16-bit version. Registered schemas must permit the
values. The host validates output before consuming input/fuel; preserving
components requires an explicit `preserve_input` policy.

`footprint` follows the same 1–8 cells, −2–2 offset and anchor rules as storage.
A fueled machine may set `active_state` to another legal state of its block.
The host changes footprint cells while fuel remains, then restores placement
state. Optional `variants` has 2–8 `{state,active_state?}` records, selecting
placement/active state pairs for the same block. States must be distinct; the
first is the block's default placement state. All variants share footprint,
recipes, ports and screen. Top-level `active_state` cannot accompany `variants`.

Optional `ports` contains up to eight named automation ports. Each declares
one or more cardinal `faces` and one-based `insert`/`extract` slot lists:

```luau
ports = {{name="input",faces={{0,1,0}},insert={1}}}
```

The callback receives readonly `data`, exact `tick` and `due`, `fuel`, `progress`,
and one-based `slots` (empty entries are nil). Slot records contain numeric
catalog `item`, `count`, and `has_components`. Return
`(binary_state, delay, work)`, with delay 1–60000 scheduled relative to `due`.
Work is true (one process), false (no work), or a dense list of at most eight:

- `{kind="process"}` asks the host to run a declared recipe.
- `{kind="transfer",offset={0,1,0},port="input",push=false,count=1}` asks for
  an exact stack transfer through the declared port. `offset` must be cardinal.
  Optional `peer_port` selects a neighboring port; optional one-based
  `source_slot`/`destination_slot` constrain selection. `item` or `same_as_slot`
  can select stacks, and cannot both be supplied. Omitted count defaults to one.

The host resolves neighbors, permissions, filters and exact component equality.
Machines can process declared recipes and move finite items; work proposals
cannot invent new transformations or bypass slot ownership. See
[machine declarations](src/server/script/startup/machine.rs),
[transfer work](src/server/script/machine/work.rs), and
[the showcase](fixtures/phase4-showcase/README.md).

## Client startup and authored UI

Format-2 packages can declare a client/shared module named `client_startup`.
It runs once per connection before content readiness and returns `function(host)`.

| Startup service | Purpose |
| --- | --- |
| `host.set_text("package:document/node",text)` | Initializes owned UI text |
| `host.set_state("package:document",state)` | Initializes explicit document-local state |
| `host.set_parameter(resource,name,value)` | Initializes a declared owned material/effect parameter |
| `host.set_replica_handler("package:module")` | Registers one own-package client/shared replica handler |

These services have presentation authority only. Startup state is reset on
join/reconnect/server switch and is not saved gameplay state.

UI documents declare fixed JSON widget trees with these kinds:

| Widget | Behavior |
| --- | --- |
| `panel` | Nested grouping, vertical layout or wrapped rows |
| `label` | Display text, updatable by callbacks |
| `image` | Packaged static PNG |
| `button` | Declared event on click or keyboard activation |
| `input` | Single-line text editing and change events |

Styles support width/height, padding, gaps, row layout, colors and packaged fonts.
The renderer provides scrolling, wrapping, focus, selection, clipboard and IME.
Luau cannot directly call egui or create/remove/rearrange widgets. There are no
authored sliders, checkboxes, selects, tables, multiline inputs, HTML/CSS or UI
animations.

A document opts into events with
`"presentation":{"capability":"local-ui","module":"garden:view"}`.
The module returns `function(input)` with `sequence`, namespaced `event`, widget
`value`, explicit local `state`, and current `texts`. Return a dense list of up
to 16 commands:

```luau
return {
    {op="text",node="garden:controls/title",value="Ready"},
    {op="visible",node="garden:controls/icon",value=true},
    {op="state",value="ready"},
    {op="parameter",resource="garden:surface",name="strength",value=0.5},
    {op="action",key="garden:store",arguments=""},
}
```

Text/state command values are bounded to 128 UTF-8 bytes without control characters. Nodes,
documents, resource parameters, events and actions are checked for ownership.
A UI result can request at most one package-owned action with up to 130 raw
binary argument bytes. The client supplies its current selected item or aimed
block/entity and observed version. UI cannot provide target coordinates or an
arbitrary entity ID. The server authorizes and commits the result, with applied
or denied receipts. UI feedback also distinguishes unsent/pending requests.
Each event uses a fresh bounded worker VM and explicit local state; invalid
results do not partially mutate presentation.

See [the UI fixture](fixtures/packages/uidemo/),
[block-target UI actions](fixtures/ui-target-actions/README.md),
[entity-target UI actions](fixtures/ui-entity-actions/README.md), and
[UI foundation](docs/modding/UI-FOUNDATION.md).

## Public replica presentation

A client replica handler receives the local presentation fields plus advisory
observations: `replica:inventory`, `replica:block`, `replica:world`,
`replica:action`, `replica:entities`, and `replica:anchors`. The first four carry
bounded ASCII summaries in `input.value`; they are not a general client world
query API. Observations coalesce by kind behind a bounded queue.

Mobile and anchor events each maintain a separate sorted window of at most
16 package-owned public views. `input.entities` records contain exact `id`,
`key`, `revision`, `motion_revision`, binary `public` (at most 4 KiB per view),
and `position`. Anchors use cell-center positions and initial motion revisions.
`entered`/`left` contain IDs entering/leaving the window; leaving is not proof
of authoritative despawning, and the window is not the entire world.

Replica replies can update owned UI text/visibility/state and shader parameters,
but cannot request gameplay actions. Up to 16 commands per result also support:

| Command | Fields and bounds |
| --- | --- |
| `visual` | Offered mobile `entity`; `yaw` −π–π, `bob` and `squash` −0.25–0.25 |
| `tint` | Offered mobile `entity`; `r`,`g`,`b` each 0–1 |
| `ember` | Offered mobile/anchor `entity`; local `x`,`y`,`z` offsets −1–1 blocks |
| `spark` | Same entity/offsets; RGB 0–1; optional `size` 0.05–0.5 blocks (default 0.16), integer `lifetime_ms` 100–2000 (default 850) |

Embers last 850 ms. Both effects follow rendered entity positions and share a
32-effect session cap. Only IDs offered by that callback are valid. Anchors
support effects but reject pose/tint. The next completed entity callback replaces
mobile pose/tint; anchor effects are removed when their anchor leaves the offered
window. Invalid batches fail the presentation session without partial results.
Switching sessions clears overrides and effects. These callbacks run off the
window/network thread and cannot change authoritative state or ownership.

## Authored materials, effects and Luau parameters

Packages declare JSON material/effect descriptors and WGSL assets. The renderer
owns geometry, projection, GPU resources and submission. These hooks do not
grant gameplay authority. Version-1 Jade/Sepia contracts remain supported;
version 2 adds surface/vertex hooks, typed parameters and effect graphs.

### Materials

Declare `material` and `material-shader` assets. A version-2 descriptor names
its shader, up to eight catalog texture `targets`, up to four sampled `textures`,
optional `vertex_offset` (0–0.25 blocks), and optional parameters. Targets and
inputs are own-package or built-in texture keys. Each target has one material
owner; conflicts fail preparation. There are at most 16 materials per bundle,
8 KiB shader source per material and 4 KiB version-2 descriptors.

Supply `material_fragment(BgSurface) -> BgSurface`, and optionally
`material_vertex(BgVertex) -> BgVertex`. The host supplies vertex position,
normal and UV; surface values also include albedo, light and emission. Hooks
can modify the supplied surface and displace vertices within the declared
per-axis allowance. Collision/selection retain host geometry. Fog, depth and
the cutout alpha test follow the hook; opaque surfaces remain opaque.

WGSL helpers are `material_texture(uv,index)`, `material_parameter(index)`,
`material_time()` (session preparation seconds), and `material_sun()`.
The surface covers opaque/cutout terrain, crossed foliage, cube drops and
sprite/cross drops. Actor cuboids and inventory/UI icons use separate paths.
Version 1 retains `custom_albedo(vec3f,vec2f,vec3f) -> vec3f` and a single
descriptor `texture`.

### Scene-color effects

Declare `effect` and `shader` assets. A version-2 pass has one or two inputs,
one unique output, optional `after` constraints and `order`, resolution `scale`
(divisor 1, 2 or 4), `final`, and optional parameters. The reserved input
`bloxgloom:scene_color` is the unfiltered HDR world scene. Other inputs name
own-package or exact-direct-dependency outputs; ordering references name
descriptor asset keys under the same visibility rule.

Inputs/`after` edges determine ordering; ready passes sort by order then key.
Missing references, cycles, duplicate/reserved outputs, disconnected passes
or a graph without exactly one final output fail. The final output feeds built-in
bloom/display mapping before UI drawing. There are at most eight passes,
16 KiB shader source per effect, 4 KiB descriptors, and a combined 64 MiB
intermediate-attachment budget. Attachments use linear HDR `rgba16float` and
are capped at 2048 or the device limit; the renderer can reduce resolution.

Supply `effect_fragment(uv:vec2f) -> vec4f` returning linear HDR color.
Helpers are `effect_input(uv,index)`, `effect_parameter(index)`,
`effect_time()`, and `effect_size()`. A one-input pass aliases input 1 to input 0.
Version 1 retains its fixed scene fragment, `stage="scene_color"` and `order=0`;
its final output cannot coexist with another final effect.

### Typed parameter updates and shader restrictions

Each resource declares up to eight parameters: `float`, `uint`, `bool`, `vec2`,
`vec3`, `vec4`, or four-component `color`. Values must be finite and match
declared defaults/types. Optional min/max bounds apply per component; defaults
are 0–1 for colors and −1000000–1000000 otherwise. Unsigned integers must also
be exact integers 0–16777215. Luau supplies vectors as dense tables.

Initialize with `host.set_parameter(resource,name,value)` or return a
`parameter` command from UI/replica callbacks. The resource key is the descriptor
asset identity, not its shader/output name. Only the owning package can update
it. Parameter changes are presentation state and reset with a new session.

Version-2 authored hooks permit bounded scalar/vector/matrix operations, small
structs, helpers and conditionals. Loops, recursion, authored arrays, resource
declarations, overrides, bind groups and entry points are rejected. Limits are
512 expressions and 32 locals per function, 12 functions including four helpers,
and 4096 expanded helper-call work. These bound shader work, not a promised
GPU frame time. CPU validation precedes content readiness; pipelines compile
asynchronously while joining. A failed package pipeline rejects the candidate
session. See [the complete shader contracts](docs/modding/AUTHORED-VISUALS.md),
[Prism](fixtures/visual-packages/prism/README.md),
[Jade](fixtures/material-packages/jade/README.md), and
[Sepia](fixtures/effect-packages/sepia/README.md).

## Runtime, delivery and save compatibility

Scripts have Luau base operations and table/string libraries in a sandbox with
readonly globals. The host removes `require`, `print`, `gcinfo`, `getfenv` and
`setfenv`; it does not supply OS, debug or math libraries, a clock, OS randomness,
file loaders, sockets or native module loading. Use host random helpers and
explicit state. Imports retain the defining module's authority; dependencies
sharing a VM are not mutually untrusted security compartments.

| Resource | Current limit |
| --- | --- |
| Packages | 64 |
| Dependencies/modules/assets per package | 32 / 64 / 64 |
| Modules/assets across snapshot | 256 / 256 |
| Manifest/source/asset file bytes | 16 KiB / 64 KiB / 256 KiB |
| Aggregate discovered file bytes | 4 MiB |
| Default invocation memory | 8 MiB |
| Default invocation wall time | 50 ms, checked at VM interrupt safe points |
| Default interrupt budget | 10000 periodic checks, not an exact bytecode instruction count |
| Client pre-readiness syntax validation | 16 MiB VM budget, two-second preparation budget |

Individual schemas can impose smaller limits. Whole-snapshot limits apply
across dependencies too. Discovery freezes bytes, and callbacks run in fresh
bounded VMs on startup/simulation/generation/presentation workers, rather than
exposing live engine objects to scripts. Budget violations cannot be suppressed
by catching the VM error. Errors retain package/version/module attribution.

Clients receive a verified canonical bundle containing client/shared sources,
declared assets and inert startup metadata; server modules are excluded.
Downloaded source is compiled before content readiness, including dormant
modules. SHA-256 verifies exact bundle bytes but does not authenticate the
publisher. The negotiated client host contract is version 2 (wire version 11).
Clients also have to match catalog identities; matching bundle bytes alone is
insufficient. A verified in-memory cache supports reconnect reuse. There is no
persistent disk bundle cache or script networking/filesystem service.

World state, edits, movement, finite inventories, drops and durable scheduling
remain server-owned. `content.map` preserves save/wire identities; generators
also participate in `world.meta`. Several script schemas/handler identities
fingerprint the entire frozen installation, so editing even dependency sources
can make a save incompatible. Use a fresh save for incompatible changes; no
world/entity migration or live reload is bound. Do not treat client procedural
fallback terrain or local presentation state as authoritative.

## Features requiring engine work or native extensions

The broader [Rust host references](docs/modding/README.md) are not automatically
Luau bindings. For example, the Luau manifest does not accept the native item
icon or general anchored-behavior capabilities. Storage/machines offer specific
anchored lifecycles, not arbitrary anchored behavior registration.

There is no bound API for arbitrary block meshes, partial collision shapes,
translucent/liquid physics, imported models, custom player models, per-stack
render callbacks, sound, arbitrary renderer/GPU access, direct network messages,
filesystem/HTTP access, unrestricted world/player administration, runtime catalog
mutation, script state migration, or hot reload. Client replicas offer bounded
public windows and summaries, not general access to server private state.
Package delivery is directly from the server; marketplace/CDN infrastructure
is outside this surface.

## Authoring tools and runnable examples

[Editor types](types/bloxgloom.d.luau) provide callback aliases and nominal handle
types; [IDE setup](docs/modding/IDE.md) explains their use. They are editor-only,
not runtime globals. Runtime validation remains authoritative: some bindings,
such as machine/creature registration and storage options, are more extensive
than the current type definitions.

Use these previews to inspect actual rendered package content:

```sh
cargo run -- block-preview 'demo:press[lit=on]' press.png fixtures/phase4-showcase/packages
cargo run -- creature-preview demo:sproutling sproutling.png fixtures/phase4-showcase/packages
cargo run -- inventory-preview demo:press_machine ./press-screen fixtures/phase4-showcase/packages
cargo run -- ui-preview ./ui-images fixtures/packages
```

| Example | Demonstrates |
| --- | --- |
| [Combined garden](fixtures/combined-mod/README.md) | Content, authorized action, scheduled growth, downloaded startup/UI and material |
| [UI demo](fixtures/packages/uidemo/) | Manifest, five widget kinds, text input and finite inventory action |
| [Block action](fixtures/ui-target-actions/README.md) | Current ray-hit targeting and server receipt |
| [Entity action](fixtures/ui-entity-actions/README.md) | Entity targeting and binary arguments |
| [Neighborhood](fixtures/neighborhood/README.md) | Cross-chunk owner edits and durable messages |
| [Phase 4 showcase](fixtures/phase4-showcase/README.md) | Creature, interaction, multi-cell fueled machine, state textures, replicas and sparks |
| [Prism](fixtures/visual-packages/prism/README.md) | Version-2 material/effect graph, parameters and creature visuals |

The implemented binding inventory lives in [server scripting](src/server/script.rs),
[startup registration](src/server/script/startup.rs),
[manifest parsing](src/server/script/package/manifest.rs), and
[client presentation](src/client/presentation.rs). Consult these when extending
the engine, rather than treating historical proposals as available APIs.
