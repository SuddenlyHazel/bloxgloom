# Runtime tags and captured world reads

Gameplay actions/handlers, machine planners, creature ticks, anchored `React`
callbacks and owner planners expose frozen runtime tag queries:

```lua
local is_log = c.tag_contains('block', 'forest:logs', 'bloxgloom:wood')
local members, total = c.tag_members('block', 'forest:logs', 0, 128)
```

Declare tags at startup with `register_tag`; there are no implicit builtin groups.
Queries use resolved, union-composed nested tags from the installed catalog.
`tag_contains(kind, tag, member)` returns membership. `tag_members` returns a
read-only sorted page and total count; offset is zero-based (0..4096), page size
is 1..128, and defaults are 0/128. Kinds are `block` and `item`. Missing tags or
invalid arguments reject the callback, including errors caught with `pcall`.
Content does not change during play. Each callback has at most 64 shared tag and
scheduled environmental queries. Borrowed functions expire at callback exit.

Scheduled machines, creatures, anchored reactions and owner planners also expose
`c.world_time()` and `c.weather()`, matching the gameplay read names. Inputs are
historical immutable snapshots captured before workers run. Clock/weather admin
overrides invalidate stale plans; read reservations protect accepted plans through
the WAL receipt. Natural clock progression does not rewrite an invocation's input.
These APIs do not grant clock/weather mutation permission. Generation stays a
pure seed/chunk operation without real-time environmental inputs; codec/startup
callbacks have no live environment. Daytime alone does not establish sunlight
under a roof or the light level inside a cave.

Gameplay and world-reading owner callbacks expose:

```lua
local cells = c.blocks(x, y, z, width, height, depth)
for _, block in cells do
    -- block.cell is {x,y,z}; block.state/block_type/primary_item/plant/
    -- supports_plant match the point query metadata.
end
```

Coordinates/dimensions must be exact integers, dimensions are positive and total
volume is at most 64 cells. Overflow/volume limits are validated before reading.
Enumeration is X, then Y, then Z (Z varies fastest). Results and cell coordinates
are read-only. Gameplay reads use the transaction overlay, capture the original
preimages, and charge the existing operation budget per cell. Owner reads use the
declared captured chunk neighborhood and share the existing 64-cell read budget
with point reads. Missing/out-of-scope terrain rejects the entire plan; the host
can defer it and request authoritative chunks rather than treating absence as air.
These APIs do not expose private remote inventories or unbounded world scans.

Regression coverage includes sorted tag paging, read-only replies, caught-error
poisoning, expired function handles, box ordering/bounds/unavailable terrain,
clock/weather override fences, and actual loopback gameplay/machine callbacks
with durable rollback and restart.
