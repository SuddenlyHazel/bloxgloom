# Sandbox rendering fixtures

These are optional, deterministic test scenes for a genre-neutral creator platform. They are not a game, a progression system, a world generator, or the default art direction.

- **Workshop:** timber-framed making space, warm windows, workbenches and stacked lumber
- **Factory:** brick machine hall, structural metal bays, boiler stacks, overhead services and inlaid tracks
- **Neon:** modular industrial hangar with cyan and magenta emissive panels

All three contain the same six articulated characters, black/blond/pastel hair and light/dark skin samples, stone/sand/wood/gravel/moss/leaves swatches, a cutout-leaf tree, and an open-front covered recess. These make geometry, material, character, sky-occlusion and emissive-light behavior inspectable together.

## Capture

```sh
cargo run --release -- sandbox-preview output/sandbox
# One fast development capture:
cargo run --release -- sandbox-preview output/sandbox workshop noon hero idle
# Matched close character crops for all three themes and times:
cargo run --release -- sandbox-preview output/sandbox all all characters idle
# A fixed walk-pose sample, not a different camera or random animation instant:
cargo run --release -- sandbox-preview output/sandbox factory sunset characters walk
```

Arguments are positional: output directory, scene (`all|workshop|factory|neon`), time (`all|noon|sunset|night`), camera (`hero|characters`), and pose (`idle|walk`). Defaults produce nine hero images at a fixed idle pose. Invalid selectors fail before creating output.

Output names are `sandbox-{scene}-{time}-{camera}-{pose}-{seconds}s.png`. The decimal point in the time suffix is written as `p` (for example, `0p35s`). A settings text file accompanies each camera/pose batch. Use separate directories for renderer revisions or quality variants so before/after captures cannot overwrite one another.

## What is held constant

- 1280 × 800, real WGPU production voxel and articulated-character pipelines
- Production HDR, tone mapping and bloom; exposure 1.0 and bloom 0.12
- Fixed camera coordinates/FOV, six decoded appearance recipes, actor coordinates and 0.35-second animation sample
- Fixed block geometry replacing every chunk, independent of procedural generation
- Actual voxel light samples at the gameplay character sample height; bounced light disabled
- No UI, extra character fill, automatic exposure or per-shot brightening

Set `BLOXGLOOM_PREVIEW_SECONDS=0.70` to capture a different matched animation instant (finite seconds in 0–60; default 0.35). Set `BLOXGLOOM_SUN_SHADOWS=off|low|medium|high` to compare quality modes; use separate output directories for each mode.

The noon/sunset/night images change only the world clock. Noon is cycle phase 0.25, sunset is 0.46 (a low sun still above the horizon, suitable for long shadow comparisons), and night is 0.75. One riveted steel panel and two colored emissive panels are minimal ordinary startup-registered texture/block declarations, available only to the `sandbox-preview` command. They use the existing material emission and voxel-light systems and never add identities to a normal world catalog. Colored surface radiance does not promise colored direct voxel-light propagation.

## Visual checks

1. Compare a theme at noon, sunset and night without changing camera or pose
2. Check faces, hair and dark clothing remain legible where the actual light permits it
3. Inspect tree, beam, building and moving-character shadows together; look for floating feet, acne, detached shadows and alpha-cutout silhouettes
4. Compare the open yard, covered recess and emissive machine surfaces; the recess must not inherit outdoor brightness
5. Inspect character crops for material/skin color consistency before accepting a lighting adjustment

The preview prints the actual WGPU adapter. A CPU software-adapter capture establishes correctness and appearance only; it is not evidence of desktop GPU performance. Use the existing release `perf 300 6` benchmark (and `bounced` when relevant) for separate CPU/GPU timing, with matching adapter and settings on both revisions.

These PNGs are rendered by the actual engine. No Blender substitutes or fake gameplay screenshots are used.

### Software GLES capture compatibility

The fixture catalog reserves one unused texture layer when its material count would be a multiple of six. WGPU 30's GLES backend otherwise allocates square 2D texture arrays as cube arrays before the explicitly requested 2D-array view, corrupting sampled voxel tiles. This fixture-local padding registers no block or item and does not change the production material allocator. The OpenGL software path must be visually inspected; a successful PNG write alone is not a correctness check.
