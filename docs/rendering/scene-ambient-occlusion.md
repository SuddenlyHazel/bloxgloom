# Scene-wide indirect occlusion

The main HDR geometry pass now writes two attachments: the original linear HDR
scene and a separate indirect-light record. The record contains normalized
indirect RGB and retained local visibility in alpha. Terrain, drops, crossed
foliage, built-in creatures, authored GLB entities, articulated characters, and
LOD terrain use the same contract. Sky writes zero indirect energy.

A full-resolution depth pass reconstructs world positions and geometric normals
and searches eight horizon directions with four radius-distributed samples each.
Normals choose the nearer adjacent depth derivative at silhouettes. Samples
outside the viewport, sky, and the configured world-space radius are rejected.
A small depth/plane-aware bilateral gather removes sampling spokes without
smearing depth boundaries. No AO history is accumulated.

The next pass subtracts `indirect * max(0, retained_visibility - scene_visibility)`
from HDR. It runs before temporal AA, bloom and display mapping. This is not a
final-color darkening multiplier: direct sun (including foliage transmission),
local glow/torch lighting, authored emission, specular highlights, the sealed-cave
floor, and fog in-scattering never enter the indirect attachment. Fog extinction
is applied to exported indirect energy, so distant fog cannot acquire dark AO
silhouettes. Fire and rain alpha-blend after AO; their emission and transparent
backgrounds therefore retain correct compositing.

## Existing local occlusion

The renderer retains the original voxel irradiance average and character joint
AO. It exports their visibility separately rather than applying another AO
multiplier. Voxel corners and trilinear plant samples use a conservative ratio of
weighted mean to maximum for nonzero sky/bounce channels. Uniform attenuated
portal/cave transport remains visibility 1. Local irradiance gradients as well
as solid-neighbor zeros can lower the ratio; this is a bound on already-present
averaging suppression, not an exact geometric occupancy estimate. RGB light
values and the sealed-cave floor are unchanged.

The ratio occupies the unused fractional half of the existing floating-point
material-layer vertex channel. The integer texture identity, vertex stride and
authored material-hook argument remain unchanged. The vertex shader decodes it
before interpolation. Analytic contact shading remains sky-only. When it overlaps
averaged voxel occlusion, the export normalizes the actually retained sky-fill
plus bounce by their conservative common visibility; exported RGB times visibility
exactly reproduces retained indirect energy. Thus an additional occluder cannot
subtract beyond that component or multiply already-dark corners again.

Character baked visibility is exported directly. Authored material hooks continue
to receive their usual lighting; a bounded per-channel hook light ratio transfers
the known built-in indirect share. Explicit emission stays separate. An arbitrary
hook that synthesizes a new lighting model has no independently authored indirect
channel, so that share is necessarily conservative rather than a physical
reconstruction of the hook.

The sign of indirect alpha is reserved for temporal reactivity. Negative alpha
marks untracked deformation/particles; AO uses its magnitude.

## Controls and limits

- `BLOXGLOOM_AO=0..1`: strength, default `0.75`; zero bypasses both AO passes
- `BLOXGLOOM_AO_RADIUS=0.1..5`: world units/blocks, default `1.5`
- Nonfinite values are rejected; finite values are clamped
- GL skips the depth-load AO shaders and preserves normal lighting, without a
  runtime shader failure. The HDR attachment contract remains consistent
- Screen-space occlusion cannot see hidden or offscreen geometry. The world
  light field and analytic grounding provide independent persistent coverage
- Pixel radius is bounded at 96 pixels and AO bypasses below 1.5 pixels, preventing
  unbounded near-camera work. Far LOD geometry naturally contributes little
- Two extra full-resolution targets are used: RGBA16F indirect and R16F
  visibility: 10 bytes/pixel, approximately 19.8 MiB at 1920×1080. They remain
  allocated when AO is disabled or GL falls back, maintaining one HDR attachment
  contract. This implementation is image-tested on software Vulkan; no
  hardware performance claim or benchmark is included

## Reproduction

Use the same executable, backend, sun setting, exposure and camera for both:

```
BLOXGLOOM_AO=0 BLOXGLOOM_TAA=0 BLOXGLOOM_SUN_SHADOWS=high cargo run -- outdoor-depth-preview /tmp/ao-off
BLOXGLOOM_AO=0.75 BLOXGLOOM_TAA=0 BLOXGLOOM_SUN_SHADOWS=high cargo run -- outdoor-depth-preview /tmp/ao-on
```

`render::scene_ao::tests` exercises real GPU shaders, a flat plane and nearby
blocker, zero indirect energy, prior strong occlusion, finite-radius behavior,
alpha preservation, disabled AO, transparent/opaque particle compositing and resize.
A separate GPU test executes production normalization with overlapping baked
corner/contact/bounce terms. An explicit GL-adapter test verifies the shader-free
fallback, unchanged pixels and resize. `lighting::sampling::tests` verifies
uniform portal transport, averaged corners and plant samples independently.
