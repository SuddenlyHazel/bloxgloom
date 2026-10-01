# Authored player vertical slice

This directory contains a reproducible native representation of the project's
authored character kit. The original game-facing GLBs are kept under `source/`:

- `player.glb`: refined-crouch six-cuboid player, seven-joint rigid skin, with the
  existing styled **cute_glint / soft_smile** face baked into its 512 × 256 atlas
- `hair.glb`: **tousled_crop** rigid head attachment, independent 32 × 32 texture

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

Material 0 uses `body.png`; material 1 uses `hair.png`. Keep them independently
sized, nearest-filtered, and sRGB-decoded. Neither uses emission. The character
atlas must not pass through the terrain's 128 × 128 resampler.

The clips are `idle`, `walk`, `crouch`, `tool_use_left`, and `tool_use_right`.
Idle/walk loop; crouch clamps at its held final frame; tool clips finish at their
authored returned-to-rest pose. All are visual-only, with no gameplay root
motion. The crouch clip does not change collision or create a crouch mechanic.

This slice deliberately contains one short hairstyle and one baked face, not a
creator UI, all 48 expression combinations, networked head look, clothing, or
hair physics. Additional selections need a versioned appearance contract and
bounded asset catalog before multiplayer use.

## Try it in the native client

Open **Settings → Graphics → Characters** and choose **AUTHORED**. The setting is
local and saved as `authored_characters=true`; **CLASSIC** remains the default.
The real replicated-player draw path uses the authored body, face and short hair,
with movement-derived walk timing and a smooth idle/walk blend. As before, this
first-person game does not draw your own body. Another connected player is needed
to inspect the feature during gameplay. Registered creatures remain unchanged.

The authored base intentionally does not interpret the old skin/shirt/pants
palette bytes as clothing. The server's appearance data and selection protocol
are preserved, and choosing CLASSIC restores that presentation. This is a local
rendering preview of the kit, not synchronized character customization.

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
