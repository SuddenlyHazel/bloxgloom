# Shader review and beauty proposals

Code-only review of the inline WGSL shading layer, written while the inventory/drops
feature was still in flight (the tree did not compile, so no preview images were
inspected — verify visuals before adopting any of this).

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

- **Magic number `input.layer == 8`** (`pipeline.rs`, glowstone emission) is coupled
  to the ordering of the `SOURCES` array in `material.rs`. Reordering that array
  silently stops glowstone glowing. Worth a shared `const GLOWSTONE_LAYER`.
- **The palette has three sources of truth**: the sun tint `(0.77, 0.66, 0.47)` in
  the voxel shader, the sun/glow colors in the sky shader, and `SKY_COLOR` plus
  `fog_sky`'s horizon `(0.59, 0.72, 0.82)` in `render.rs`/`pipeline.rs`. The horizon
  values match by hand. Any atmosphere work (dawn/dusk, weather) will drift
  immediately — one shared uniform or generated constants would help.
- **`with_world_sun` does textual find-and-replace** on the whole WGSL source
  (`shader.rs`). It works because the sun is a compile-time `const`, but changing the
  sun recompiles pipelines, and the token would corrupt anything else containing
  that string. An `override` or a uniform value is cleaner — and required anyway for
  time-of-day work.
- **Light bleeding across greedy quads**: `emit_quad` interpolates per-corner light
  (`field.corner(...)`) across quads the mesher merged based on a *single* face
  sample. A 16×16 merged wall gets a linear light gradient spanning 16 blocks. The
  `bounce_level` nibble limits merging for bounce light only, not sky/glow
  gradients. Added shading detail (below) runs into the same ceiling.
- **Sky is drawn first with `depth_compare: Always`**, so the procedural noise is
  evaluated on every pixel and then mostly overdrawn by voxels. Drawing voxels first
  and the sky last with `LessEqual` at `z ≈ 1` saves that shading for free.
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
  at grazing angles is where voxel games look mushy; 4–8× aniso is the cheapest
  sharpness win. Note: `request_device(&DeviceDescriptor::default())` won't allow
  it — needs `max_sampler_anisotropy` in the limits.
- **Derivative-based edge AA in the sky**: the sun disc
  (`smoothstep(0.9990, 0.99955, ...)`) and cloud edges are sub-pixel hard and will
  shimmer as the camera turns. Size the smoothstep width with `fwidth()` — two
  instructions, glassy sky.
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
  cloud banks. The `fract(sin(...))` hash also degrades at large coordinates on some
  drivers — an integer-based hash is safer once clouds drift.

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

## Suggested order

1 (all cheap) → 2 (AO + bleed + variation) → 4 (bloom/tonemap — biggest identity
win) → 3 (sky) → 5 (foliage/water as feature work alongside the gameplay features
in flight).

After the inventory/drops work lands, validate against real frames with `preview`,
`ui-preview`, and `lighting-preview` before the team picks any of this up.
