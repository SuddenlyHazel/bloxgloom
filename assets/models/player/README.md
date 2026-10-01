# Authored player vertical slice

This directory contains a reproducible native representation of the project's
authored character kit. The original game-facing GLBs are kept under `source/`:

- `player.glb`: refined-crouch six-cuboid player, seven-joint rigid skin, with the
  existing styled **cute_glint / soft_smile** face baked into its 512 × 256 atlas
- `hair.glb`: **tousled_crop** rigid head attachment, independent 32 × 32 texture
- `hair_undercut.glb`: **side_swept_undercut**, another short head-local attachment
- The remaining named `hair_*.glb` files: all approved short, curly, variety and bold rigid styles (IDs below)

These are the supplied project assets, not downloaded third-party models. The
body geometry, UVs, five clips, and painted texture pixels are retained. No
generated concept art or browser/Three.js runtime is needed by the game.

## Rebuild

From the repository root:

    python3 tools/character_assets/convert.py
    python3 -m unittest discover -s tools/character_assets -p 'test_*.py'

The converter uses Python's standard library only. It copies embedded PNG bytes
exactly and writes deterministic body/rig `character.json`, per-clip `clip_*.json`,
and per-hairstyle `hair*.mesh` buffers. Unsupported glTF extensions,
mesh transforms, weighted skins, morph targets, and non-linear samplers are
rejected rather than approximated. This is an intentionally narrow importer for
the authored kit, not general glTF support.

Each `.mesh` begins with `BGH1`, little-endian u32 vertex and index counts,
then eight little-endian f32 values per vertex (position, normal, UV), followed
by local u16 triangle indices. The material ID comes from the append-only table;
all attachment vertices use the head joint. Bounded decoding validates byte
lengths/counts before allocation and validates geometry before GPU upload.
No float quantization or runtime GLB importer is involved. The largest mesh is
144,444 bytes; source GLBs remain checked in for reproducible conversion.

## Runtime contract

`character_asset.rs` validates the normalized data once at initialization. A
vertex is joint-local: its inverse-bind transform has already been applied.
Rigid hair vertices use the head-local identity socket. Runtime sampling returns
seven global joint transforms, including a uniform 0.9 scale and a 180-degree Y
rotation: source is 2 m tall, Y-up, -Z-forward; engine is feet-origin, 1.8 blocks
tall, Y-up, +Z-forward. Do not apply inverse binds or this basis a second time.

The source has both a joint and a mesh named `head`; the attachment is joint 1.
Preserve legacy arm names: `right_arm` lies at negative source X, `left_arm` at
positive source X. The names are reversed relative to the textured -Z face:
`tool_use_left` animates legacy `right_arm` (source -X, anatomical left);
`tool_use_right` animates legacy `left_arm` (source +X, anatomical right).
These mappings were checked against the source clip channels. Do not rename
channels by guessing from legacy bone labels.

Material 0 uses `body.png`; material 1 uses `hair.png`; material 2 uses
`hair_undercut.png`; materials 3–13 use their matching named PNGs. Only the selected
hair material is drawn. Keep body and hair at their native sizes, nearest-filtered
and sRGB-decoded. None uses emission. The character
atlas must not pass through the terrain's 128 × 128 resampler.

The clips are `idle`, `walk`, `crouch`, `tool_use_left`, and `tool_use_right`.
Idle/walk loop; crouch clamps at its held final frame; tool clips finish at their
authored returned-to-rest pose. All are visual-only, with no gameplay root
motion. The crouch clip does not change collision or create a crouch mechanic.

## Native character editor and multiplayer

Open **Pause → Character**. Choose Classic or Authored; authored selections expose
fourteen hair choices (none plus the thirteen styles listed below), eight eye styles,
six mouths, and an optional RGB iris color. Untinted irises retain the original
per-style artwork. Closed eyes are unaffected by tint; temple accents and all
body pixels remain unchanged. Neon-looking details are painted, never emissive.

The animated portrait uses the production skinning/material path. Its animation
selector previews idle, walk, held crouch and both anatomical tool actions.
Changing controls edits a local draft. **Apply** sends only a bounded recipe for
the admitted session's player; “Saved” appears only after the authoritative own
replica echoes it. Closing discards unapplied edits. Pending Apply is disabled
and survives closing/reopening; disconnect clears the local pending state.

The server validates IDs, atomically persists the profile's full appearance, and
replicates it to peers and late joiners. Reconnect and server restart restore the
recipe. Identical retries are no-ops. Existing palette selection/Luau operations
retain their three-color projection and preserve the authored recipe; choosing
Classic makes those palettes visible again. No client chooses a target profile.

**Settings → Graphics → Characters** remains a local rendering fallback. Authored
rendering is enabled by default; a player whose recipe is Classic still uses the
classic mesh. Existing explicit Classic config choices remain respected. Applying
an authored selection also enables the local authored renderer.

Wire version 18, player schema 2, exact asset/catalog fingerprinting and BGA2
appearance saves keep IDs and pixels consistent. This prerelease deliberately
starts default saves in **world-v20** (and world-v20-fixture); old appearance/save
formats are rejected rather than silently migrated or discarded. Existing world
folders are not modified by using the new default.

This is a bounded builtin kit, not generic runtime glTF loading, uploaded atlases,
clothing, physics hair, or a networked crouch/tool mechanic. First-person self-body
rendering remains unchanged; use the native portrait to inspect your own model.

For a repeatable headless native render (same production GPU pipeline):

    cargo run -- character-preview character-idle.png idle 0.35
    cargo run -- character-preview character-walk.png walk 0.20
    cargo run -- character-preview character-crouch.png crouch 1.0
    cargo run -- character-preview character-left-tool.png tool_use_left 0.3
    cargo run -- character-preview character-right-tool.png tool_use_right 0.3
    cargo run -- character-preview character-curly-bob.png tool_use_right 0.3 4

Each image shows the kit from three angles in the lit world. Crouch and tool-use
are inspectable authored clips here; gameplay does not yet replicate crouch or
hand-action animation states, so the live path uses idle/walk only. Server
movement, collisions, inventory and block interactions are unchanged.

A repeatable two-second, 30 Hz walk sequence can be captured without rebuilding
the world per frame:

    cargo run -- character-motion-preview character-motion
    ffmpeg -framerate 30 -i character-motion/%03d.png -pix_fmt yuv420p character-walk.mp4


## Expanded hair catalog and draw cost

Hair IDs are append-only. The existing IDs are retained:

| ID | Style | Hair triangles | Body + selected hair triangles |
|---:|---|---:|---:|
| 0 | none | 0 | 72 |
| 1 | tousled crop | 120 | 192 |
| 2 | side-swept undercut | 168 | 240 |
| 3 | space buns | 264 | 336 |
| 4 | curly bob | 888 | 960 |
| 5 | curly pigtails | 588 | 660 |
| 6 | sidepart bob | 120 | 192 |
| 7 | compact braid | 144 | 216 |
| 8 | long loose curls | 924 | 996 |
| 9 | long curly ponytail | 660 | 732 |
| 10 | half-up curly cascade | 804 | 876 |
| 11 | rounded afro | 1,416 | 1,488 |
| 12 | twin braids | 1,056 | 1,128 |
| 13 | curly mohawk | 336 | 408 |

The shared kit has 22,680 vertices and 7,560 triangles. Asset validation is bounded
at 32,768 vertices / 98,304 indices, with separate per-style budgets and socket
bounds. All thirteen hair textures retain their native 32 × 32 sRGB pixels.
Instances are grouped after nearest-first admission. Only the body and selected
hair mesh are drawn; catalog expansion does not submit every style per avatar.

`hair_sockets.json` records source hashes, the identity head socket, exact bounds,
and draw ranges. `hair_compatibility.json` and `.md` describe source-backed sampled
clearance evidence and native regressions. The original body, face, rig and all
five clips are unchanged. These are rigid attachments without secondary motion.

The long loose curls and half-up cascade support only limited extra head-look
(±20° yaw / ±5° pitch tested at neutral and held crouch). Twin braids require extra
head-look disabled. The current runtime adds no extra head-look. Original-clip
and sampled idle/walk-blend tests do not imply continuous-motion or arbitrary
clip/layer compatibility. See the compatibility report for precise coverage.

An earlier draft iteration used world-v19. This expanded exact asset catalog is
incompatible with that saved identity, so the current default is world-v20;
older folders remain untouched. Wire 18 and the recipe byte layout are unchanged.

## Character-only performance check

The terrain `perf` benchmark does not include avatars. Use the separate bounded
character pass to compare selected meshes at the same resolution and population:

    cargo run --release -- character-perf 300 128 classic
    cargo run --release -- character-perf 300 128 0
    cargo run --release -- character-perf 300 128 1
    cargo run --release -- character-perf 300 128 4

It renders a deterministic nonoverlapping grid at 1280 × 720, with 30 warmup
frames followed by the requested 1–2000 samples and 1–512 actors. The report
separates setup, CPU joint/instance update and queue writes, total CPU submission,
and GPU pass time where timestamp queries are supported. GPU time includes color
and depth clear; it excludes terrain, post-processing, UI, networking and present.
There is no explicit per-frame GPU wait. Submission time may include driver
backpressure, so it is not end-to-end frame latency. A final pixel checksum keeps
the render observable. Compare hardware, actor count and sample count consistently.

Submitted triangles per actor are 72 without hair, 192 tousled, 240 undercut,
336 space buns, 960 curly bob and 660 curly pigtails. The full catalog is retained
once in the shared mesh; its 7,560 triangles are never all submitted per actor.

See [measured software-GPU samples](PERFORMANCE.md) for the 128-actor comparison,
the 512-actor cap check, and the separate terrain baseline. Dense curly bob has
a meaningful draw cost; the local Classic setting remains available.
