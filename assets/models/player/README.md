# Articulated characters

A new [native GLB import and preview foundation](../../../docs/modding/AUTHORED-MODELS.md)
preserves authored clips and named appearance controls for future player/creature
models. It can inspect a GLB directly with `model-preview`; the in-game builtin
family described below remains installed during this preparation stage. The supplied
[revised master](master/README.md) now has native previews of its authored gameplay
clips, body/hair variants and independent color controls.

The articulated family is the game's only player renderer. There is no Classic/Authored switch or legacy fallback. A palette-only player snapshot resolves to the default articulated recipe.

## Appearance

Pause → Character previews the same native WGPU renderer used in play. Apply remains a server-authoritative, acknowledged operation; closing discards an unapplied draft.

- Two bodies: **Flat chest** and **Defined chest + sports bra**
- Thirteen hairstyles plus no hair, with the existing stable hairstyle IDs
- Arbitrary 8-bit RGB hair color (normalized sRGB at the shader boundary)
- Eight eye choices, six mouths and optional iris RGB
- Existing registered skin palettes; pants palettes tint the shorts and shirt palettes tint the sports bra. Flat chest is intentionally bare and has no upper garment to recolor
- Default: flat chest, tousled crop, hair `#BE7940`, original articulated 3D eyes, soft smile, original iris color

Eye ID 0 uses the base's modeled white/iris/pupil/glint/brow geometry. Other eye IDs adapt the existing pixel features onto the articulated eye surfaces; source eye details are hidden only for those styles. Mouth pixels use the head's new dimensions rather than the old atlas rectangle.

## Geometry and movement

Sources are in `articulated/source/`. The two larger body GLBs use deterministic
lossless `.glb.gz` containers; `gunzip -c flat_chest.glb.gz > flat_chest.glb`
restores the original bytes for Blender or other glTF tools. The converter reads
these containers directly. Manifest source hashes refer to the original GLB bytes. The two bodies share the exact 30-node skin hierarchy, including pelvis, spine, chest, neck/head, clavicles, upper arms, forearms, wrists/hands, grips, thighs, shins, feet, toes, eyes/pupils and brows. Source models are already 1.8 metres high. The native basis rotates −Z-forward to +Z-forward without the old 0.9 scale.

Flat chest is 2,244 triangles; the defined body is 3,664. Only the selected body and selected hair are submitted for each actor. Admission remains nearest-first and capped at 512; grouping by body/hair does not admit distant actors around that cap. Static assets upload once. Only instance data and 30 joint matrices per actor change each frame.

Gameplay motion is authored for this hierarchy in `src/render/avatars/character_asset/gameplay.rs`:

- Breathing idle, walking, speed-blended running, and smoothly blended crouch
- Bent knees/ankles and articulated toes; separate clavicle, elbow and wrist motion
- Mirrored left/right 0.8-second tool one-shots layered over locomotion, returning to rest rather than looping
- Bounded head look: ±20° yaw and ±5° pitch; no unvalidated hair-handle simulation
- First-person framing applies one rigid delta to every joint in each complete arm subtree, including the grip, instead of disconnecting elbows/wrists
- `grip_R` and `grip_L` supply native attachment transforms. The game currently has no held-item mesh renderer; the anchors are ready for that renderer rather than inventing a separate tool model

The original four source validation clips remain unchanged and are separately sampled. They are not mislabeled as gameplay walk/mining clips. Body collision, eye height, movement authority and world edits remain server-owned.

## Color contract

Hair neutral textures use sRGB sampling. The player's RGB is decoded once, multiplied with the already-linear sampled neutral shade, and encoded by the render target once. Alpha stays unchanged. Fixed-color ties are separate primitive roles and sample their original texture; overlapping source UVs cannot cause ties to be recolored.

Exact black necessarily removes the painted shade contrast; a lifted charcoal preserves it. Color selection does not alter skin, eye or clothing materials. Body and face images remain independent texture layers.

## Rebuild and verify

The installed player renderer uses the offline format below; the native GLB preview
loader is separate. Clients cannot receive asset paths or arbitrary geometry over the wire. The offline converter validates embedded GLBs, rigid weights, hierarchy and mesh limits, bakes hair adjustment handles at neutral, and writes bounded BGC2 mesh files. Its standard-library-only rebuild is deterministic:

```sh
python tools/character_assets/convert.py
python -m unittest discover -s tools/character_assets -v
cargo test
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

Additional native-pose collision validation requires NumPy and SciPy. It tests actual converted vertex bytes with matrices exported by the Rust gameplay code, including source clips, run/crouch/tool blends and head-look limits:

```sh
BLOXGLOOM_POSE_DUMP=/tmp/articulated-poses.json cargo test native_articulated_pose_dump
python tools/character_assets/check_articulated_clearance.py /tmp/articulated-poses.json /tmp/articulated-clearance.json
```

This is a sampled convex SAT check, not a continuous collision guarantee. Nonconvex body pieces use conservative outer hulls. Scalp contact is intentional; ears, eyes/brows, body and the frontal face region are protected. Re-run validation after changing movement, the head envelope, mesh geometry or hair handles.

Native render commands:

```sh
cargo run -- character-preview character.png idle 0.35 8
cargo run -- character-preview crouch.png crouch 0.6 12
cargo run -- character-preview tool.png tool_use_right 0.24 9
cargo run -- character-style-preview styles.png
cargo run -- character-motion-preview motion-frames
cargo run -- third-person-preview third-person-frames
cargo run --release -- character-perf 300 128 8
cargo run --release -- perf 300 6
```

The standard character preview shows both bodies, from front/three-quarter/back, on the game's terrain with its lighting and HUD. The style preview also demonstrates independently colored hair and face choices.

## Save and wire boundary

Per repository prerelease policy, this change starts new default worlds at **world-v22**. It does not convert or delete old worlds. Explicitly opening an incompatible catalog fails with a diagnostic before profile creation; old directories and file bytes stay untouched.

- Protocol 21; player payload schema 3
- Appearance recipe v2: body, hair, eyes, mouth, optional iris RGB and hair RGB
- Public appearance payload maximum: 16 bytes; palette-only 4-byte payload still means the new default
- Profile store `BGA3`; old `BGA2` files are explicitly rejected, never reset or rewritten
- Full catalog identity hashes the new assets, stable choice names, default recipe and presentation version

New clients and servers must match. Real nonblocking-listener tests cover both bodies, hair color, independent peers and restart persistence; rejection tests verify old profile/catalog bytes are preserved.
