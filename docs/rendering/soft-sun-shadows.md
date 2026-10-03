# Distance-dependent soft sun shadows

Medium and High now estimate blocker/receiver separation and widen the sun's
penumbra as that gap grows. A wall touching the floor keeps a tight contact edge;
a canopy several blocks above it casts a softer edge. This is a bounded PCSS
approximation for the existing camera-local orthographic shadow map, not a new
ambient-darkening term. Low retains its single bilinear depth comparison, and
Off/night still skip shadow work.

## Controls

The saved `sun_shadow_quality` option still selects the map resolution/range.
These optional startup environment overrides control the new filtering:

- `BLOXGLOOM_SUN_SOFTNESS=1`: default apparent angular size. Values are clamped
  to 0–4. Zero skips blocker search and restores the previous fixed nine-tap
  tent filter exactly, making matched A/B captures straightforward. The default
  uses `tan(half_angle) = 0.02` (about a 1.15-degree angular radius); this is a
  restrained art setting rather than the astronomical sun's angular size.
- `BLOXGLOOM_SUN_PENUMBRA_TEXELS=6`: default maximum filter radius in shadow-map
  texels. Values are clamped to 2–12. The cap remains in texels to bound sparse
  filter artifacts and prevent very remote blockers from smearing contact.

Malformed/non-finite overrides use defaults. The effect has no time-varying
noise, rotation, frame hash, or screen-space dither. Both search and filtering
use the snapped light-space map and the geometric receiver-plane derivative,
not shading normals. The 24-world-unit maximum search distance and fixed
iteration/sample counts bound the work independently of scene complexity.

## Portable blocker search and filtering

The current Naga GL backend cannot load a depth texture or sample one with a
non-comparison sampler. Instead of adding a second map or disabling soft shadows
on GL, nine deterministic light-space search taps estimate occluder distance
using comparison samples. Interior diagonal taps reduce missed thin/rotated
blockers between the center and outer axial taps. Each covered tap runs six binary depth probes over
24 world units, resolving blocker distance to a 0.375-unit interval. Fractional
bilinear coverage weights the average; its median-depth search threshold adapts
to that coverage so cutout edge samples do not falsely collapse to zero gap.

The estimated gap times angular radius yields world-space penumbra radius,
which is converted to shadow-map texels. The original minimum anti-aliasing
footprint is combined in quadrature and the radius is clamped. Sixteen fixed,
symmetric disk taps perform the final comparison filter. Receiver-plane
correction applies to both the blocker search and final filter, preserving
contact and avoiding sloped-surface self-shadow bands.

The maximum is 79 comparison samples per shaded Medium/High receiver: nine
initial probes, up to fifty-four binary probes, and sixteen filter taps. Uncovered
search taps skip binary probes. Disabled/faded/out-of-map receivers exit before
search; the zero-softness reference uses nine taps and Low uses one. This is a
quality/cost tradeoff; no performance benchmark was run for this change.

This approximation does not reconstruct multi-layer blockers, add cascades, or
make a sparse disk filter exact for large radii. Distant blockers and wide
penumbrae are deliberately capped. The angular size is shared by every caster:
terrain, cutouts, articulated players, and registered models retain their
existing geometry/material participation.

## Light and binding invariants

Only the existing direct-sun visibility scalar changes. Voxel skylight remains
authoritative for caves; indirect sky fill, bounce, torch/emission, and the
sealed-cave floor are unchanged. Range/edge and grazing-sun fades are retained.

One 16-byte vector is appended after the existing scene-contact array in the
shared shadow uniform. Existing projection/contact offsets and all texture and
sampler bindings stay unchanged. Fallback and test uniforms include the tail.
No additional texture, render pass, feature flag, or readback is required.

## Reproducible checks

Run the focused production GPU checks with a serial test runner:

    cargo test render::sun_shadow -- --test-threads=1

The suite covers coplanar sloped receivers through moving sun angles at every
quality, wall/floor contact, edited caster removal, opaque/transparent cutouts,
night, sealed cave and torch isolation, deterministic repeated output, and
near/far aligned blockers. The near/far fixture holds the projected footprint,
camera, material, sun, and receiver constant so only blocker separation changes.

To save tiny matched High-quality fixture images while running the test:

    BLOXGLOOM_SHADOW_CAPTURE_DIRECTORY=soft-shadow-check \
      cargo test gpu_soft_sun_widens -- --test-threads=1 --nocapture

This writes `contact.png`, `before-fixed.png`, and `after-soft.png`. The last two
use the same detached caster. For whole-scene comparison use the production
outdoor preview at the same quality and with all other effect settings fixed:

    BLOXGLOOM_SUN_SHADOWS=high BLOXGLOOM_SUN_SOFTNESS=0 \
      cargo run -- outdoor-preview soft-sun-before
    BLOXGLOOM_SUN_SHADOWS=high BLOXGLOOM_SUN_SOFTNESS=1 \
      cargo run -- outdoor-preview soft-sun-after

### Scoped validation checkpoint

All 12 sun-shadow checks passed on llvmpipe/OpenGL and SwiftShader/Vulkan, with
five fixture images visually inspected. At High quality the detached block's
partially shadowed edge grew from 204 pixels with fixed filtering to 645 on GL
(644 on Vulkan), while the near-contact edge stayed at 197 (198 on Vulkan).
All four separate actor shadow checks passed on Vulkan. On GL, three passed and
the first-person color-framing check returned black for both compared color
buffers; that separate actor readback check is not counted as a pass. The shared
contact shader validation passed. All-target/all-feature Clippy with warnings
denied and scoped formatting passed on the isolated patch. The integrated
renderer and production outdoor A/B still require the aggregate verification.
