# Shader review and beauty proposals

The original proposals below were a code-only review written while the inventory/drops
feature was in flight. A focused follow-up on 2026-09-24 checked the current code and
rendered previews; the decisions are recorded here so the proposals are not mistaken
for adopted work.

## Follow-up decisions (2026-09-24)

| Idea | Decision and reason |
|---|---|
| Shared glowstone layer | **Implemented.** The material mapping and voxel shader constant now use `GLOWSTONE_LAYER` from `material.rs`. |
| Sun shader source injection | **Implemented.** `shader.rs` now prepends a WGSL constant declaration instead of replacing every occurrence of a token in the source. The sun direction remains fixed at pipeline creation. |
| Cloud edge AA | **Implemented.** The existing threshold keeps its width at normal screen sizes and widens with `fwidth(cloud_noise)` where the projected cloud pattern would alias. |
| Anisotropic filtering | **Skipped.** The current sampler magnifies with nearest filtering; wgpu 30 requires linear magnification for anisotropy, which would change the close-up pixel look. The original note about a required `max_sampler_anisotropy` device limit is inaccurate for this wgpu version: support is a downlevel capability and unsupported devices clamp to 1×. |
| Sun disc AA | **Skipped.** Its existing `smoothstep(0.9990, 0.99955, ...)` spans several pixels at the preview resolution. A derivative width would rarely affect the image. |
| Sky/fog dithering | **Skipped.** No convincing banding was visible in the preview, and a raw ±1/255 linear-space dither would make dark caves grainy. This needs an output-space-aware treatment if banding becomes visible. |
| Aerial-perspective warmth; stronger warm/cool grading | **Skipped.** The former needs a camera/view direction in the voxel shader; the latter is an art-direction change needing side-by-side visual tuning. |
| Per-vertex AO; merged-quad light bleed | **Skipped.** `LightField::corner` already darkens corners next to opaque cells, and the greedy mask already includes exact face sky/glow levels. Long quads can still interpolate different corner samples; further splitting needs a representative scene and mesh/CPU/GPU cost comparison. |
| World-space texture variation; baked tile-border AO | **Skipped.** The variation adds per-fragment work and requires visual/performance tuning. Dark tile borders would draw a grid on flat, repeated textures even without geometry edges. |
| Time uniform; richer drifting clouds | **Skipped.** A useful day/night cycle needs coordinated light updates, and extra cloud octaves and shadow samples need GPU profiling. The current cloud coordinates are bounded by the ray calculation, so the large-coordinate hash concern does not apply yet. |
| Bloom, HDR/tonemap, vignette, contrast, MSAA/FXAA | **Skipped.** These require render-target/pass changes and a separate visual and bandwidth budget. |
| Foliage/wind, water, normal/height maps | **Skipped.** These are new surface/engine features, beyond this shader pass. |
| Shared sky/fog palette; sky draw order; radial fog; color-space reauthoring | **Skipped.** These cross the renderer and preview setup or require a broader palette calibration. The existing view-depth fog difference is minor. |

The surface preview and all three lighting previews rendered successfully. Visual
inspection found the sky change subtle, the sealed cave dark, and the glowstone
and bounced-light scenes intact. `cargo test` passed 62 tests; the formatting check
and strict Clippy also passed.

The headless 1280×720 benchmark on an Apple M1 Pro compared the original cloud
threshold with the derivative version, keeping the scene and all other code fixed.
The one-run frame numbers have normal run-to-run noise; the matching mesh sizes and
GPU medians indicate no measurable cost in this sample.

| Lighting mode | Scene setup, old → new | Mesh bytes, old = new | Steady CPU p50, old → new | Steady GPU p50, old → new |
|---|---:|---:|---:|---:|
| Voxel | 872.2 → 868.1 ms | 14,113,008 | 0.176 → 0.163 ms | 0.126 → 0.126 ms |
| Bounced | 1218.8 → 1217.1 ms | 14,260,752 | 0.165 → 0.174 ms | 0.126 → 0.126 ms |

## What the shader layer is today

All shading is inline WGSL in four places, forward-rendered straight to an sRGB
surface with no post chain:

| Shader | File | Role |
|---|---|---|
| Voxel | `src/render/pipeline.rs` (`SHADER`) | Vertex-baked sky/glow/bounce light + sun N·L, trilinear texture array, linear fog |
| Sky | `src/render/sky.rs` (`SKY_SHADER`) | Full-screen gradient + value-noise clouds + sun glow/disc |
| Target outline | `src/render/target.rs` | Flat gold wireframe |
| UI | `src/ui/draw.rs` (`UI_SHADER`) | Font atlas + flat panels |

## Review notes (correctness / robustness)

- **Glowstone layer coupling** was a literal `input.layer == 8` in `pipeline.rs`.
  The shader and `material_layer` now share `GLOWSTONE_LAYER`; the texture source
  array still must keep its layer order aligned with all material mappings.
- **The palette has three sources of truth**: the sun tint `(0.77, 0.66, 0.47)` in
  the voxel shader, the sun/glow colors in the sky shader, and `SKY_COLOR` plus
  `fog_sky`'s horizon `(0.59, 0.72, 0.82)` in `render.rs`/`pipeline.rs`. The horizon
  values match by hand. Any atmosphere work (dawn/dusk, weather) will drift
  immediately — one shared uniform or generated constants would help.
- **`with_world_sun` used textual find-and-replace** on the whole WGSL source.
  It now prepends a constant declaration. Changing the sun still requires a new
  pipeline; a uniform would be needed for time-of-day work.
- **Light bleeding across greedy quads**: `emit_quad` interpolates per-corner light
  (`field.corner(...)`) across quads merged on equal face samples. The merge key
  already contains exact sky/glow face levels and a quantized bounce level, but a
  long quad can still interpolate different corner samples over many blocks.
- **Sky is drawn first with `depth_compare: Always`**, so the procedural noise is
  evaluated on every pixel and then mostly overdrawn by voxels. Drawing voxels first
  and the sky last with a depth test at `z ≈ 1` could save shaded sky pixels, but
  requires coordinated changes to the game and preview render passes.
  (`SKY_COLOR` clear is effectively unreachable — the full-screen triangle covers
  everything — harmless but dead.)
- **`input.distance = output.position.w`** is view depth, not radial distance, so fog
  is subtly direction-dependent. Usually invisible; be aware when tuning fog ranges
  (38/135) against render distance.
- **Mixed color authoring**: texture samples decode sRGB→linear and the target
  encodes back (correct), but hand-authored sky/fog/UI constants are written as raw
  outputs and get encoded again, so they display brighter than their literal values.
  Everything is eye-tuned and coherent today, but decide the color pipeline before
  adding tonemapping or bloom, or the post pass bakes in the inconsistency.

## Proposals for beauty

Ordered roughly by impact per unit of work. Art-direction anchor: the strongest
look for bloxgloom is **deep black caves with small pools of warm glowstone light
and colored bounce**, against a bright airy sky. The lighting data (sky/glow/colored
RGB bounce) is already richer than the shader uses.

### 1. Shader-only polish (small, immediate)

- **Anisotropic filtering** on the material sampler (`pipeline.rs`). Distant ground
  at grazing angles can look mushy. On wgpu 30 this also requires linear
  magnification, changing the current nearest-filtered close-up look.
- **Derivative-based edge AA in the sky**: widen the cloud threshold with
  `fwidth()` where the projected noise is finer than a pixel. The sun disc already
  has a broad smoothstep transition at ordinary resolutions.
- **Hash/blue-noise dither on final output** (`±1/255`): kills 8-bit banding across
  the big flat sky gradient and fog falloff. Two lines, visible quality.
- **Aerial perspective in fog**: nudge `fog_sky` warm toward the sun based on
  `dot(view_dir, sun_dir)`. Sunsets over terrain silhouettes get dramatic for ~5 ALU.
- **Push the warm/cool asymmetry**: lit faces warm, shadowed faces on the cool sky
  term and colored bounce. Half-present already (`0.77,0.66,0.47` vs
  `0.31,0.40,0.53`); slightly more saturation reads as "painted" rather than flat.

### 2. Depth in the block shading (mesher + shader)

- **Per-vertex ambient occlusion** computed during meshing (concave corner test over
  neighboring blocks), packed into the spare byte of `bounce_packed` or its own
  attribute. AO is the biggest single depth win for blocky geometry. Reuse the
  `bounce_level` trick: split greedy merges where AO varies so gradients stay local.
- **Fix the merged-quad light bleed** the same way — quantize sky/glow into the merge
  key at finer granularity, or fall back to flat per-face light when corner samples
  disagree.
- **Break texture repetition** with a low-frequency world-space noise multiplying
  albedo in the fragment shader (pass world position through `VertexOutput`). Every
  grass field is currently the same 128² tile repeated; subtle ±6% value/tint
  variation per ~8 blocks makes terrain read as landscape. Biome tinting
  (green→yellow grass) is the next step on the same mechanism.
- **Baked edge AO in the material tiles**: `material_mips()` already post-processes
  tiles; darkening tile borders a few percent gives seams and depth with no shader
  work.

### 3. Bring the sky to life (`sky.rs`)

- **A time uniform** driving everything below (replaces `with_world_sun`). Caveat
  for the team: true day/night needs world relighting (sky light is baked per
  revision), but a sky-only cycle — moving sun disc, dawn/dusk gradient bands, stars
  at night — can land first and look great on its own.
- **Better clouds**: currently 2 octaves of value noise with a hard threshold and no
  shading. Domain-warped 3–4 octaves, slow drift, and fake self-shadowing (sample
  density toward the sun, darken the far side) turns flat blobs into believable
  cloud banks. The `fract(sin(...))` hash may degrade at large coordinates on some
  drivers if clouds later drift far enough to reach them.

### 4. A post chain (the big unlock)

Render the scene to an offscreen target instead of the surface, then one composite
pass:

- **Bloom** (small dual-filter blur, threshold ~1.0) — glowstone halos in dark caves,
  soft sun glare. Probably the highest-impact item on this whole list for the game's
  identity.
- **HDR scene target (`Rgba16Float`) + tonemap** (ACES or AgX): `albedo * light +
  emission` currently clips at 1.0, and `bounce * 1.35` plus glow plus sun already
  exceeds it near lamps. Proper highlight rolloff is what makes lighting look
  expensive.
- **Vignette + slight contrast curve**, and **MSAA ×4 or FXAA** on block edges (the
  gold target outline and thin sun disc alias badly today).
- Cost caveat: at least one more full-screen pass and extra bandwidth. Per the repo
  guidance, measure with `cargo run --release -- perf 300 6` (and `bounced`) and
  compare scene setup, mesh size, CPU frame time, and GPU frame time separately
  before adopting.

### 5. New surface types (engine work, not just shader)

- **Cutout foliage** (leaves, grass tufts) with `discard`/alpha-to-coverage, and
  **vertex-shader wind sway** driven by the same time uniform and world position.
  Motion is enormous for perceived beauty — a static world reads as dead no matter
  how good the lighting is.
- **Water**: transparent pass with animated UV distortion and a reflection term
  using the sky shader's gradient (share the sky color function between shaders —
  one more reason to centralize the palette). Shoreline foam is a texture layer away.
- **Normal/height data per material layer** (second texture array) for bump on close
  surfaces — axis-aligned faces have trivially derivable tangent bases, so no new
  vertex data is needed. Do this only after 1–4.

## Possible later work

If a representative frame shows light interpolation stretching across large quads,
measure that scene before changing the merge key. If the look calls for glowstone
halos, prototype bloom and tonemapping together and compare GPU frame time and
bandwidth with the current forward path. Surface features can follow their own
gameplay and art requirements.
