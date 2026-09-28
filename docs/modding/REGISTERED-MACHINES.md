# Registered inventory machines

`Registrar::machine` in `crates/host-api` registers scheduled anchored inventories.
Kiln and Hopper run through this adapter in production. `fixture:crusher` is a
separately compiled consumer of the same API; it imports no engine internals.

## Trying the fixture

Run `cargo run --release --features lifecycle-fixture`. The development world is
`world-v13-fixture/`; ordinary builds use `world-v13/`. In F4:

```text
give fixture:crusher 1
give bloxgloom:hopper 2
give bloxgloom:stone 8
give bloxgloom:stick 2
```

Place the Crusher and right-click it. Insert sticks in FUEL and stone in STONE.
Three processing pulses turn one stone into two gravel. The screen publishes fuel
and progress. A Hopper above feeds its top `feed` port; a Hopper below pulls from
its bottom `product` port. Other faces reject automation. Breaking the Crusher
refunds its placement item and remaining exact stacks, including unburned fuel.

## Contract

- Register the block/item and `InventoryScreen`, then declare its machine type.
  Registration resolves keys and checks slot counts, references, screen footprint,
  status ranges, filters, and recipes before the catalog is installed. The manifest
  fingerprints machine metadata; `schema` identifies changes to callback semantics.
- `Variant` maps a placement state to matching idle/active footprints. Placement
  consumes one registered placement item; removal from any footprint cell refunds
  one item and contents in the same transaction as world/entity changes. The active
  footprint represents remaining burning fuel; unfueled machines use idle states.
- The host owns 1–54 slots, capped at 128 items per stack. Filters use namespaced
  item allowlists (empty means any item) and explicit component acceptance. Manual
  access additionally follows screen groups; automation follows named ports and
  cardinal world-space faces. Passive stores expose an all-sided `storage` port.
- A `Behavior` receives immutable current slots, due/current ticks, fuel/progress,
  and up to 1 KiB of durable private bytes. It returns replacement bytes, a later
  deadline, and up to eight ordered work alternatives. The first process operation
  or available transfer wins; at most one runs per invocation. Empty work updates
  only private data and schedule. Callbacks are pure, deterministic, trusted Rust
  functions, scheduled through the existing worker/coordinator barriers.
- `Process` selects a registered single-input recipe, optionally burns registered
  fuel, and produces an explicit output quantity. Recipes and fuels consume only
  component-free stacks. Exact component stacks can be stored/transferred when
  allowed, but processing does not strip or silently reinterpret their components.
  A full output blocks production and new fuel consumption; existing fuel keeps
  burning. Recipe/fuel lookup and item filtering use resolved lookup tables.
- `Transfer` names the local port, an optional peer port, a cardinal adjacent cell
  offset from the anchor, and push/pull direction. Discovery is advisory. Commit
  rechecks both footprint cells, facing permissions, authoritative slots, exact
  withdrawn components, and both revision preimages before one atomic WAL batch.
  One item moves per pulse. The peer's schedule is preserved.
- Private data, slots, fuel/progress, schedules, terrain states, and publication
  use the existing durable entity path. Clients receive bounded inventory and
  status projections only after confirmation. Shared UI requests retain reach,
  revision, action receipt, and slot-permission checks.

The built-in Processor advances from its persisted deadline, retaining Kiln's
catch-up behavior. DownwardFlow advances from the current tick, pushes below first,
then pulls above, retaining Hopper's one-item/20-tick behavior.

## Verification

The real nonblocking-listener test uses production client inventory requests to
place the independent Crusher, supply fuel/input, retry a duplicate request,
observe output, take output, feed it from a Hopper, restart, extract into another
Hopper, and break/retry the break. Recovery checks exact placement-item and unburned
fuel refunds. Durable tests reject forged ports and wrong faces without moving
items. Existing Hopper conflict/restart tests now use generic machine payloads.
An oracle test compares the migrated process against the previous Kiln's fuel,
progress, and production rules; codec tests retain component bytes.

## Remaining boundaries

This is an inventory-machine contract, not the complete arbitrary anchored-entity
API. Initialization currently uses empty slots/private bytes, placement costs and
refunds are one item, and projections are inventory plus standard process status.
Custom state codecs/projections, arbitrary lifecycle callbacks/costs, component
recipe predicates, multi-input recipes, and arbitrary world edits remain future
surfaces. Block/state asset registration also remains narrower than the internal
built-in catalog. Port faces are world-space, with no facing-relative transform.
Offer selection still uses item identity and port order rather than a public
opaque component/slot selector. Full capability parity remains in progress in
the historical `docs/modding/history/MODDING-SURFACE-PLAN.md`.
