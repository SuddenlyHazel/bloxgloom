# Public storage lifecycle boundary

The first implementation of the [historical modding surface plan](../archive/modding/MODDING-SURFACE-PLAN.md)
provides a separately compilable host API and moves Chest's server lifecycle onto
it. It is a declarative storage capability, not yet the complete entity/modding API.

Follow-up: [registered inventory views and screens](REGISTERED-INVENTORIES.md)
now supplies independent container persistence and the full client interaction
path described as future work in the original slice.

## Package boundary

- `crates/host-api`: dependency-free `Extension` / `Registrar` interfaces,
  namespaced cube-content and storage-block-entity declarations, read-only
  lifecycle contexts, and declared placement/removal effects.
- `extensions/lifecycle-fixture`: a separate package depending **only** on that
  API. It registers `fixture:tall_store`, a nine-slot, two-cell storage block.
- `ServerStartup::with_extension`: the development host installation seam. The
  extension receives only a public registrar. The `lifecycle-fixture` development
  feature now installs the package into the local server/client catalogs.

See the fixture's `Extension` implementation for a complete registration example.
It declares namespaced keys, display name/texture, capacity, and footprint; it
does not import server state, numeric IDs, entity stores, or transaction types.

The host resolves declarations before opening world storage. Installation is
atomic: unresolved references, duplicate ownership, invalid bounds, or duplicate
content do not leave a partially modified catalog. Numeric assignments are
host-owned and participate in `content.map`. Extension storage fingerprints
include capacity, references, footprint, and the storage schema identifier.

## Lifecycle contract

`StorageBlockEntity` declares:

- Block type, entity type, placeable item, and anchor state by namespaced key.
- A bounded footprint of relative cells and their expected states.
- Initial empty storage capacity and an optional allowlist of outward cardinal
  automation faces (`None` preserves access from all six faces). An explicit
  list must be nonempty, unique and cardinal. Slot insert/extract permissions
  still come from the registered inventory screen.
- One-item placement cost, one-item removal refund, and release of all contents.

Placement/removal planning receives immutable coordinate/state/footprint
contexts and returns effects. This first capability deliberately fixes storage
ownership rules rather than accepting arbitrary item-producing callbacks.

The host validates the placement item, resident terrain, replaceability, existing
anchors, bedrock, and player collision. Every footprint cell participates in the
prepared edit. Placement consumes exactly one item and creates the entity in the
same atomic transaction as the blocks and any displaced-plant drops.

Breaking any footprint cell validates the entire footprint and entity type,
removes all cells and the entity, and releases the refund and exact stored stacks
in one transaction. The extension cannot invent or suppress the contents used by
this storage service. Item components remain intact. Retry receipts and the
existing write-key/revision checks prevent duplicate application.

Unload is not destruction. The host codec and existing journal/checkpoint path
retain storage without invoking a removal action. Recovery reinstalls registered
codecs before replay. Existing non-lifecycle edit paths continue to reject edits
that would orphan anchored entities; this slice does not add arbitrary system-edit
removal semantics.

## Chest integration

`src/server/entities/chest.rs` supplies the same public lifecycle declaration.
There is no Chest placement or break branch in the shared workstation dispatcher.
`storage_lifecycle.rs` translates registered effects, while the common transaction
helpers perform authoritative validation and assembly for all consumers.

Registered containers use runtime-sized private slot arrays and the same shared
interaction/transfer rules as Hopper. Slot interactions now include **every**
footprint cell in their terrain dependency checks, including across chunk seams.

The initial lifecycle slice preserved Chest's private/public bytes. The subsequent
inventory slice introduced BGCT snapshots and a generic view schema; its default
was `world-v11`, followed by `world-v12` for that dynamic-entity milestone.
Current save defaults are recorded in the root README; these are historical formats.

## Explicit limits / next work

- This storage declaration creates passive block entities. Custom entity payloads
  and behavioral callbacks use the separate [entity](DYNAMIC-ENTITIES.md),
  [anchored behavior](ANCHORED-BEHAVIORS.md) and [gameplay](../../SCRIPTING.md)
  services; the storage shorthand does not expose raw internal interfaces.
- The storage service now uses an independent container format with a **1–54
  slot** bound, including full component-bearing stacks.
- Footprints contain **1–64 unique cells**, with each offset component in
  **-16..=16**. All cells belong to the declared block type; the anchor must be
  explicitly included. Unavailable terrain defers instead of being generated by
  the planner.
- The cube shorthand uses a registered texture and opaque cube defaults. Richer
  registered content uses [explicit block declarations](REGISTERED-CONTENT.md).
- Generic client screen discovery and inventory views are now implemented.
  Storage packages register their screen through the host API; arbitrary custom
  UI composition uses the separate [authored document service](UI-FOUNDATION.md).
- The external fixture is a development/test dependency, not default game content.
  The host can install it without changing lifecycle dispatch or persistence code.

## Verification

At the initial storage milestone, `cargo test --workspace`: **647 tests passed**, with clean workspace formatting
and strict all-target/all-feature Clippy. Coverage includes:

- External registration and changed-schema fingerprints; failed installs leave
  the catalog untouched. Lifecycle collisions with built-in hooks are rejected
  before a world directory or content map is created.
- Cross-chunk placement, last-slot interactions through a non-anchor cell,
  late-tick restart recovery, duplicate placement/removal requests, and exact
  component-preserving refunds.
- Blocked footprints and conflicting prepared placements: no pre-confirmation
  inventory debit, partial footprint, or extra entity.
- Real nonblocking listener, matching extension catalog handshake, production
  client replica assembly, and full fixture place/interact/break sequence.
- Byte-identical old/new Chest encoding, full-capacity refusal, and public
  automation discovery for runtime-sized storage.

The release Chest compact-screen preview was inspected. This is not a live-window
recording. No rendering or meshing algorithms changed.
