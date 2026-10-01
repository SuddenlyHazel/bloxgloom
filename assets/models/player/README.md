# Authored player vertical slice

This directory contains a reproducible native representation of the project's
authored character kit. The original game-facing GLBs are kept under `source/`:

- `player.glb`: refined-crouch six-cuboid player, seven-joint rigid skin, with the
  existing styled **cute_glint / soft_smile** face baked into its 512 × 256 atlas
- `hair.glb`: **tousled_crop** rigid head attachment, independent 32 × 32 texture
- `hair_undercut.glb`: **side_swept_undercut**, another short head-local attachment

These are the supplied project assets, not downloaded third-party models. The
body geometry, UVs, five clips, and painted texture pixels are retained. No
generated concept art or browser/Three.js runtime is needed by the game.

## Rebuild

From the repository root:

    python3 tools/character_assets/convert.py
    python3 -m unittest discover -s tools/character_assets -p 'test_*.py'

The converter uses Python's standard library only. It copies embedded PNG bytes
exactly and writes deterministic `character.json`. Unsupported glTF extensions,
mesh transforms, weighted skins, morph targets, and non-linear samplers are
rejected rather than approximated. This is an intentionally narrow importer for
the authored kit, not general glTF support.

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
`hair_undercut.png`. Only the selected hair material is drawn. Keep them independently
sized, nearest-filtered, and sRGB-decoded. Neither uses emission. The character
atlas must not pass through the terrain's 128 × 128 resampler.

The clips are `idle`, `walk`, `crouch`, `tool_use_left`, and `tool_use_right`.
Idle/walk loop; crouch clamps at its held final frame; tool clips finish at their
authored returned-to-rest pose. All are visual-only, with no gameplay root
motion. The crouch clip does not change collision or create a crouch mechanic.

## Native character editor and multiplayer

Open **Pause → Character**. Choose Classic or Authored; authored selections expose
three hair choices (none, tousled crop, side-swept undercut), eight eye styles,
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
starts default saves in **world-v19** (and world-v19-fixture); old appearance/save
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

Each image shows the kit from three angles in the lit world. Crouch and tool-use
are inspectable authored clips here; gameplay does not yet replicate crouch or
hand-action animation states, so the live path uses idle/walk only. Server
movement, collisions, inventory and block interactions are unchanged.

A repeatable two-second, 30 Hz walk sequence can be captured without rebuilding
the world per frame:

    cargo run -- character-motion-preview character-motion
    ffmpeg -framerate 30 -i character-motion/%03d.png -pix_fmt yuv420p character-walk.mp4
