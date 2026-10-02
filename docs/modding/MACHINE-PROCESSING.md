# Custom Luau machine processing

Machine tick callbacks may return a `transform` work alternative alongside
`process` and `transfer`. This computes inputs and outputs at runtime instead
of selecting a registered recipe. The callback only proposes a transaction;
the server validates and durably commits the entire owned-inventory change.

```lua
return function(ctx)
    local input = ctx.slots[1]
    if not input or not input.components then
        return ctx.data, 20, false
    end
    local quality = string.byte(input.components.bytes, 1)
    return ctx.data, 20, {{
        kind = "transform",
        inputs = {{slot = 1, count = 1}},
        outputs = {{
            slot = 2,
            item = "demo:finished",
            count = quality > 10 and 2 or 1,
            components = {version = 1, bytes = string.char(quality - 1)},
        }},
    }}
end
```

Slot numbers are one-based. Occupied `ctx.slots` records expose `item`, `count`,
`has_components`, and optional read-only `components = {version, bytes}`. Bytes
are binary strings copied from authoritative slots owned by this machine, never
from client observations or adjacent inventories. Component values are bounded
to 1 KiB and the output item's registered version/size schema still applies.

A transformation consumes one to eight distinct input slots and produces zero
to eight distinct output slots. Input and output lists must be dense. At least
one input item must be consumed. Counts are 1–128; input counts cannot exceed
the captured stack. The host captures exact input preimages, including full
count and components, instead of trusting a script-supplied expectation.

Outputs must name registered items accepted by their destination slot filters.
They merge only with exactly matching item/component stacks, and the resulting
stack cannot exceed 128. An output may target a consumed input slot. Invalid
proposals are rejected; stale input or insufficient output space performs no
partial processing and allows the next work alternative. Only the first
successful alternative runs. All accepted inventory changes use the existing
entity revision fences and WAL transaction, including replay/restart behavior.

Registered machine recipes/fuel still define the current two/three-slot layout,
item allowlists and component permissions. `transform` does not require recipe
matching or recipe pulse counts, but does not widen those startup declarations.
Packages are responsible for their authored production balance; the host retains
slot ownership, component schema, capacity and atomic commit guarantees.
