# Registered content and composition

The dependency-free `bloxgloom-host-api::{content, composition}` modules are
startup declarations, not a plugin loader or an arbitrary renderer API. Use
`Registrar::{texture, block, item, tag, package}` inside `Extension::register`.
`CubeBlock` remains a shorthand for an opaque cube, one legal state, and its item.
The separately compiled `lifecycle-fixture::content::Content` demonstrates the
full declarations without importing engine types or assigning numeric IDs.

The Copper Lamp's unlit top/bottom deliberately use the fixture's tiny copper
checker PNG (`fixture:copper_checks`), also used by Copper Reed. This is diagnostic
fixture art, not a missing-texture fallback. Its lit states use glowstone art;
ordinary placement selects the unlit state.

## Existing capabilities exposed

* PNG textures: up to 4 MiB, at most 2048×2048, RGB/RGBA after PNG expansion,
  edge/vertical stitching, alpha-cutout preprocessing, and surface emissive
  strength 0–16 (separate from voxel lighting). Glowstone's existing 3.5 radiance
  is now registered, not a shader key/texture-layer exception. A frozen 4-byte
  scalar per texture feeds the common cube/cutout shader in constant time.
  Embedded builtin art
  uses the same public metadata with a private trusted-byte fast path; external
  art is always decoded and validated before installation.
* Blocks: display name/swatch, top/side/bottom texture references, opaque/cutout/
  invisible material, solid/empty voxel collision, replacement, plant support,
  flammability, emission 0–15 and RGB reflectance. Invisible is **not** alpha
  blending: it emits no terrain geometry, matching the existing air material.
* Shapes: cubes and double-sided crossed foliage quads. Cube selection is the
  voxel. Normal/narrow plant selection is the existing centered .56/.30-wide,
  .9-high box; both plant profiles have empty collision. The old tall-grass key
  check is now `Geometry::NarrowCrossedPlant` in builtin registration.
* Up to eight finite properties and 4096 explicit legal states per block; the
  host sorts property pairs into canonical state keys. The existing `axis=x/y/z`
  convention rotates cap textures. States may override textures and emission,
  just like kiln variants; arbitrary state-dependent collision is not an
  existing capability. Missing combinations are illegal rather than generated.
* Items: independent namespaced identity, display name/swatch, texture, sprite
  versus block-item cube, and optional exact placement-state reference. Stack
  capacity remains 128. Item actions/harvest policies are separate contracts.
  Non-placeable Luau items can set `drop_size` (`small`, `normal`, `large`) and
  `drop_animation` (a table of bounded `pop_duration`, `pop_height`,
  `hover_amplitude`, `hover_speed`, `spin_speed`, `pickup_duration`, `pickup_arc`,
  `pickup_turn`). Omitted fields keep the stock animation. These are negotiated
  client presentation only: age and pickup events, item counts, and ownership
  stay server-authoritative. `DropAnimation` is also available to Rust items.
* Components: `Unstructured` preserves builtin behavior (nonzero version,
  1–1024 opaque bytes), `None` forbids components, and `Opaque` declares a version,
  schema fingerprint, byte maximum and whether a payload is required. The host
  checks envelopes, **not** extension-specific byte semantics. Exact bytes
  continue through existing equality, transfers, inventory/container/drop codecs
  and persistence; this does not authorize item creation or conversion.
  Voxel placement records a block state, not item bytes, and its existing refunds
  construct plain items. Therefore required-component items cannot be placeable;
  optional component bytes are not promised to survive placement and harvest.
  Required-component pickups need an explicit component-producing authority,
  rather than a plain-item grant or recipe.

The original sixteen block definitions, all original block items/non-block
pickups, and builtin texture metadata now use these public descriptions. Reserved
IDs and the remaining workstation registration helpers use the same internal
compilers. They have no additional block/material/geometry capability. Cube
shorthand also goes through this compiler rather than a separate restricted path.
The frozen catalog remains the source for meshing, material upload, light,
collision, selection, placement and inventory validation.

Builtin kiln fuel derivation runs again after extension items are compiled.
Thus a placeable flammable extension item participates in the builtin's existing
60-pulse fallback, while stick/sapling/wood retain 40/80/240 pulses. Required
component-only items are not incorrectly admitted to this plain-item process.

## Composition and deterministic resolution

`Package` declares an exact nonzero contract version, exact-version dependencies,
and required named host capabilities. `bloxgloom:core` version 1 is implicit.
Supported keys are the constants in `composition`: content, passive storage,
machines, mobile entities, and inventory screens. Unsupported requirements,
missing/mismatched dependencies, cycles and duplicate package keys fail startup.
No semver solver, package discovery, runtime selection or optional loading occurs.

Use `composition::Bundle(&[&extension_a, &extension_b])` for dependent extensions.
All declarations are collected first, so a consumer may precede its provider.
Textures, blocks, states, items and identities are compiled in canonical order
within that bundle, not callback order. Ownership is singular: duplicate block,
item, entity, lifecycle or screen bindings are rejected, never overwritten.
Behavior ordering inside a single declared machine/model is still that explicit
declaration's contract, not extension load order.

Tags have a namespaced key and block/item kind. Multiple contributors in the
same bundle union their members; nested same-kind tags resolve transitively.
Cycles and missing members fail. Machine filter allowlists accept
`#namespace:item_tag`, expanded to exact keys before the runtime builds its
constant-time slot membership sets. Expansion above 4096 entries or an empty
explicit filter tag fails rather than accidentally admitting every item.
Block tags currently provide validated, persisted composition metadata; general
world-system tag queries are outside this content-registration slice.

A later startup installation may reference already installed packages/content,
but may not replace owners or contribute to an already installed tag. Submit all
tag contributors in one bundle: rejecting late contributions prevents silently
stale previously compiled consumers. There is no mutation after catalog install.

## Bounds, atomicity and compatibility

One collector retains at most 4096 declarations. New content descriptions have a
conservative 64 MiB aggregate collection budget; per-description lengths are
checked before traversal/copying nested members. Existing catalog definition/ID
limits still apply. There are at most 256 packages and 1024 tags, with at most
65,536 total declared and resolved tag memberships. Package/tag graph validation
is iterative, not recursive on extension-controlled depth. Native Rust callbacks
are trusted code; this bounds host collection/compilation, not callback CPU or
memory allocation. Runtime isolation belongs to a future adapter.

Installation builds a private candidate catalog. Reference resolution, content,
screen/machine/lifecycle validation and composition all succeed before replacing
the startup catalog. Failure creates no save and does not partially install
definitions. Retrying a failed installation is an ordinary startup retry;
successful definitions cannot be overwritten by retry. No worker completion
timing participates in registration or identity assignment.

Manifest entries now also use `P` (packages), `T` (block tags), and `U` (item tags).
Their assigned IDs, fingerprints and every supplemental table survive manifest
reconstruction, save-local remapping and client negotiation. Components,
selection profiles, display metadata, resolved tags and package contracts enter
compatibility identity; assets already enter material/catalog fingerprints.
No inventory, block palette or component wire encoding changes. Builtin numeric
IDs are preserved, but expanded fingerprint coverage changes compatibility: the
combined release must advance its default world-folder version rather than
convert existing prerelease saves.

## Verification and limits

Focused tests exercise the production registration seam, out-of-order bundles,
cycle/missing-reference failures, manifest remapping of all new tables, component
codec rejection, registered selection, light/material/mesh compilation and kiln
interoperability. The fixture Crusher's filter uses an item tag in its existing
real-listener processing/restart test. The additional real-listener storage test
moves `etched_chip` stacks with registered version-2 components, reconnects,
withdraws and checks exact recovered bytes.

This surface does not invent arbitrary block meshes, partial collision boxes,
translucent shaders, liquid physics, per-stack render callbacks, sound, or custom
component interpreters. Those are not current builtin content capabilities.
General gameplay hooks, process transformations, world systems, UI actions,
world generation and mod discovery remain their own host surfaces.

**UI integration gap:** `ui/draw.rs` still selects its small hand-authored HUD
bitmap icons by builtin item key. Registered texture/sprite presentation covers
world meshes and drops; it does not yet provide those inventory bitmap icons.
That key dispatch belongs to the UI registration workstream and must be migrated
before claiming exhaustive item-presentation parity.

For a direct GPU check of registered terrain art (including non-builtin texture
layers and emissive surfaces), use:

```text
cargo run --release --features lifecycle-fixture -- block-preview 'fixture:copper_lamp[axis=x,lit=false]' lamp.png
cargo run --release --features lifecycle-fixture -- block-preview fixture:copper_reed reed.png
```

The focused preview uses the installed catalog, normal voxel light/meshing,
opaque/cutout pipelines and post-processing. Standard UI previews still use
their builtin-only UI catalog; they are not evidence of external HUD-icon parity.
## Integration additions

`Registrar::item_icon` registers bounded bitmap HUD art (up to 32×32 pixels,
32 palette symbols). Built-in handcrafted icons and the external Etched Chip use
the same frozen catalog lookup, manifest remapping and compatibility fingerprint.
The capability is `bloxgloom:item_icons/v1`. Anchored entities, actions, and
owner-local systems also have explicit versioned package capability identifiers.
Recipe/fuel constants are checked against registered component schemas; dynamic
component-preserving output is validated before input or fuel is consumed.
