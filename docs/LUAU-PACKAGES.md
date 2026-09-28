# Local Luau package snapshots (foundation)

Local package discovery is implemented but **not yet connected to game startup**.
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

This currently registers only non-placeable sprite items with existing builtin
textures. Keys must use their package's namespace; script and registrar errors
abort startup before opening the save. See `src/server/script/package.rs` and
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
Other gameplay events and remote package delivery remain unfinished.

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
details. Pickup and fallback handlers remain unbound.

A package with the owner-systems capability can register one persistent chunk
system with `host.register_system { key, schema, revision, module,
max_state_bytes, max_jobs_per_tick, read_world, seeds }`. Its module returns a
function that receives an immutable owner context and returns `(binary_state,
delay_ticks)`; the context exposes bounded `block`, conditional `edit` and
durable `wake` methods. The existing owner WAL commits state, deadline, edits
and wakes together. See `src/server/script/system.rs` for the precise schema,
limits and semantics. Entity/profile ownership, neighboring reads, atomic
entity/drop effects, and general script state migration remain unbound.
