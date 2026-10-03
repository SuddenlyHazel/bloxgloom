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
  flammability, emission 0–15, sky attenuation 0–15 and RGB reflectance. Invisible is **not** alpha
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
  `drop_policy` independently sets bounded authoritative `gravity`,
  `terminal_speed`, collision `radius`, `pickup_range`, `merge_range`, and
  `lifetime_ms` (1 second–24 hours). Omitted fields retain stock behavior.
  The policy is part of item save/catalog identity and the verified client
  metadata, but the server alone decides motion, merges, pickup and expiry.

Local Luau `register_block(key, name, texture, options?)` currently registers
one-state cubes or crossed plants. Its optional table supports `flammable`,
`supports_plant`, `solid`, `replaceable`, `emission` (0–15),
`sky_attenuation` (0–15 sky levels absorbed per voxel; default 0, opaque blocks always stop sky),
`reflectance` (exactly three 0–255 channels), and `side`/`bottom` face texture
keys. `geometry` selects `cube`, `crossed_plant` or `narrow_crossed_plant`;
`material` selects `opaque` or `cutout`. A crossed plant requires cutout
material and `solid=false`. `register_texture(key, asset, {alpha_cutout=true})`
declares a cutout PNG; every cutout block face must use a cutout texture.
Face textures must already be registered by the same package; the required
third argument supplies the top texture and the default for other faces.
`properties={lit={'off','on'}}` together with
`states={{lit='off'},{lit='on',emission=12}}` declares a bounded explicit state
set (up to 8 property names, 16 values per name and 32 legal states). No
Cartesian product is implicit; `set_block` and property transitions accept only
listed combinations. State names are canonical and sorted, for example
`package:lamp[lit=off]`. The block's item places the lexicographically first
legal state. A state may also override all three faces with
`textures={top='package:top',side='package:side',bottom='package:bottom'}`.
Each face must already be a registered texture of the same package, and cutout
blocks must use cutout face textures. V28 bundles preserve these overrides;
unspecified states are rejected rather than synthesized. Invisible materials
still require a broader Luau binding. Do not
mistake this shorthand for the
entire public Rust `Block` contract.

Luau `register_item(..., {components={version=1, fingerprint="000000070000002a",
max_bytes=64, required=true}})` declares a bounded opaque
component schema. The fixed-width 16-digit hexadecimal fingerprint forms a nonzero
schema identity; increment it when the byte format changes. The host checks
version, length and requiredness on inventory creation/transfer, retaining exact
bytes and the 128-item stack cap. Component bytes are not interpreted as public
client logic. A V27 bundle negotiates this schema; default items retain their
older bundle bytes.

With `content/v1`, `storage/v1`, and `inventory_screens/v1` requirements,
`register_storage('package:chest', 'package:chest_block', 'Chest', 9, 3)`
binds a previously registered package block to a one-cell, host-owned storage
entity and a negotiated inventory screen. Placement uses the block item's
canonical default state. The host owns slots, transfers, removal refunds and
contents; Luau cannot supply or mint their contents. V29 bundles carry the
same screen and storage identity to joining clients.

`register_tag(key, "item" | "block", members)` contributes a same-kind tag at
startup. Each member is a namespaced definition key or a nested
`"#namespace:tag"`; forward references resolve when the full package set is
installed. Names are owned by the declaring package, with at most 32 tags and
32 unique members per package/tag. The catalog rejects missing references,
wrong kinds and cycles. Tags participate in saved and negotiated identities;
the V24 bundle carries inert tag declarations, not executable callbacks.
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

## Luau package capacities

The sandboxed startup adapter admits 256 blocks, 512 total items (including one
item for every block) and 256 textures per package. These are local declaration
limits; installation-wide native catalog ceilings, builtin consumption and GPU
preparation limits remain independent. The 2 MiB general package asset ceiling
may be smaller than a native asset-kind bound. Server discovery and client
metadata decoding consume the same shared capacity policy. Invalid or caught
startup declarations reject the complete candidate before world/save mutation.
See [package composition](PACKAGE-COMPOSITION.md) for execution, delivery and
resource admission, and [farming scale](../../fixtures/farming-scale/README.md)
for a 128-block/192-item package with independent simulation features.

### Thin foliage daylight

Texture declarations may opt in to thin-surface shading without changing generic
cutouts: `host.register_texture("example:leaves", "leaves", {alpha_cutout=true,
foliage_wrap=0.35, foliage_transmission=0.28})`. Both foliage values are finite
numbers in 0–1, defaulting to zero. Wrap softens the diffuse silhouette; transmission
adds a bounded back-side sun response. Both remain gated by voxel sky visibility
and sun shadows. Emission and cave lighting are unchanged. Native extensions use
`Texture.foliage: FoliageShading` with the same bounds.

Block option `sky_attenuation=2` reduces vertical sky level by two units (range
0–15). Zero retains transparent behavior; opaque geometry always blocks sky.
This is separate from material shading: glass, fabric, or other cutouts are not
automatically treated as foliage. Crossed plants sample the voxel light field at
each vertex while retaining upward artistic normals, without additional AO.
Nondefault lighting metadata is transmitted as bounded, fingerprinted V52 client
bundle data; ordinary packages retain their existing bytes.
