# Bounded local-light shadows

Glowstone and catalog emitters now cast actual six-direction dynamic depth shadows
from terrain, alpha-cutout foliage, dropped geometry, articulated characters and
registered/skinned GLB actors. Terrain and all three actor material paths receive
them per fragment. This is a depth-map system, not a screen-space contact effect.

## Lighting contract

Voxel propagation still owns solid-wall occlusion, source color and attenuation.
The maps only occlude the existing directional fraction; unresolved isotropic
scatter, the cave floor, emission, indirect bounce, sky, sun and AO stay separate.
No additional point-light radiance is added. Matching selected lamps to the
existing dominant voxel transport is a conservative approximation using source
color, emitted level and incoming direction. Exact per-emitter decomposition is
not stored in voxel vertices; mixed or unmatched transport keeps its residual fill.

A source cube is excluded from its own local caster pass without clipping nearby
walls. Other emitters remain casters. Depth comparison sampling works on Vulkan
and GL; GL reserves one unused array layer to avoid implicit cube-array allocation.

## Bounds and controls

Default: two selected sources, 256px per face, 16-block maximum reach, two whole
lights refreshed per rendered frame. Maximum: four sources, 1024px, 32 blocks,
four whole-light updates/frame. Each selected update renders all six faces;
round-robin refresh bounds stale geometry to ceil(count/updates) rendered frames.
Selection uses nearest-source hysteresis and admission/removal fades. Every mesh
carries at most 32 worker-extracted emitter records; revision rejection and chunk
unloading also retire the corresponding sources.

A maximum array is 96 MiB on Vulkan, 100 MiB with GL's extra layer; the default
array is 3 MiB (3.25 MiB on GL). Off allocates only tiny fallback bindings and
schedules no local depth passes. Device texture limits cap the allocation.

Saved config and verified version-2 material packages expose `local_shadows`
with `count`, `resolution`, `range`, and `updates`. A package cannot exceed saved
user budgets or override user count zero. See AUTHORED-VISUALS.md for syntax.
Explicit `BLOXGLOOM_LOCAL_SHADOWS=off` and `BLOXGLOOM_LOCAL_SHADOW_{COUNT,RESOLUTION,RANGE,UPDATES}`
overrides are useful for matched development captures.

## Temporal behavior

Projected shadow motion has no receiver surface velocity. Locally influenced
pixels use the existing reactive-history sign, including currently uncovered
pixels, to avoid TAA trails when a caster moves. TAA remains active outside those
bounded local receiver regions; AO continues using the absolute visibility.
This favors responsive local shadows over temporal accumulation in lamp-lit areas.

## Visual acceptance

`local-shadow-preview <directory>` produces matched off/on workshop frames with a
walking articulated character and registered GLB beside a real glowstone lamp.
Camera, world time, exposure, geometry and poses match between off/on frames.
Software-rendered captures validate appearance and correctness, not hardware speed.
