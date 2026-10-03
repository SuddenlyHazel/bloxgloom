# Shared environment and material lighting

This pass connects the procedural atmosphere to surface illumination without
changing exposure. The clear-noon diffuse calibration is retained. Low-angle sun
light becomes warm and fades below the horizon; diffuse hemispheres follow the
weather-adjusted horizon and zenith. Terrain, articulated characters, authored
actors and distant terrain consume the same camera lighting basis. No actor-only
fill is used, and zero sky visibility still excludes all exterior illumination.

## Material response

The workshop's original steel had only a dark albedo texture: no specular
companion meant its direct specular function returned zero. The existing procedural
steel now includes normal and oldPBR specular companions. Unpainted panel/rivet
reflectance is calibrated as conductor F0; dark rough joints remain dielectric.
Metals reduce their diffuse contribution instead of adding reflection on top of a
full diffuse lobe. Materials without companions retain their legacy response.

The environment lobe samples an analytic horizon/zenith field, blurred with
roughness, and uses existing sky visibility and local occlusion. Indoors it can
reflect the existing voxel glow/bounce estimate. This is not a cubemap or a local
reflection probe: it cannot reflect room geometry, windows or individual lamps.
It also inherits the voxel sky field's spatial precision. These limits matter
most on smooth metal and at partially sheltered thresholds.

## Controls

Local configuration accepts `lighting_sun_intensity`,
`lighting_ambient_intensity`, and `lighting_environment_intensity`, each finite
0–4, default 1. They scale lighting rather than camera exposure. Existing files
retain defaults. Invalid/nonfinite local values use safe defaults.

Version-2 custom material descriptors may select one global `environment_lighting`
object with `sun_intensity`, `ambient_intensity`, and `environment_intensity`.
Each omitted field defaults to 1; authored out-of-range/nonfinite/unknown fields
are rejected. At most one descriptor in a verified package set may select the
scene profile; duplicate selections fail instead of relying on load order.
This profile is startup-immutable and is not a dynamic material parameter.
Package and user scales multiply and the result is clamped to 0–4.

## Reproducible comparison

Use the same six views, camera, geometry, animation time, exposure, shadows and AA
as `woodland-workshop.md`. The steel companion/reflectance correction is the sole
content difference in the light/material pair. Do not present these software GPU
captures as performance measurements. Outdoor dark-cave captures and shader
regressions separately check the zero-sky boundary.

Validation results and visual assessment will be recorded after production
captures and final checks, rather than inferred from compilation.

## Stage 1/2 visual checkpoint (7ddab93)

All six 1280×800 production captures were inspected at unchanged exposure1,
bloom0.12, high sun shadows, TAA off, default AO, contact1 and SwiftShader Vulkan.
The final custom-normal safety-fix recapture was directly inspected and remains
visually consistent with the first set (at most one RGB code-value difference). Paths are `woodland-workshop-light-material-v2-final` in the shared
capture directory; matching image names identify the original six baseline views.

- The steel chimney, workbench panels and service fixture now have readable
  metallic separation, with cooler exposed faces and warm interior response.
- Dusk changes illumination with the sky while clear-noon diffuse stays calibrated.
- The rear character still lacks separation in the dim workshop. This is not
  resolved by the atmosphere/material work and motivates the separate occluded,
  source-colored local-light step. No compensating character fill was added.
- Repeated grain/leaf art and the chunky, nearly repeated crown shapes remain.
  The compact scene's finite background and block-scale furniture also remain.

This checkpoint establishes the scoped material/atmosphere improvement, not the
complete visual target, local geometric reflections, or hardware performance.

## Source-colored local transport

Local light now carries a dominant emitter color and incoming-direction confidence
through the same opaque-aware, bounded scalar voxel field. Descending light-level
buckets visit each lit cell once; ties use a deterministic color ordering. The
incoming direction points toward brighter reachable neighbors, rather than
through a wall toward a hidden emitter. Opposing paths cancel toward isotropy.
Emitter tint currently uses normalized catalog reflectance (black falls back to
neutral white), an explicit interim convention rather than a separate emission
spectrum API. Modded cyan/red/etc. emitters therefore work without scene IDs.

Terrain, block and sprite drops, player/procedural characters and authored GLBs
share source RGB, squared scalar falloff and local directional response. Direct
local light remains outside sun-shadow and screen-space indirect-AO subtraction.
Bounced emission also uses source color. Unlit sealed rooms gain no source energy.

The default mixture is 65% directional and 35% unresolved scattered local light
at full directional confidence, applied equally to every surface. This avoids
pretending that approximate around-corner voxel transport is purely punctual.
`lighting_local_directionality` (local config) and `local_directionality` inside
the mod profile are finite 0–4 multipliers, default1; the final directional
coefficient is `clamp(0.65 * user * package, 0, 1)`. Zero restores isotropic colored
local light, while 1.539 or above gives fully directional response. Exposure is
unaffected and no character-specific lighting gain exists.

Retained transport metadata costs 6 bytes per voxel only for lit halos (648 KiB
for 48³), plus a transient u32 index bucket payload up to432 KiB. Published light
samples grow6 bytes (24 KiB per16³ chunk). Terrain/drop vertices grow52→60 bytes;
actor packing adds no vertex-attribute locations and preserves the16-attribute
player limit. Lit terrain faces stay cell-sized to retain chroma/direction gradients.
These are explicit size/work bounds, not hardware performance measurements.

Far-terrain LOD snapshots still carry scalar glow only. Their previous warm,
isotropic scalar-squared approximation is preserved; source RGB and directional
transport apply to the near terrain/material path and moving objects. Extending
that distant span schema is outside this checkpoint, so far LOD is not evidence
of source-color parity.

## Final scoped acceptance

The final vegetation pass is preview content only: deterministic asymmetric
multi-lobe crowns and clustered groundcover reuse existing textures. Building,
lamps, character poses, camera and exposure are unchanged; world generation and
save data are untouched. All six final images were independently inspected twice.
Compared with the lighting-only set, the approach has more varied silhouettes and
clearer open/vegetated patches. Grain/leaf repetition and voxel-scale furniture
remain. The rear figure remains dim: no claim that these changes fully resolve
interior character readability or the complete artistic target.

Separate retained evidence sets distinguish the changes:

1. `woodland-workshop-v1`: original scene
2. `woodland-workshop-light-material-v2-final`: shared atmosphere and materials
3. `woodland-workshop-local-light-v3-final`: source-colored local illumination
4. `woodland-workshop-vegetation-v4`: content variation

Final workspace tests: 1,724 application +52 host-API passed, zero failed,
11 ignored; doc tests passed. Formatting, clippy (all targets/features with
warnings denied), and diff checks passed. The 8 outdoor tests were rerun after
the test-only iteration cleanup. The ignored GPU daylight regression was run
explicitly and passed, covering18 shared lighting cases. Focused actual GPU tests
also cover all three actor material paths, custom materials and terrain shadows.
Independent read-only review found no blocking issue. Graphify query/update were
attempted but unavailable because the executable is absent.

No hardware benchmark, push, merge or rebase was performed. This change series
starts at6dcc195 on the isolated lighting branch. Parallel changes on main must
be reconciled explicitly later; these commits do not claim compatibility with
that unseen newer integration state.

## Local shadow controls

The saved client config accepts a JSON value on one line:

```ini
local_shadows={"count":2,"resolution":256,"range":16,"updates":2}
```

`count=0` disables dynamic point-light maps. Limits are four lights, 1024 texels
per face, 32 blocks, and four whole-light updates per frame. Range cannot be
lower than two blocks; enabled map resolution cannot be below 64. Saved values
are sanitized before use. Disabling preserves the saved resolution.

A version-2 material's optional `local_shadows` object uses the same fields;
it is verified strictly and capped by the local client settings. Only one
material in a bundle may select it. This affects scene-wide local occlusion,
not just that material's target textures. The existing `local_directionality`
controls response strength separately.

For diagnostics, `BLOXGLOOM_LOCAL_SHADOWS=off` disables the maps, and
`BLOXGLOOM_LOCAL_SHADOW_COUNT`, `BLOXGLOOM_LOCAL_SHADOW_RESOLUTION`,
`BLOXGLOOM_LOCAL_SHADOW_RANGE`, and `BLOXGLOOM_LOCAL_SHADOW_UPDATES` override
individual values within the same hard resource limits.
