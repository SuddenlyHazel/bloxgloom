# Native GLB player

The installed player uses [master/model.glb](master/model.glb) directly through the
Rust glTF loader and the instanced wgpu character renderer. The in-game Character
menu, third-person players, first-person body and sun casters share this asset.
All textures come from its embedded PNG images. The old articulated meshes,
separate texture files and pixel face overlays are retained only as authoring
history; no runtime asset or catalog fingerprint reads them.

## Appearance

Pause → Character previews the same renderer used in play. Apply remains a
server-authoritative acknowledged operation; Close discards unapplied changes.

- Flat chest or defined chest with sports bra.
- Thirteen hairstyles plus no hair, arbitrary sRGB hair color.
- The GLB's modeled eye design with optional iris RGB; no mouth variants in this
  export. Eye and mouth selectors are absent until authored variants exist.
- Registered skin/pants/shirt palettes remain available. Pants color the shorts;
  shirt palettes color the sports bra. Flat chest has no upper garment.
- Hair color multiplies neutral painted shading; accessory materials retain their
  original colors. Custom iris RGB replaces only iris colors, preserving alpha.

## Geometry and movement

The immutable GLB and PNGs decode before the window event loop begins. Static
cube transforms are folded into vertices attached to the thirty named actor
joints. This preserves the GLB's hierarchy and clips without evaluating hundreds
of static cube nodes every frame. Vertices and native-size textures upload once.
Admission remains nearest-first and capped at 512; grouping only reorders actors
already admitted. Only the selected body and hair are submitted for each actor.

Idle, walk, run, crouch and both tool clips use the baked GLB tracks. State blending
happens in local TRS; walk/run cycles stay phase-aligned despite different clip
durations. Grounding uses the actual foot/toe vertices and is presentation-only.
First-person arm framing applies one rigid delta to each entire arm chain,
including the grip. Sun casters use the original world pose. Root motion and the
four validation clips remain available for inspection. Collision, eye height and
movement remain server-owned.

## Verify

```sh
cargo run -- character-preview character.png idle 0.35 1
cargo run -- character-style-preview styles.png
cargo run -- first-person-preview first-person-frames
cargo run -- third-person-preview third-person-frames
cargo run --release -- character-perf 300 128 8
cargo run --release -- perf 300 6
cargo test
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

Tests compare the live compiled vertices against native GLB samples, and cover
first-person attachment, head limits, crouch height, planted feet, independent
colors, shadows, population bounds and authoritative network/restart behavior.
See [the native model guide](../../../docs/modding/AUTHORED-MODELS.md) for generic
GLB previews and color multiply/replace controls.

## Save and wire boundary

The default is **world-v23**. Earlier directories remain untouched and incompatible
catalogs fail before opening player data; no prerelease converters are provided.
Character recipe v3 retains the bounded 12-byte public recipe structure, with one
supported eye and mouth ID. Player payload schema 4 and the exact model/controls
fingerprint prevent peers with different installed models from joining. Old
recipe versions are rejected, never silently reset. Appearance persistence and
inventory ownership remain keyed by stable profile ID.
