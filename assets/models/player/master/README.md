# Revised Blockbench character master

`model.glb` is the unchanged `Bloxgloom-Character-Master-Revised.glb` supplied by
Hazel. It is a native GLB asset for the preparation/inspection pipeline; the
installed in-game player remains the articulated family until its replacement
is integrated. Keep the editable `.bbmodel` alongside your authoring files.

SHA-256: `0bbf30cd94428cd55622145ad23226b54202f2eeb7cdabdd87bc0bf94ffe6aaa`.

## Export and structure

Blockbench 5.2.1; binary GLB, export scale 16, embedded textures and exported
animations, groups exported as ordinary nodes (armature conversion off).
The body is 1.8 units tall with feet at Y=0 and glTF -Z forward.

- 794 named nodes, 708 cube primitives, 23,368 vertices, 34,344 indices.
- 17 embedded PNG images/materials; named textures supply control targets when
  Blockbench omits material names. All material names are resolved uniquely.
- Two chest variants and thirteen hair styles plus no hair. Every choice stays
  in the file; controls hide unselected subtrees without altering animations.
- One modeled eye design, separate iris geometry, no mouth-choice groups.
  Eye and mouth variants are expected in a later authored export.

## Authored clips

| Clip | Duration | Playback |
| --- | --- | --- |
| idle | 3 s | Loop |
| walk | 1.2 s | Loop |
| run | 0.72 s | Loop |
| crouch | 0.3 s | One-shot, hold end |
| tool_use_left / tool_use_right | 0.8 s | One-shot, hold end |
| hair_fit_head_turn | 3 s | Loop in inspection |
| crouch_test / grip_test / weight_shift / wrist_ankle_test | 3 s | Validation, one-shot |

The GLB supplies these tracks directly; no procedural gameplay clips replace
them. Game transitions, first-person framing and returning tool motion to idle
remain integration behavior rather than properties of this preview.

## Controls and previews

`controls.json` defines `body`, `hair_style`, and colors for hair, skin, shorts,
shirt/sports bra and iris. Colors support `multiply` and `replace`. The shared
body atlas is scoped by mesh names so coloring clothing does not color skin or
eyes. Hair accessory materials keep their painted colors. Default white body
multipliers preserve the original texture; replacement can recolor dark shorts
without multiplying black pixels.

From the repository root:

```sh
cargo run -- model-preview assets/models/player/master/model.glb /tmp/master-idle.png assets/models/player/master/controls.json assets/models/player/master/idle.json
```

Replace `idle.json` with `walk.json`, `run.json`, `crouch.json`, `alternate.json`
or `colors.json`. The last two demonstrate body/hair selection and independent
iris/clothing colors. Each render reports visible material draw batches: the
default body and tousled hair require two draws, rather than one per cube.

See [the native model guide](../../../../docs/modding/AUTHORED-MODELS.md) for the
JSON schema, interpolation support and loader bounds. Tests use this actual
export to protect animation sampling, scale, control mappings and batching.
