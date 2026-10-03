# Temporal AA (opt-in)

Set `BLOXGLOOM_TAA=1` for the live client, `preview` / `outdoor-preview`, or
`perf`. Omit it for single-sample rendering. Image previews accumulate one
eight-sample Halton cycle at each fixed camera; use the motion fixture below
for animation acceptance. UI and target outlines are drawn after AA.

The resolve runs in linear HDR after scene ambient occlusion and particles,
before authored post effects, bloom, and display mapping. It uses stable-output
camera/object reprojection, neighborhood color clipping, a color-change weight,
and reduced history weight under motion. History depth is R32Float linear depth.
All four bilinear history taps must agree with the previous surface's expected
depth. Out-of-view, behind-camera, reactive, and sky samples reject history.
Resizing, FOV changes, sharp turns, and teleports reset or reject history.

## Object motion contract

A separate depth-equal actor pass writes RGBA16Float motion after the scene. It
reuses the production vertex transforms, alpha cutouts, selected model parts, and
first-person clipping. It never writes depth. RG stores signed **pixel** velocity,
B previous linear depth, and A distinguishes camera-only static geometry (0),
valid actor motion (+1), and explicit rejection (-1).

Supported production geometry includes:

- Characters: previous complete joint palettes, actor translation/yaw, walking,
  crouching, head look, tools, and the camera-framed first-person rig
- Packaged GLB creatures: previous skinned joint palettes, node animation,
  actor translation/yaw/scale, selected layers and variants
- Procedural creatures and rigid moving objects: previous foot motion, squash,
  bob, ear sway, and projectile orientation

Previous poses are keyed by stable entity ID, not instance-buffer order. They
advance only after a submitted scene, together with camera history and jitter.
Repeated preparation, shadow passes, or a failed surface acquisition cannot
invent a previous frame. New IDs, disappeared/re-admitted actors, model or
appearance changes, first-person mode changes, and actor teleports reject history.
The stored camera projection is unjittered. Both current and previous clips are
interpolated from the same current triangle before computing motion; this avoids
mistaking rasterizer subpixel vertex snapping for actor movement.

There is deliberately no invented velocity for arbitrary package `bg_vertex`
deformation, CPU-animated world drops, fire/rain, or the fixture's moving foliage.
Those surfaces write negative indirect-buffer alpha and reject history at their
current pixels. The AO pass reads absolute alpha as existing local visibility.
Particles run after AO so transparent quads cannot erase the underlying scene's
indirect energy. Unsupported moving surfaces therefore retain single-frame edge
quality; exposed surfaces still use depth rejection and current-color clipping.

## Backends and cost

The pinned wgpu/Naga GLSL backend cannot read non-comparison depth textures.
On GL, the opt-in prints a diagnostic and leaves TAA disabled. Vulkan, Metal, and
DX12 use the depth-reprojected path. The tests exercise safe GL fallback separately.

TAA's full-resolution allocation is 32 bytes per pixel: two HDR color histories,
two R32Float depth histories, and one RGBA16Float motion texture, about 63.3 MiB
at 1920×1080. These textures and the HDR resolve/copy exist only when enabled.
Actor motion pipelines and bounded GPU pose buffers are prepared with actor
renderers (roughly 1 MiB for character poses, 32 KiB for primitive poses, plus the
bounded maximum GLB palette). CPU previous-pose capture and cloning are disabled
when TAA is off. The motion pass adds a depth-tested redraw of admitted actors.
This sprint did not run performance benchmarks; representative hardware timing
remains a separate user-owned step.

## Regression checks

`cargo test render::post` includes real GPU checks for camera history, one-pixel
motion, first-frame rejection, changed depth, offscreen reprojection, and color
clipping. Object-motion cases prove valid vectors override incorrect camera-only
reprojection and that new objects, wrong previous depth, and offscreen velocities
reject history.

`cargo test render::avatars::motion` renders the production character, packaged
GLB, and procedural-creature pipelines. It checks signed translation, actual
joint deformation, zero stationary velocity through all eight jitter samples,
packed-order stability, first-person resets, and disable/re-enable invalidation.
CPU tests protect stable-ID lookup, skipped frames, visibility churn, teleports,
nonfinite poses, appearance changes, and camera-history invalidation. Existing
actor GPU tests cover alpha, selected parts, hair, first-person and shadow parity.

## Bounded visual acceptance

`outdoor-creature-motion-preview` installs a preview-only registered GLB asset
and retains history across 28 deterministic frames. Frames 0–7 warm up; 8–19
combine a camera pan, translating/skinned characters and GLB creatures, and
explicitly reactive deformed foliage. Frames 20–23 exercise first-person clipping;
23 resizes from 640×400 to 800×500; 24 cuts back to the canopy. Animation uses
explicit simulation steps, independent of software-GPU wall time.

Use one copied executable and identical adapter/shadow/AO settings:

```sh
BLOXGLOOM_SUN_SHADOWS=high BLOXGLOOM_AO=0 BLOXGLOOM_TAA=0 ./bloxgloom outdoor-creature-motion-preview motion-off
BLOXGLOOM_SUN_SHADOWS=high BLOXGLOOM_AO=0 BLOXGLOOM_TAA=1 ./bloxgloom outdoor-creature-motion-preview motion-taa
python3 tools/check_temporal_motion.py motion-off motion-taa
```

Early matched SwiftShader Vulkan captures on 2026-10-03 showed clean actor
silhouettes/overlaps at frame 12. The fixture-specific check found 8,940 newly
exposed sky pixels outside the one-pixel current-edge neighborhood and zero
lingering dark pixels. This narrow check does not establish same-depth interior
texture quality, arbitrary authored animation, or hardware frame cost. TAA
remains opt-in pending representative gameplay/hardware review.
