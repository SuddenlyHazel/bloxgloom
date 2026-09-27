# Registered inventory views and screens

This slice completes the first end-to-end extension container: registration,
placement, persistence, right-click discovery, screen layout, slot requests, and
destruction. Chest, Hopper, and Kiln use the same inventory-view and screen
contracts as extension content.

## Public registration

An extension calls `Registrar::inventory_screen(InventoryScreen { ... })` from
`bloxgloom-host-api`. The descriptor supplies:

- Entity and block namespaced keys, title, and optional help text.
- Slot count, grid columns, and footprint offsets for targeting either half of
  a multi-cell block entity.
- Non-overlapping slot groups covering every slot, with labels and independent
  insertion/extraction permissions.
- Up to four optional bounded status values, formatted as a number, milliseconds,
  or a progress bar relative to a declared maximum.

`InventoryScreen::storage(...)` supplies the standard unrestricted storage grid.
The separate `extensions/lifecycle-fixture` package registers its nine-slot,
two-cell store through this method; it imports only the host API crate.

Descriptors are resolved and frozen into the catalog. Their canonical bytes are
part of entity identity fingerprints: changing a permission, footprint, layout,
or status contract changes compatibility. Manifest ID remapping preserves the
descriptor binding. Both peers install the same package; a server does not send
arbitrary UI code to a client.

Storage lifecycle registration must agree with its screen's capacity, block,
and footprint. Duplicate block/screen ownership, missing references, overlapping
or incomplete slot groups, invalid layouts, and oversized text/status definitions
are rejected before play.

## Generic client and server paths

Right-click discovers an inventory from registered block metadata and resolves
the replicated entity in the touched chunk. Registered footprint offsets replace
the former Kiln/Hopper/Chest recognition branches. Target ID and revision continue
to fence stale interaction requests.

One `UiScreen::Container` handles every registered inventory. Shared layout and
drawing build a grid, player inventory, group labels, optional status widgets,
and help/footer text. Hit testing and keyboard focus use the same descriptor.
The UI cache includes the descriptor, so switching between different-sized
containers rebuilds geometry even though the screen variant is unchanged.

The public inventory projection is versioned and bounded: slot item/count pairs
plus numeric status fields. There is no closed list of machine kinds and no
mandatory Kiln fuel/facing/progress fields on plain storage. Client validation
checks item validity, exact registered shape, and status maxima before accepting
the entity projection. Item components remain private.

Player transfers use the existing identity/revision-checked request and atomic
commit path. Registered storage enforces slot permissions on the server as well
as the client. Its automation port applies the same access flags during public
discovery and authoritative deposit/withdrawal. Kiln retains its specialized
recipe/fuel policy and finished-output automation rules.

Some internal helper/field names still say `kiln`; these no longer select inventory
types or capacities. The legacy Kiln verb adapter remains its own registered
behavior, separate from generic inventory opening and rendering.

## Independent persistence and bounds

Chest, Hopper, and registered passive storage use **BGCT v1** slot snapshots,
independent of player inventory revision and the 36-slot backpack. The format
stores its own capacity and exact stack components. It rejects wrong capacities,
invalid items/counts/components, noncanonical empty slots, truncation, and trailing
bytes. The enclosing journal/checkpoint frame supplies integrity checking.

The current bound is **54 slots** at 128 items per stack. Even every slot carrying
the maximum component payload fits the host's 64 KiB private entity bound. Layouts
support 1–9 columns and at most six rows; status fields and text are bounded to
the current bitmap-font renderer's capabilities. Kiln's existing private recipe
payload remains independent of player inventory storage.

Private storage and public entity schemas changed. The default save is now
**`world-v11`**; no old-save converter was introduced.

## Try the external fixture

```sh
cargo run --release --features lifecycle-fixture
```

This installs the same fixture package into both peers and uses the separate
default `world-v11-fixture/`. In F4 enter `give fixture:tall_store 1`, place it,
and right-click either block. Select a source and destination to transfer; right
click the destination for one item, or left click for as much as fits. E/Escape
closes the screen. Restart to reopen the stored contents.

Generic preview command:

```sh
cargo run --release --features lifecycle-fixture -- inventory-preview fixture:tall_store fixture-previews
```

The feature is an explicit development package installation, not the eventual
mod language, runtime, loader, or distribution design.

## Verification

- Real nonblocking-listener test drives the production client replica assembler,
  screen discovery, focus/layout, and click-to-request path. It opens both halves,
  deposits into the last slot, closes/reopens, restarts the server, withdraws one
  and then the remaining stack, and breaks the block. Replayed actions do not
  duplicate transfers.
- Forged requests cannot insert into output-only slots or extract from input-only
  slots. Registered storage automation respects those flags too.
- Descriptor metadata survives manifest ID remapping, and permission changes
  fail compatibility checks.
- A fully populated 54-slot component-bearing snapshot round-trips independently
  of player storage; maximum-sized compact/enlarged UI geometry stays bounded.
- Release previews of the fixture, Kiln status/roles, and Chest were inspected at
  compact/enlarged sizes. No live-window interaction recording is claimed.
- `cargo test --workspace --all-features`, workspace formatting, and strict
  all-target/all-feature Clippy passed: **652 tests passed**.

One `perf 300 6` check versus the last recorded Chest baseline: scene setup
1691.4 → 1697.5 ms; steady CPU median 0.308 → 0.319 ms; GPU median
0.232 → 0.286 ms. Mesh bytes remain 17,292,744 and visible triangles remain
88,026. These single terrain runs exclude open inventory screens and live
automation; they are not evidence of an inventory performance improvement.

General entity behavior callbacks, named/sided automation ports, and arbitrary
custom UI composition remain later surfaces in the root modding plan.
