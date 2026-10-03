# Woodland workshop visual benchmark

Run from the checkout root:

```sh
BLOXGLOOM_SUN_SHADOWS=high BLOXGLOOM_TAA=0 cargo run -- workshop-preview /tmp/workshop-previews
```

This is a bounded authored scene, not a performance benchmark or a new gameplay
world. Three cameras capture the identical scene at daylight (clock phase .25)
and dusk (.48), at 1280×800, exposure 1.0 and bloom .12. Bounce is disabled.
The game voxel, character, shadow, indirect scene-AO and HDR/post paths are used.
Existing builtin textures and the existing preview-only sandbox steel/cyan
materials are reused; no downloaded assets or texture-pack changes are included.
The two glowstone practicals and one cyan service marker are real voxel emitters.
The same two idle characters, pose sample .35 s, are retained in all captures.

The command does not load or alter a save. Startup registration is isolated to
preview commands; normal gameplay catalogs are unchanged. Scene generation is
independent of camera and time. The existing outdoor/cave fixtures remain intact.

## Fixed views

| Image prefix | Camera | Look at | Vertical FOV | What it exposes |
|---|---|---|---|---|
| 01-approach | (23,40.5,17) | (2,36.7,-11.5) | 52° | path, layered foreground/trees, eaves, silhouettes |
| 02-doorway | (-6,35.6,3) | (1,35.3,-13) | 57° | shaded threshold, skin, cyan fixture, warm interior |
| 03-workbench | (3.5,35.5,-9) | (0,35,-18.5) | 66° | wood/steel/brick, practical lights, interior character |

Keep adapter, shadow quality, AA, AO and contact-occlusion configuration identical
for comparisons. `capture-settings.txt` records requested settings; stdout records
the actual adapter and character sky/glow samples. The first reference set uses
SwiftShader Vulkan, high shadows, TAA off, default scene AO and contact occlusion.
Software-GPU images are visual evidence only, not hardware performance evidence.

## First reference diagnosis

All six PNGs were visually inspected. The scene reads as a workshop in a wooded
clearing; the path and eaves establish a clear approach, warm lamps distinguish
an interior, and the neon accent remains small. It is intentionally a compact
diorama: the finite background, cubic log roof, oversized voxel-scale furniture
and repeated trees are limitations of this fixture, not discovered renderer bugs.

Prioritized next investigations, **not implemented in this scene change**:

1. **Material response** — in both workbench images, the steel chimney and steel
   bench parts read as dark matte surfaces, distinguished mainly by rivets. Check
   a minimal roughness/specular/metal response on these existing test surfaces
   before a broad texture replacement. Wood also lacks separation beyond albedo.
2. **Shared lighting balance and character response** — compare the doorway pair
   and workbench pair. The interior character's face/clothes lose separation even
   though the wall/bench are legible, and the dusk exterior changes less than the
   sky. Inspect sampled sky/glow energy, directional contribution and material
   response together, preserving the existing dark sealed-cave reference. Do not
   fix this with character-only fill or per-shot exposure. Current character
   samples are threshold sky=12/glow=8 and interior sky=2/glow=6.
3. **Foliage and surface art** — approach images show a chunky crown outline and
   strong repeated leaf texture; doorway/workbench show repeated timber grain.
   Crown shape/leaf distribution and scale-consistent material variation are
   likely higher-leverage than adding more postprocessing. These are content
   improvements; the images alone do not establish a lighting implementation bug.

The fixture does not establish moving TAA quality, hardware cost, normal-map
quality, reflective behavior, or a complete forest biome. Existing motion and
unlit-cave tests remain necessary independent evidence.

## Checks

The adjacent workshop test protects an open doorway, enclosed rear wall, side
window, real metal references, exactly three emissive blocks and camera clearance.
Outdoor regression tests protect existing canopy and dark-cave behavior. Use:

```sh
cargo test preview::outdoor
cargo test
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

## Local-light shadow acceptance

```sh
BLOXGLOOM_SUN_SHADOWS=high BLOXGLOOM_TAA=0 cargo run --release -- local-shadow-preview /tmp/local-shadow-previews
```

This additional view writes eight `off-NN.png` / `on-NN.png` pairs at 960×600.
Every pair uses the same camera, scene, dusk phase .48, actor positions, character
animation time, exposure 1.0 and bloom .12. Only local shadow-map rendering is
disabled for `off`; directional and contact shadows remain unchanged. This is a
visual acceptance sequence, not a performance measurement.

The camera is `(4.5,36.2,-10.5)` aimed at `(-2.3,34.4,-16)`, vertical FOV 58°.
One existing glowstone practical is moved to a one-block timber task-lamp stand
at block `(1,34,-16)`. The kiln area is cleared to expose the existing plaster
wall and timber floor as receivers. The other practical and cyan service marker
remain. An articulated walking character translates alongside the lamp at 8 Hz;
the existing registered GLB fixture stands nearby. Both use the production
avatar caster paths and actual sampled voxel lighting. No texture asset changes,
painted shadows, hidden character fill, or per-shot exposure adjustments are used.

Inspect the character silhouette on the wall behind and left of the actor in
matched off/on pairs, then compare successive on frames for its movement.
The adjacent test protects the real lamp and receiver geometry, a clear caster
to wall path, and identical off/on camera settings. Preserve the sealed-cave and
outdoor references when evaluating changes beyond this local-light view.
