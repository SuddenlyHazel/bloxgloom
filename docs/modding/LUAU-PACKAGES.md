# Local Luau packages

The complete implemented binding inventory is [SCRIPTING.md](../../SCRIPTING.md).
This page explains package structure and the delivery/presentation workflow.
The [combined garden](../../fixtures/combined-mod/README.md) is the starting
example; [Phase 8 acceptance](PHASE-8-ACCEPTANCE.md) records the integrated checks
and the explicitly deferred scope.

# Try the UI example

`fixtures/packages/uidemo/` is a runnable package showing the manifest, server
entry, JSON document/styles, TTF font, PNG image and a client Luau event handler.
From the repository root, start these in separate terminals using a new save
directory (do not point the server at a world created with another package set):

```sh
cargo run --release -- server-packages fixtures/packages 127.0.0.1:4000 ./world-uidemo
cargo run --release -- client 127.0.0.1:4000
```

Press **F6** after joining. The name field updates local presentation. Select
a stick in your hotbar and click **Store one stick**, or focus it and press
Enter. The client sends `uidemo:store` through the normal registered-action
request/receipt path; the server transfers exactly one stick to slot 2 or
denies the request. There is no planting feature in this example. See
`client/view.luau` for the handler, `server/store.luau` for its authoritative
policy, and `assets/ui/welcome.json` for widget/event declarations.
`cargo run -- ui-preview <output-dir>` writes UI previews without joining.
Pass a package root as a second argument to preview that package's authored UI
and downloaded client startup state instead of the built-in `uidemo` fixture.
For one-process development with your normal local admin profile, run
`cargo run --release -- local-packages <package-root> <new-save-dir>`.
This starts the same verified package server and client on a real loopback
listener, then stops the server when the client closes. Keep each package set
in its own save directory so `content.map` remains its stable save identity.

# Authored UI widgets and current limits

Package UI documents declare a fixed widget tree in JSON. The supported kinds
are:

| Kind | Current behavior |
| --- | --- |
| `panel` | Groups child widgets vertically or in a wrapped row; panels can nest. |
| `label` | Shows text that a presentation callback may update. |
| `image` | Shows a packaged, static PNG. |
| `button` | Sends its declared event on click or keyboard activation. |
| `input` | Edits one line of text and sends change events; egui handles focus, selection, clipboard and IME input. |

Styles provide width, height, padding, gaps, row layout, colors and packaged
fonts. egui supplies document scrolling and text wrapping. Luau callbacks may
change text and visibility or retain a bounded state string, but they cannot
create, remove or rearrange widgets. Modders cannot call egui directly from
Luau. There are currently no authored checkboxes, sliders, dropdown/select
controls, tables, multiline inputs, HTML/CSS or UI animations. Adding one of
these requires extending the verified document schema and its Rust renderer.
See `fixtures/packages/uidemo/assets/ui/welcome.json` for all five kinds.

# Package shape

Place one package directory per identity under a chosen root, containing a UTF-8
`package.txt` and declared `.luau` files:

```text
format 1
package example
version 1.0.0
entry main
dependency arithmetic 1.2.0
module main scripts/main.luau
requires bloxgloom:content/v1
requires bloxgloom:generation/v1
module terrain scripts/terrain.luau
requires bloxgloom:actions/v1
module shift scripts/shift.luau
requires bloxgloom:owner_systems/v1
module clock scripts/clock.luau
```

`ScriptWorker::execute_package` runs the entry from an immutable discovered
snapshot in a fresh, sandboxed Luau VM. Modules use `import("arithmetic:operations")`;
only their own modules and direct dependencies are visible. Errors name the
package/version/module. Sources and imports share one execution and memory
budget. Filesystem access happens only during bounded discovery, off the window
thread. For explicit local development, run
`server-packages <package-root> <address> <save-dir> [max-clients]`. Its startup
entry receives a registration host instead of tick/seed input:

```lua
return function(host)
    host.register_item("example:token", "Token", "bloxgloom:stone")
    host.register_item("example:stone_token", "Stone Token", "bloxgloom:stone", {sprite=false})
end
```

With package format 2 and `requires bloxgloom:content/v1`, a declared PNG can
also become a catalog texture and a placeable opaque cube:

```text
asset texture tile assets/textures/tile.png
```

```lua
host.register_texture("example:tile", "tile")
host.register_block("example:brick", "Brick", "example:tile")
host.register_block("example:hedge", "Hedge", "example:tile", {flammable=true, supports_plant=true})
```

The block has one default state, uniform texture, solid opaque cube geometry,
no special emission/flammability by default, and a same-key placeable item with
the normal 128 stack cap. Registration requires an already registered package-owned
texture; `register_item` remains non-placeable, with a sprite by default. Its
optional `{sprite=false}` argument selects the existing textured item-cube
presentation. `drop_size="small"` or `"large"` selects a bounded client-only
world-drop size; `"normal"` is the default. This affects cube/sprite drops and
pickup-flight meshes, not inventory, pickup eligibility, server age, or world
ownership. Unknown or mistyped item options fail startup, even under `pcall`.
Each package may declare at most 32 textures, 32 blocks and 32 items total.
Keys must use their package's namespace; script and registrar errors abort startup before
opening the save. For `register_block`, the optional fourth argument also accepts
documented geometry, material, support/collision flags, emission/reflectance,
face textures and explicit legal properties/states. See
[content registration](REGISTERED-CONTENT.md) and [the full binding reference](../../SCRIPTING.md#textures-and-blocks).
Unknown or mistyped options abort startup; omitted fields retain the cube defaults.
See `src/server/script/package.rs` and
`src/server/script/startup.rs` for exact syntax, bounds and Unix path
restrictions. With the generation capability, an entry may also call
`host.register_generator("example:terrain", 1, "example:terrain")`; its named
module returns a chunk function using chunk coordinates,
`world_position`, `builtin_terrain_height`, `builtin_base_block`, `random_at`
and `set_block`. `random_at(x,y,z,salt?)` returns a deterministic sample in
`[0,1)`; its optional integer salt is `0..4294967295` and defaults to zero.
The host consumes the exact world seed internally. Bump the declared revision whenever its output changes;
`world.meta` rejects incompatible restarts. Scripts run in fresh bounded VMs
on generation workers and never write neighboring chunks directly.
Local packages may register up to 32 semantic actions under the actions capability:

```lua
host.register_action("example:shift", 1, "Shift", "item", "bloxgloom:stick", "example:shift")
```

### Exact identities and time

Callback entity IDs are immutable host-created `BloxEntityId` handles. Pass
`entity.id` directly to `entity`, `entity_state`, `update_entity`, `remove_entity`,
`schedule_entity`, or inventory services. Action/tick events supply `event.entity`;
pickup candidates supply `drop.id`. An inventory owner is `"player"` or an entity
ID handle, so a transfer can use `c.transfer_inventory(drop.id,0,'player',0,1)`.
These values do not grant access: host ownership, finite inventory, read fences
and transaction checks remain authoritative. Numbers, lookalike tables, printed
labels and revision tokens are rejected where an entity handle is required.

Compare identities with `==`, use them as table keys, or print exact diagnostic
labels with `tostring`. Repeated reads of one identity in a VM share a handle;
the weak handle cache does not retain departed entities. No runtime constructor
is exposed. Userdata is local to its VM; wire/save identities remain Rust values.

`context.tick`, event ticks and message `produced_tick` are `BloxTick` values.
Use `earlier:before(later)` or `later:elapsed_since(earlier)` rather than numeric
halves. Elapsed intervals error if reversed or greater than `2^53`. Revisions are
`BloxRevision` tokens with equality and `:is_initial()`. A chunk owner updates a
captured entity with `c.update_entity(e.id,e.revision,state)` or removes it with
`c.remove_entity(e.id,e.revision)`; the host still checks the captured revision.
Machine inputs supply exact `tick` and `due` values. Creature ticks and public
neighbour views supply `id` handles.

Older word fields and validated word-argument forms remain compatibility paths
for existing modules. Fixtures use exact handles; editor types describe legacy
word fields where useful for event input. New authoring uses handles.

Action callbacks also expose `c.world_time()` and `c.admin_set_time(elapsed_ms)`.
The latter rechecks the authenticated admin actor and stages a daylight phase
in the same WAL transaction as world/entity/inventory effects. See
`verdant:noon` in the combined package for a zero-argument authored command.
Logical ticks remain the scheduling API; daylight clock control is an action service.

Its module returns `function(context, event)`, with scoped `block(x,y,z)`,
`set_block(x,y,z,state)` and exact-stack `transfer(from_slot,to_slot,count)`
on the requesting player's inventory. The existing server authorizes targets,
reach and sessions, and commits successful effects through its shared gameplay
transaction. Script failures abort that transaction; no Lua state survives
between retries. See `src/server/script/gameplay.rs` for limits and fields.
The decision and pickup event shapes below use the same bounded planner.
Client bundle delivery is
implemented; this action callback runs on the authoritative server.

A downloaded `client_startup` may register one own-package client/shared module
with `host.set_replica_handler('package:module')`. Its function receives the
bounded local presentation input and returns the same command list as authored
UI events. Accepted inventory, block, world-assembly and action-result
observations use `replica:inventory`, `replica:block`, `replica:world` and
`replica:action`, with bounded ASCII summaries in `input.value`. The callback
runs on the session-owned presentation worker, never the window/network thread.
Replies may update owned document text/visibility/state but **cannot** request
actions. Observations are advisory and coalesced by kind behind a bounded
queue; overflow or a callback error fails that presentation session.
For `replica:entities`, `input.entities` is a sorted window of at most 16
package-owned mobile views with opaque `id` handles, `key`, and position.
Each view also has opaque `revision` and `motion_revision` tokens, and the exact binary `public`
payload from the installed server replica. These are readonly callback inputs;
the creature's private state is never sent to the client. At most 16 views and
4 KiB of public bytes per view reach one callback.
`input.entered` and `input.left` describe changes to that bounded window, not
authoritative spawn or despawn events. The callback may return at most 16
commands: `visual` pose offsets, `tint` RGB multipliers, short-lived attached
`ember` effects, or colored `spark` effects for IDs offered in that input. A
tint command is
`{op='tint',entity=e.id,r=0.2,g=0.8,b=0.3}`; each channel is
finite and in 0–1. A spark command is
`{op='spark',entity=e.id,x=0,y=0.5,z=0,r=0.2,g=0.8,b=1,size=0.28,lifetime_ms=1200}`;
offsets are finite in −1–1 blocks and RGB channels are finite in 0–1. A spark's
optional `size` is 0.05–0.5 blocks (default 0.16), and optional integer
`lifetime_ms` is 100–2000 (default 850). Embers keep the 850 ms lifetime. Both
effects are client-only, follow the offered entity's rendered position, and
are capped at 32 retained effects per session. Tint changes only
the rendered model, after the host-owned
replica is installed. The next entity callback replaces prior tints and pose
offsets, and switching sessions clears them. Invalid IDs, colors, or commands
fail that presentation session without applying a partial result.
For `replica:anchors`, `input.entities` is a separate sorted window of at most
16 package-owned anchored views. Each view has the same exact ID, key, public
bytes and revision fields; `position` is the anchor cell center and both motion
revision is initial (`e.motion_revision:is_initial()`). `input.value` reports that window's total count, while
`input.entered` and `input.left` track its own bounded window. The callback may
attach `ember` or `spark` effects to offered anchors. `visual` pose and `tint`
commands remain mobile-only; an anchored target for either rejects the whole
batch. Effects follow the currently offered anchor position and disappear after
the next completed callback removes that anchor from the presentation window,
or when the session resets. Empty anchor windows do not dispatch repeatedly.

Under `bloxgloom:actions/v1`, `host.register_handler(key, revision, event,
target, module)` can also register exact-target `BlockRemoved`, `BlockPlaced`,
`NeighborChanged`, or `EntityTick` decisions. These use the same scoped VM and
transaction; `spawn_drop`, `spawn_stack(x,y,z,stack,delay_ms)`, `spawn_entity`,
private owned `entity_state`,
`update_entity`, `remove_entity`, and `schedule_entity` are available where the
host's public context permits them.
`BlockRemoved` supplies the exact `cause` name (`Break`, `Replacement`,
`SupportLoss`, `WorldEdit`, `Burn`, or `AnchoredBreak`), immutable `previous`
block descriptor and a deterministic `random` sample in `[0,1)`. The callback reads
staged block results through `c.block` while the host keeps the original
preimage and all decision effects in one receipt.
`entity(id)` returns a readonly public projection (ID, type, position,
optional anchor and binary public data), not owned private bytes.
`nearby_entities(x,y,z,radius)` captures mobile query dependencies, filters
bucket candidates to the requested spherical radius (0..16), and fails rather
than truncating above 128 results;
`anchored_entity_at(x,y,z)` returns an entity ID handle or nil for absence, including
footprint cells. Public entity reads reflect staged updates/removals but do not
invent IDs for staged spawns. `random(x,y,z,sequence?)` returns a reproducible
sample in `[0,1)`, salted by the dispatched handler key. Its optional sequence
is an integer in `0..4294967295` (default zero); retries reproduce the same
sample without mutable VM state. All query failures poison the
whole transaction, even if caught with `pcall`.
Register exact-length private entity bytes
and a bounded public prefix with `host.register_entity(key, schema_version,
state_bytes, public_prefix_bytes, initial_delay_or_nil)`. An initial delay
requires an `EntityTick` handler; passive types stay out of the due index.
See `src/server/script/gameplay.rs` and `src/server/script/entities.rs` for
details. `PickupRequested` can also be registered for the exact
`bloxgloom:drop` target; its event lists server-eligible drop candidates.
Scoped `inventory(owner)`, `give`, `take` and `transfer_inventory` preserve
exact components and host permissions. Owners are the requesting `"player"`
or an entity-ID table; arbitrary profiles are not accessible. Client pickup
animations do not decide ownership. Fallback handlers remain unbound.
`move_slots(owner, from, to, count)` uses the stock finite-inventory move:
an exact move to an empty slot, a bounded partial merge, or a full-stack swap;
it returns false without changing anything if the move cannot fit or the slot
permissions deny it. The builtin inventory-move binding uses this same public
operation and WAL receipt, rather than a separate private item-move branch.
`collect_drop(entity, max_count)` uses the stock capped, exact-component
slot-order pickup routing and returns the count actually credited. Supply the
event's entity-ID table; host inventory access, pickup delay, destination
filters and transactional validation still apply. A refused destination leaves
those items available for later slots. This does not select candidates or
permit a client to claim a pickup based on animation timing.
The convenience operation returns zero for an out-of-range, delayed or expired
drop even if its entity inventory is otherwise within interaction reach.
`c.player_position` is a readonly 1-indexed `{x,y,z}` sequence of the
authenticated actor's server-captured feet position, or nil for non-player
events. It is not the requested target cell and does not authorize edits or
movement by itself.
`spawn_stack` accepts the exact `{item,count,components}` stack returned by
`take`, so a script can move a component-bearing item from an inventory to a
world drop in one authoritative transaction. The builtin drop-stack binding
uses the same `take`/`spawn_stack` operations; its 1.5-second pickup delay and
the eventual pickup animation do not transfer ownership on the client.
For an authenticated local admin, an action callback can call
`context.admin_give(item_key, count)` (one stack, 1–128; returns false if the
whole grant will not fit) or `context.admin_spawn(creature_key)` (one registered
creature at a host-selected supported position). These are denied for ordinary
players even if a package exposes a button or catches the error with `pcall`;
they do not accept a script-supplied admin token or spawn coordinates. The
builtin `give` and `spawn` console commands use these same registered action
operations and durable receipts. `help` is local console text, not a gameplay
transaction or mod command registry.

`register_storage(entity, block, title, slots, columns, options)` creates a
host-owned inventory screen. `options.footprint={{0,0,0},{1,0,0}}` may declare
one to eight distinct cells with offsets from -2 through 2 on each axis; the
anchor `{0,0,0}` is required. Every cell uses the block's placement state.
The host places and removes the whole footprint atomically, opens the same
inventory from any cell, and refunds the placed item only once. Footprints
participate in V38 client catalog and save identity.

Host-owned storage, kiln and machine screens transfer from the slots that exist
when the server handles the click. The request names a direction, player slot,
container slot and maximum count: left-click asks for up to a full stack and
right-click asks for one. The host moves the lesser of that maximum, the current
source count and destination space. Empty sources, incompatible stacks, full
destinations, invalid slots, denied insert/extract groups and unreachable or
replaced containers reject the move. A scheduled machine tick or another valid
slot change alone does not reject it. The host still commits both inventories
atomically with its own WAL conflict checks and action receipt.

`register_machine` recipes may set `input_components` to `'empty'`
(the default), `'present'`, or an exact `{version=1,bytes='...'}` value.
They may set `output_components` to `'empty'` (the default),
`'preserve_input'`, or an exact value. Fuel entries use the same
`components` predicate as recipe input. Exact bytes are a binary Luau string
of 1–1024 bytes, with a positive 16-bit version. The declared item schemas
must permit the selected component values. These constants and the resulting
slot filters are checked by the host and reconstructed in V39 client bundles;
the machine screen still shows host-owned finite stacks. Processing preserves
the input bytes only when the recipe selects `'preserve_input'`.

A fueled `register_machine` may set `active_state` to another explicit state
key of its registered block, such as `'demo:press[lit=on]'`. The default is the
placement state. While fuel remains, the host changes every footprint cell to
that active state; when fuel runs out it restores the placement state. The
state key must exist on the same block, and a distinct active state requires
fuel. V40 bundles reconstruct both states and their save identity.
Use `block-preview <state-key> <output.png> <package-root>` to inspect either
authored state with the regular renderer.

`register_machine` may instead declare `variants={{state='demo:press[face=north,lit=off]',
active_state='demo:press[face=north,lit=on]'}, {state='demo:press[face=south,lit=off]',
active_state='demo:press[face=south,lit=on]'}}`. The list has two to eight
entries. Its first state must be the block's default placement state; each
state is distinct and belongs to that block. Every variant uses the same
declared `footprint`, recipe, ports and screen. Omitted `active_state` means
the variant's own placement state. A separate top-level `active_state` cannot
be combined with `variants`. Placement selects the variant from the requested
state, and the host persists that choice with the machine. V41 client bundles
reconstruct every variant and participate in catalog/save identity.

A mobile creature tick receives read-only `c.position` and bounded host world
queries: `c.solid(x,y,z)` uses integer cells; `c.clear(x,y,z)` and
`c.grounded(x,y,z)` use feet positions; `c.walk_edge(x1,y1,z1,x2,y2,z2)` checks
a traversable step. The existing `c.route(x,z)` finds a bounded next point.
The tick may make at most 16 world queries, of which at most 8 may be routes.
Any invalid or unavailable query rejects the tick even if Lua catches the
error. These reads use the server's captured terrain; they do not change
authoritative movement directly.
The tick returns `(binary_state, delay, target_x, target_z)` and may add a fifth
`{despawn=true, spawns={{x,y,z}, ...}}` result. Each position is an absolute
feet position within eight blocks of the parent, with at most four children.
Children use the same creature declaration and its initial private state unless
an entry uses `{key='package:other',position={x,y,z}}` to select another creature
declared by that package. The child starts with its own registered initial state.
The host checks terrain and commits the parent and children atomically; invalid
lifecycle data rejects the tick.
`register_creature` may set `reads_neighbours=true` to capture the host's bounded
public entity view. Tick callbacks then receive `c.neighbours`, a read-only list
of `{id,key,position,public}` records; private neighbour state is never
included. `wakes_on_terrain_change=false` disables automatic terrain wakes.
Both declaration options participate in save and client catalog identity.

A package with the owner-systems capability can register one persistent system
with `host.register_system { key, schema, revision, module, partition,
max_state_bytes, max_jobs_per_tick, read_world, read_radius_chunks, seeds }`.
The default `partition='chunk'` retains `{x,y,z,data}` seeds. Entity and profile
systems use `partition='entity'|'profile'` and seeds `{id='fixed-width hex',data}`
with exactly 16 or 32 hexadecimal digits respectively. Their readonly callback
inputs are `c.owner_kind` and `c.owner`, an entity or profile ID handle.
They cannot declare world reads or world effects. `c.wake_entity(system,c.owner)`
and `c.wake_profile(system,c.owner)` carry the exact identity; the host validates destination system
and partition and commits wakes with the owner WAL receipt.
With `read_world=true`, `read_radius_chunks=0` (the default) captures the
owner chunk; `1` captures its 3×3×3 chunk neighborhood. Missing authoritative
chunks defer the complete job while the host requests bounded loads. Reads and
conditional edits outside the declared capture fail, even if caught by Luau;
the client never contributes procedural fallback terrain. Duplicate edit cells
across jobs reject the complete owner wave. `fixtures/neighborhood/` demonstrates
a cross-chunk edit after durable intent delivery. Its module returns a
function that receives an immutable owner context and returns `(binary_state,
delay_ticks)`; the context exposes bounded `block`, `block_info`, conditional `edit` and
durable `wake` methods. The existing owner WAL commits state, deadline, edits
and wakes together. With `read_world=true`, adding `accepts_intents=true` makes
`c.inbox` available as a deeply readonly list of up to eight deliveries with
source chunk, exact revision/ordinal identity, producing tick and binary
payload. `c.send(x,y,z,payload)` sends to a chunk owner of the **same system**;
up to eight sends of 512 bytes each are allowed per invocation. Caught invalid
or over-budget sends still reject the entire plan. Optional
`intent_bootstrap="constant binary state"` lets the host atomically create an
absent destination with that validated state in the producer's WAL record.
Delivery and acknowledgement occur on later ticks with owner state, edits and
wakes; no script VM state is persisted. See `src/server/script/system.rs` for
the precise schema, limits and semantics. Direct chunk-owner entity/drop
effects are bound with explicit declared capabilities. General script state
migration remains unbound.
`c.block(x,y,z)` returns the captured state key; `c.block_info(x,y,z)` returns
the immutable public descriptor `{state,block_type,primary_item,plant,supports_plant}`.
The methods share a 64-read budget and the same unavailable-read failure rule.

Package format 2 declares each module as `module server|client|shared <name>
<side>/<path>.luau` and textures as `asset texture <name>
assets/textures/<path>.png`. Format 1 modules stay server-only. The immutable
client artifact contains client/shared source and declared texture bytes, but
never server modules or original paths. Its SHA-256 cache key verifies exact
canonical bytes; it does not authenticate who supplied that key. Registered catalog PNGs are decoded and
bounded during preparation. Local package
servers now offer the verified bundle before catalog matching and gameplay
admission. The offer includes client host contract version **5** (wire version
**14**): opaque identity handles, authored UI, bounded session replica callbacks,
declared visual resources and [basic runtime tools](RUNTIME-TOOLS.md), including
seeded native randomness and structured logging. An unsupported contract is rejected before bytes
are requested or a cached artifact is acknowledged. This versions the public
host API, not a Luau compiler patch release. Before `ContentReady`, every delivered
client/shared module is compiled, without executing dormant modules; syntax
errors name the package version and module. Validation has a 16 MiB VM budget,
a two-second preparation budget, and the existing 64 KiB per-module/256-module
bundle limits. The client verifies and caches one artifact across reconnects;
matching bundle bytes alone do not grant client catalog compatibility.
Canonical bundle metadata builds a fresh session catalog for the current Luau
startup texture/block/item/action/entity/handler/system identities, including saved numeric
IDs. Server-only code is not downloaded or executed on clients. A format-2
client/shared module named `client_startup` (for example `module client
client_startup client/client_startup.luau`) runs once per connection on a
bounded Luau worker **before** `ContentReady`. Its module returns
`function(host)` and may call `host.set_text("package:document/node", "text")`
or `host.set_state("package:document", "state")` to initialize its own authored
UI. `host.set_parameter(resource, name, value)` initializes a declared owned
material/effect parameter; presentation callbacks can return `parameter` commands
to update it from local UI or public replicas. See [authored visuals](AUTHORED-VISUALS.md)
for typed values, shader hooks and effect graphs. The targets must exist in the verified package UI; the host has no world,
inventory or networking authority. `import("dependency:module")` sees only
client/shared sources in the package and its exact direct dependencies, never
server-only code. Invalid registration or execution aborts readiness with the
package/module in the error. Initial presentation state is reset on each join,
reconnect or server switch; it is not saved gameplay state. The client now opens
its window first and shows download percentage and received/total KiB while one
retained join worker
connects, transfers/verifies packages, negotiates the catalog and runs client
startup. Verified cache reuse is identified separately and transfers no bytes;
100% means the advertised bytes arrived, followed by verification and preparation
before play. A failure displays its attributed reason; **Enter** or the button
retries, **Esc** cancels the active attempt, and **F2** leaves a live session to
edit the server address and join another. Failed/closed sessions retire socket
workers and UI/material/effect/startup resources. Cancellation cannot interrupt
OS DNS/filesystem work immediately, but no replacement worker is admitted until
the previous attempt finishes. The window creates the candidate renderer after the server acknowledges
`ContentReady`; package shader compilation then runs asynchronously while
joining progress remains responsive. The candidate admits snapshots only after
every material/effect pipeline succeeds. There is no persistent disk cache or
arbitrary networking/filesystem service exposed to client scripts. Packages are
delivered directly by the server; marketplace and CDN infrastructure will not be
added.

An authored `ui-document` may opt into client-only presentation events with
`"presentation":{"capability":"local-ui","module":"uidemo:view"}`. The
module must be a verified client/shared `.luau` module and return a function
receiving `{sequence,event,value,state,texts}`. It returns up to 16 commands:
`{op="text",node="uidemo:welcome/title",value="Hello"}`,
`{op="visible",node="uidemo:welcome/icon",value=false}`, or
`{op="state",value="local state"}`, or one
`{op="action",key="uidemo:store"}`. Widget event IDs and action keys must belong
to the document's package. The worker executes each event in a fresh sandbox;
it never receives world/inventory/network handles. The client composes
registered item, empty, block or entity gameplay requests using its current
selection, streamed target and session action ID. A callback may supply up to
130 raw argument bytes for its package-owned action; the server handler must
validate their meaning. Block actions use the current ray hit and observed
chunk version; entity actions use the currently aimed replica identity and
revision. The UI cannot supply coordinates or an entity identity. Server reach,
line of sight, target type, permissions, costs, WAL transactions and receipts
remain authoritative. UI feedback distinguishes unsent, pending, applied and
denied requests. Same-type block replacement rejects stale chunk observations
and rechecks the server read fence at WAL admission. See
`fixtures/packages/uidemo/` and
`fixtures/ui-target-actions/` for item and block examples, and
`fixtures/ui-entity-actions/` for entity targeting with binary arguments.

Player lifecycle services require `bloxgloom:players/v1`. Their public compatibility
identities use a bounded V42 envelope around the canonical client artifact.
Server callback registrations and private initial profile state are not projected
into this envelope. See [PLAYER-LIFECYCLE.md](PLAYER-LIFECYCLE.md).
