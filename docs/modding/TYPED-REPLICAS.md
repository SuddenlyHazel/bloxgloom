# Typed client replica snapshots

## Accepted implementation scope

Expose `input.replica` to authored UI callbacks, replica handlers and UI-free
visual handlers. It is a readonly copy of data already delivered to this client:
the local player's inventory, recent installed block changes, server clock
samples and terminal receipts for actions owned by the callback's package.
Server authority, frozen catalogs and presentation worker isolation remain in
effect. This adds no inventory/world mutation authority or private remote-player
data access.

Unknown inventory and clock data are `nil`. A known inventory is a dense sequence
of all 36 slots, including empty slots. Each record has a zero-based `slot` and
an optional `stack` with namespaced `item`, `count` and optional `{version,bytes}`
components. Component bytes are binary strings, preserved exactly; stack counts
remain capped at 128 and component payloads at 1024 bytes.

Inventory `revision` and block `revision` are exact revision handles, accompanied
by `revision_lo` and `revision_hi`. Do not combine the halves into a Lua number
when the revision may exceed its exact integer range. World `elapsed_ms` and
`cycle_ms` are plain numbers within the twenty-minute clock cycle. The clock is
the last server sample, without client interpolation.

`blocks` contains at most 64 recent installed cell states, each with
`position={x,y,z}`, namespaced `state` and its chunk revision. A complete commit
must install before any of its cells enter this window. Retained cells refresh
after replacement snapshots and disappear when their chunk is evicted.
`blocks_truncated` reports omitted/evicted observations. This window is neither
a full chunk snapshot nor an exhaustive edit history; procedural fallback never
supplies these records.

`actions` retains at most 16 local terminal receipts, oldest first. Each has an
exact 32-character hexadecimal `id`, package-owned `key`, boolean `accepted` and
server `reason`. Duplicate receipt delivery does not create a second record.
Native actions and other packages' receipts are omitted. These receipts are
presentation data; the server owns acceptance and inventory effects.

## Update and lifecycle semantics

Existing `replica:inventory`, `replica:block`, `replica:world` and `replica:action`
events announce changes. Legacy `value` summaries remain available. Pending
events coalesce by kind; a callback receives the latest accepted snapshot when
it is dispatched, rather than an exhaustive historical event stream. A running
callback keeps its immutable snapshot even if new packets arrive.

Opt into additional clock, block-installation and UI-free observation events
with `h.set_replica_handler("package:replica", true)` during client startup.
Omitting the second argument preserves the existing notification surface,
including entity-only notifications for UI-free handlers. All presentation
callbacks can read `input.replica` regardless of this event opt-in.

Inventory, block and receipt streams carry independent revisions and ordering.
An accepted receipt may arrive before its inventory snapshot. UI should refresh
on subsequent observations and must not infer an inventory revision from the
receipt alone. Unknown data must not be treated as a known empty inventory.

Closing/cycling UI documents retains current connection observations. A new or
retired connection clears observations and pending callbacks; reconnect populates
them from fresh authoritative packets. Editable local UI state remains scoped
to its document owner, while inventory and public world observations describe
this local client's permitted view.

The recipe browser is the acceptance example: it displays component-free input,
a maximum craftable quantity from a matching input slot and actual applied/denied
receipts. The estimate includes output space freed when the craft consumes the
whole input stack; smaller quantities can still fail in a full inventory.
Its server callback still validates selected input, exact components, quantity
and output capacity. Predictions do not authorize crafting.

Arbitrary world queries, remote inventories, complete event histories,
larger-package composition and save converters are outside this scope. Retained
replica callback state is described in [VM lifetime](VM-LIFETIME.md).

## Verification

The unrestricted `cargo test --workspace` run passes all 1,215 tests (1,179 game
and 36 host API), with no exclusions. Regression coverage includes readonly
nesting, exact binary component bytes and revisions, unknown versus empty
inventories, stale/duplicate packets, package receipt ownership, terrain-action
receipt decoding, complete-commit publication and reconnect/retire cleanup.

The packaged recipe browser runs through its actual startup and presentation
worker. The real nonblocking TCP listener acceptance test verifies crafting,
capacity rollback, component rejection, duplicate replay, inventory conservation
and restart persistence. Its simulated UI inputs wait for admitted replica
callbacks to finish, matching the live controls' busy-state gating.

Browser predictions also cover full-stack consumption freeing an output slot,
and a larger input stack remaining blocked by the eight-item recipe limit.
An accepted receipt and a later inventory observation are handled independently.

Formatting, strict all-target/all-feature Clippy and fixture Luau analysis pass.
Graphify's code graph is refreshed. Production egui previews were generated and
inspected at 1280x720 and 640x360 for unknown data and a filtered representative
post-craft snapshot. The desktop view shows ingredient availability, world time,
component bytes and the separate status/receipt-ID labels; the compact view
keeps lower controls in its scrollable body. The offline preview sample does
not replace the real-listener acceptance test.

Reproduce the acceptance checks with:

```sh
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo run -- egui-preview /tmp/bloxgloom-typed-replicas-ui fixtures/recipe-browser/packages
```
