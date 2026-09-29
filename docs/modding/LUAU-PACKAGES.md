# Local Luau packages (work in progress)

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
opening the save. For `register_block`, the optional fourth argument accepts
only the boolean `flammable` and `supports_plant` flags. Unknown or mistyped options
abort startup; geometry and light values remain at the opaque-cube defaults.
See `src/server/script/package.rs` and
`src/server/script/startup.rs` for exact syntax, bounds and Unix path
restrictions. With the generation capability, an entry may also call
`host.register_generator("example:terrain", 1, "example:terrain")`; its named
module returns a chunk function using `seed_lo`/`seed_hi`, chunk coordinates,
`world_position`, `builtin_terrain_height`, `builtin_base_block`, `random_at`
and `set_block`. Bump the declared revision whenever its output changes;
`world.meta` rejects incompatible restarts. Scripts run in fresh bounded VMs
on generation workers and never write neighboring chunks directly.
Local packages may register one semantic action under the actions capability:

```lua
host.register_action("example:shift", 1, "Shift", "item", "bloxgloom:stick", "example:shift")
```

Its module returns `function(context, event)`, with scoped `block(x,y,z)`,
`set_block(x,y,z,state)` and exact-stack `transfer(from_slot,to_slot,count)`
on the requesting player's inventory. The existing server authorizes targets,
reach and sessions, and commits successful effects through its shared gameplay
transaction. Script failures abort that transaction; no Lua state survives
between retries. See `src/server/script/gameplay.rs` for limits and fields.
Other gameplay event shapes are still being bound. Client bundle delivery is
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
package-owned mobile views with exact `id_lo`, `id_hi`, `key`, and position.
Each view also has `revision_lo`/`revision_hi`,
`motion_revision_lo`/`motion_revision_hi`, and the exact binary `public`
payload from the installed server replica. These are readonly callback inputs;
the creature's private state is never sent to the client. At most 16 views and
4 KiB of public bytes per view reach one callback.
`input.entered` and `input.left` describe changes to that bounded window, not
authoritative spawn or despawn events. The callback may return `visual` pose
offsets, `tint` RGB multipliers, or short-lived attached `ember` effects for
IDs offered in that input. A tint command is
`{op='tint',id_lo=e.id_lo,id_hi=e.id_hi,r=0.2,g=0.8,b=0.3}`; each channel is
finite and in 0–1. Tint changes only the rendered model, after the host-owned
replica is installed. The next entity callback replaces prior tints and pose
offsets, and switching sessions clears them. Invalid IDs, colors, or commands
fail that presentation session without applying a partial result.

Under `bloxgloom:actions/v1`, `host.register_handler(key, revision, event,
target, module)` can also register exact-target `BlockRemoved`, `BlockPlaced`,
`NeighborChanged`, or `EntityTick` decisions. These use the same scoped VM and
transaction; `spawn_drop`, `spawn_stack(x,y,z,stack,delay_ms)`, `spawn_entity`,
private owned `entity_state`,
`update_entity`, `remove_entity`, and `schedule_entity` are available where the
host's public context permits them.
`BlockRemoved` supplies the exact `cause` name (`Break`, `Replacement`,
`SupportLoss`, `WorldEdit`, `Burn`, or `AnchoredBreak`), immutable `previous`
block descriptor and exact `random_lo`/`random_hi` words. The callback reads
staged block results through `c.block` while the host keeps the original
preimage and all decision effects in one receipt.
`entity(id_lo,id_hi)` returns a readonly public projection (ID, type, position,
optional anchor and binary public data), not owned private bytes.
`nearby_entities(x,y,z,radius)` captures mobile query dependencies, filters
bucket candidates to the requested spherical radius (0..16), and fails rather
than truncating above 128 results;
`anchored_entity_at(x,y,z)` returns two ID halves or nil for absence, including
footprint cells. Public entity reads reflect staged updates/removals but do not
invent IDs for staged spawns. `random(x,y,z,sequence_lo,sequence_hi)` returns
two reproducible 32-bit halves salted by the dispatched handler key; retries
reproduce the same word without mutable VM state. All query failures poison the
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
of `{id_lo,id_hi,key,position,public}` records; private neighbour state is never
included. `wakes_on_terrain_change=false` disables automatic terrain wakes.
Both declaration options participate in save and client catalog identity.

A package with the owner-systems capability can register one persistent system
with `host.register_system { key, schema, revision, module, partition,
max_state_bytes, max_jobs_per_tick, read_world, read_radius_chunks, seeds }`.
The default `partition='chunk'` retains `{x,y,z,data}` seeds. Entity and profile
systems use `partition='entity'|'profile'` and seeds `{id={low_word,...},data}`
with two or four exact unsigned 32-bit words, least significant first. Their
readonly callback inputs are `c.owner_kind` and `c.owner` with the same words.
They cannot declare world reads or world effects. `c.wake_entity` and
`c.wake_profile` carry exact ID words; the host validates destination system
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
canonical bytes; it does not authenticate who supplied that key. General client
module startup remains separate work. Registered catalog PNGs are decoded and
bounded during preparation. Local package
servers now offer the verified bundle before catalog matching and gameplay
admission. The client verifies and caches one artifact across reconnects;
matching bundle bytes alone do not grant client catalog compatibility.
Canonical bundle metadata builds a fresh session catalog for the current Luau
startup texture/block/item/action/entity/handler/system identities, including saved numeric
IDs. Server-only code is not downloaded or executed on clients. A format-2
client/shared module named `client_startup` (for example `module client
client_startup client/client_startup.luau`) runs once per connection on a
bounded Luau worker **before** `ContentReady`. Its module returns
`function(host)` and may call `host.set_text("package:document/node", "text")`
or `host.set_state("package:document", "state")` to initialize its own authored
UI. The targets must exist in the verified package UI; the host has no world,
inventory or networking authority. `import("dependency:module")` sees only
client/shared sources in the package and its exact direct dependencies, never
server-only code. Invalid registration or execution aborts readiness with the
package/module in the error. Initial presentation state is reset on each join,
reconnect or server switch; it is not saved gameplay state. The client now opens
its window first and shows a simple preparation stage while one retained join worker
connects, transfers/verifies packages, negotiates the catalog and runs client
startup. A failure displays its attributed reason; **Enter** or the button
retries, **Esc** cancels the active attempt, and **F2** leaves a live session to
edit the server address and join another. Failed/closed sessions retire socket
workers and UI/material/effect/startup resources. Cancellation cannot interrupt
OS DNS/filesystem work immediately, but no replacement worker is admitted until
the previous attempt finishes. Renderer/GPU installation still runs on the
window thread after the server acknowledges `ContentReady`, before the client
processes any authoritative snapshot. There is no persistent disk cache or
general client services API yet.

An authored `ui-document` may opt into client-only presentation events with
`"presentation":{"capability":"local-ui","module":"uidemo:view"}`. The
module must be a verified client/shared `.luau` module and return a function
receiving `{sequence,event,value,state,texts}`. It returns up to 16 commands:
`{op="text",node="uidemo:welcome/title",value="Hello"}`,
`{op="visible",node="uidemo:welcome/icon",value=false}`, or
`{op="state",value="local state"}`, or one
`{op="action",key="uidemo:store"}`. Widget event IDs and action keys must belong
to the document's package. The worker executes each event in a fresh sandbox;
it never receives world/inventory/network handles. The client composes only
registered item/empty/block gameplay requests using its current slot,
inventory revision and session action ID. Block actions use the current ray hit
and observed chunk version from streamed terrain when the callback is dispatched;
the UI cannot supply
coordinates or an entity identity. Server reach, line of sight, target type,
permissions, costs, WAL transactions and receipts remain authoritative. UI
feedback distinguishes unsent, pending, applied and denied requests. Entity
targets and arbitrary argument schemas are not yet bound. Same-type block
replacement rejects stale chunk observations and rechecks the server read fence
at WAL admission. See `fixtures/packages/uidemo/` and
`fixtures/ui-target-actions/`.
