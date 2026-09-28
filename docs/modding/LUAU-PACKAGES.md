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
```

The block has one default state, uniform texture, solid opaque cube geometry,
no special emission/flammability, and a same-key placeable item with the normal
128 stack cap. Registration requires an already registered package-owned
texture; a normal `register_item` remains a non-placeable sprite. Each package
may declare at most 32 textures, 32 blocks and 32 items total. Keys must use
their package's namespace; script and registrar errors abort startup before
opening the save. See `src/server/script/package.rs` and
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

Under `bloxgloom:actions/v1`, `host.register_handler(key, revision, event,
target, module)` can also register exact-target `BlockRemoved`, `BlockPlaced`,
`NeighborChanged`, or `EntityTick` decisions. These use the same scoped VM and
transaction; `spawn_drop`, `spawn_entity`, private owned `entity_state`,
`update_entity`, `remove_entity`, and `schedule_entity` are available where the
host's public context permits them. Register exact-length private entity bytes
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

A package with the owner-systems capability can register one persistent chunk
system with `host.register_system { key, schema, revision, module,
max_state_bytes, max_jobs_per_tick, read_world, seeds }`. Its module returns a
function that receives an immutable owner context and returns `(binary_state,
delay_ticks)`; the context exposes bounded `block`, conditional `edit` and
durable `wake` methods. The existing owner WAL commits state, deadline, edits
and wakes together. See `src/server/script/system.rs` for the precise schema,
limits and semantics. Entity/profile ownership, neighboring reads, atomic
entity/drop effects, and general script state migration remain unbound.

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
reconnect or server switch; it is not saved gameplay state. Connection setup
currently waits for this worker before the window opens, so startup is not yet
an asynchronously displayed progress flow or a general client services API.

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
registered item/empty gameplay requests using its current slot, inventory
revision and durable session ID. Server permissions, costs, WAL transactions
and receipts remain authoritative. UI feedback distinguishes unsent, pending,
applied and denied requests. Block/entity-targeted controls and arbitrary
argument schemas are not yet bound. See `fixtures/packages/uidemo/`.
