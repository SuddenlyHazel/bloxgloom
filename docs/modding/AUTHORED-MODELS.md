# Authored models: native GLB foundation

The `feat/authored-model-pipeline` branch adds native Rust GLB loading, bounded
immutable model data, authored animation sampling/blending, appearance controls,
and a wgpu inspection command. The preview reads the GLB directly: no Python
conversion or intermediate mesh format is required.

The revised builtin player is installed in live gameplay and the Character menu.
Its geometry, baked clips and embedded textures are loaded natively. The live
player compiler folds static cubes into thirty actor joints for bounded instancing;
all authored animation targets survive. Public body/hair choices and colors remain
server-authoritative and persist under stable profile IDs. The current export has
one eye design and no mouth variants, so no alternate face selectors appear.

[Packaged GLB creatures](GLB-CREATURES.md) now expose startup registration through
Luau, verified model delivery, named appearance controls and clip playback.
The builtin player still uses its fixed checked-in GLB, recipe v3. The current default world is world-v29.

## Blockbench authoring

Keep `.bbmodel` as the editable source; export a GLB with animations and embedded
PNG textures. Include vertex normals. Give appearance groups, materials and clips
clear, unique names. Use separate mesh groups for independently selectable
geometry: e.g. `eyes_classic`, `eyes_sleepy`, `mouth_smile`, `mouth_open`, `hat`.
Each group may contain children; hiding it hides its complete subtree. Keep
animation bones present even when attached appearance geometry is hidden.

The loader supports ordinary animated node hierarchies as well as skins with up
to four weights per vertex. It does not require our existing humanoid joint names
or a 30-joint skeleton. Exported model dimensions, pivot hierarchy, root motion,
and coordinate basis are preserved; the preview automatically frames the rest
bounds from the glTF front (-Z). Agree on game height and forward direction when
installing the new model. A player model around 1.8 units high with feet at Y=0
matches the existing gameplay envelope.

Animations are named baked translation/rotation/scale tracks. LINEAR, STEP and
CUBICSPLINE interpolation are preserved. Translation/scale blend linearly and
rotations blend with quaternion slerp before hierarchy evaluation. GLB does not
standardize gameplay state transitions, loop intent, sound events, or appearance
options. Blockbench expressions must be baked to keyframes; dynamic expression
evaluation is not implemented here. Loop intent comes from the controls file.

## Appearance controls

A controls JSON maps author-facing options to named nodes/materials. It contains
no geometry or animation data. Referenced names must be unique in the GLB;
missing/ambiguous names, duplicate controls and overlapping target assignments
are rejected before GPU upload.

```json
{
  "variants": [{
    "name": "eyes", "default": "classic",
    "options": [
      {"name": "classic", "nodes": ["eyes_classic"]},
      {"name": "sleepy", "nodes": ["eyes_sleepy"]}
    ]
  }],
  "layers": [{"name": "hat", "nodes": ["hat"], "visible": false}],
  "tints": [{
    "name": "hair", "materials": ["hair_neutral"],
    "color": {"rgb": [190, 121, 64], "mode": "multiply"}
  }],
  "loops": {"idle": true, "walk": true, "attack": false}
}
```

Variants choose one option in a group; layers are independent on/off controls.
Visibility changes rendering only, never animation joints. Use neutral/light
textures for materials whose color should be adjustable. An optional tint
`nodes` list restricts its materials to named node subtrees. This lets skin, iris
and clothes share an atlas while retaining independent colors. Omit `nodes` to
color the whole material. Distinct controls may share materials when their
geometry targets do not overlap. Fixed-color details need separate geometry or
materials to remain outside the tint selection.

Color inputs are 8-bit sRGB and decoded exactly once. `multiply` multiplies
linear sampled texture × glTF baseColorFactor × selected color, preserving
painted shading. `replace` replaces that RGB with the selected color while
retaining material alpha/cutout and lighting. Each preview look can select either
mode. Color alpha is not an appearance control.

## Preview

```sh
cargo run -- model-preview model.glb preview.png
cargo run -- model-preview model.glb preview.png controls.json preview.json
```

The command reports clip names/durations. With no preview settings it uses the
rest pose and default appearance controls. With settings:

```json
{
  "clip": "walk", "seconds": 0.4,
  "look": {
    "variants": {"eyes": "sleepy"},
    "layers": {"hat": true},
    "tints": {"hair": {"rgb": [90, 140, 230], "mode": "multiply"}}
  }
}
```

Optional `blend: {"clip": "idle", "seconds": 0.6, "weight": 0.25}` blends the
first pose toward that clip. One-shots hold their final pose; configured looping
clips wrap. Unknown clip/choice names and invalid time/blend values fail with an
error instead of silently selecting another pose. The output is a 768×768 PNG
with consistent rest-pose framing and simple inspection lighting. World lighting,
first-person body framing, held-item attachments and animation event dispatch
are not exercised by this command.

Vertices, textures and material parameters upload once. Pose/color changes upload
joint matrices and a per-part color palette. Geometry is batched by material;
only appearance visibility changes rebuild the visible index buffers. Hidden
variants contribute no draws. The revised player master uses two material draws
for its default body/hair despite exporting hundreds of cube meshes. Loading and PNG
decoding are ordinary CPU functions and can run on an asset worker when connected
to play; the current headless command has no window thread.

## Current supported subset and bounds

- One embedded GLB buffer; embedded PNG images, UV set 0, static/skinned triangles.
- Opaque and alpha-cutout materials, base color factors, double-sided geometry,
  and clamp/repeat/mirror wrapping. Textures currently use nearest filtering.
- 64 MiB GLB; 1,024 scene nodes/meshes/primitives; 65,536 vertices; 196,608 indices;
  32 materials; 32 images up to 2048×2048, 64 MiB decoded texture budget.
- 64 clips; 120-second clips; 4,096 time keys/channel; 262,144 total stored
  animation values; at most 1,024 skin-palette entries.
- 64 appearance controls, 32 options/group; each controls/preview file at most
  64 KiB. Positive nonsheared node scale and nonsingular sampled poses.

External resources, sparse accessors, required glTF extensions, morph targets,
more than four weights, transparency blending, companion/emissive maps and
material PBR shading are not supported in this initial path. These are rejected
where applicable rather than fetched or silently approximated. Scalar metallic
and roughness properties are not used by the inspection shader.

Verification includes native parser/skin/image tests, stepped/cubic/quaternion
sampling, subtree/choice validation, and GPU image regressions showing authored
pose changes, eye selection, hat visibility, and both color modes. The checked-in
[fixture](../../fixtures/authored-model/README.md) makes those checks repeatable.

## Revised player master

The supplied [revised Blockbench master](../../assets/models/player/master/README.md)
is checked in unchanged, with mapped body/hair choices, atlas-scoped color
controls and preview settings. Its seven gameplay clips are sampled directly
from the export. Eye/mouth variants will come from a later authored export.
