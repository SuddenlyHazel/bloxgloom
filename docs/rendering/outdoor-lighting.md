# Outdoor lighting acceptance

The outdoor path uses content-authored sky absorption, spatial voxel samples,
explicit thin-material shading, and shared terrain/actor sun shadows. It does
not select an art genre or replace the sandbox material system.

## What changes

- `sky_attenuation` is a bounded 0–15 per-voxel absorption value, independent of
  collision and cutout flags. Builtin leaves absorb two levels; plants and air
  absorb none. Opaque cells still stop skylight. Absorption applies above the
  local halo and to lateral propagation, and authored values reach LOD summaries.
- Crossed plants sample the existing light field per vertex. Their upward
  diffuse normals remain intact. Explicit foliage wrap/transmission controls
  opt materials into thin-surface shading; geometric card normals affect only
  transmission. Transmission remains sun-shadowed and cannot illuminate caves.
- [Distance-dependent soft sun shadows](soft-sun-shadows.md) estimate blocker
  separation using portable depth comparisons. Nearby contacts remain tight,
  elevated casters produce broader penumbrae; softness zero restores tent PCF.
- Dynamic [scene contacts](scene-contact.md) replace final-color decals with
  bounded floor-validated indirect-only player and registered-creature grounding. Direct light and emission
  are preserved; overlapping footprints do not stack. This is distinct from
  full-scene cavity AO.
- [Scene-wide depth-horizon AO](scene-ambient-occlusion.md) exports diffuse indirect light separately, then
  unions screen-space visibility with existing local suppression before TAA.
  Direct sunlight, torches, emission, specular and fog scattering remain outside
  the occluded term. Its geometric coverage is limited to visible depth.
- Opt-in temporal AA is described in [temporal-aa.md](temporal-aa.md). It runs
  before bloom/display mapping and safely falls back on GL. Supported actors
  now export previous submitted-pose coordinates; unsupported procedural motion
  is explicitly reactive. AA remains opt-in during visual acceptance.

Native and Luau content knobs, validation, and wire transport are described in
[registered content](../modding/REGISTERED-CONTENT.md). Content fingerprints
changed, so default scratch worlds are now `world-v27` and `world-v27-fixture`.
Existing folders are not deleted or migrated. Explicit old world paths continue
rejecting incompatible schemas rather than reinterpreting them.

## Reproducible visual checks

Use the same executable, adapter, shadow quality, camera, and exposure for A/B:

    BLOXGLOOM_SUN_SHADOWS=high cargo run --release -- outdoor-preview outdoor-off
    BLOXGLOOM_SUN_SHADOWS=high BLOXGLOOM_TAA=1 cargo run --release -- outdoor-preview outdoor-taa
    BLOXGLOOM_SUN_SHADOWS=high cargo run --release -- outdoor-motion-preview motion-off
    BLOXGLOOM_SUN_SHADOWS=high BLOXGLOOM_TAA=1 cargo run --release -- outdoor-motion-preview motion-taa
    BLOXGLOOM_SUN_SHADOWS=high cargo run --release -- sandbox-preview regression factory noon hero idle
    BLOXGLOOM_SUN_SHADOWS=high cargo run --release -- sandbox-preview regression neon night hero idle

The outdoor fixture has four views of one authored scene: open grass, layered
canopy with characters, cave entrance, and deep interior. No auto exposure,
per-shot fill, or hidden light sources are used. The scene records its settings
and prints actor light samples and adapter identity.

With matched SwiftShader/Vulkan captures, open actors retained sky=15 while the
canopy actors changed from 15 to 9 and 10. Entrance actors retained sky=9, and the
deep-interior capture was pixel-identical. Software rendering is visual evidence,
not representative hardware performance evidence.

## Tone and atmosphere calibration

Keep exposure at 1.0 while assessing these changes. The existing neutral
highlight shoulder and 0.12 bloom strength are retained: the canopy improvement
comes from light visibility, not globally crushing shadows. Bloom extraction
already excludes ordinary daylight and preserves emissive industrial/neon
accents. The existing weather scattering uses air exposure separately from
surface shading; changing leaf absorption must not carve dark silhouettes into
fog. No global color-grade, haze, or extra cavity-darkening multiplier is added.

## Prior checkpoint validation

The implementation checkpoint passed 1,688 tests with zero failures and ten
intentionally ignored tests on serial SwiftShader/Vulkan. The default parallel
GL run exposed unsupported temporal depth access and several existing
backend/readback and script-time-budget failures; the temporal GL path now
explicitly remains single-sample. After the contact and crossed-plant changes, the rendering suite passed 160
tests with zero failures and four intentionally ignored tests. Clippy passed
with all targets/features and warnings denied. Focused material tests exercise
backlit crossed cards, winding parity, cave isolation, and bounded energy.

Hardware checks should cover moving characters/foliage, rapid pans, newly
exposed surfaces, first-person views, resize, and day/night changes before
turning temporal AA on by default. That checkpoint preceded the scene AO and per-object motion pass; neither
software capture set establishes representative hardware performance.


The deterministic motion command retains history across 28 frames: stationary
warmup, walking/translating characters, synthetic cutout-foliage movement,
camera pan, first-person cut, viewport resize, and return cut. It is a visual
stress fixture, not a runtime wind feature or a benchmark. Review the matched
AA-off/AA-on frames, including interior surfaces as well as sky silhouettes.


## Larger pipeline acceptance

`outdoor-depth-preview <directory>` adds a fixed stone courtyard with close
corners, steps, an inset emissive reference and a registered GLB actor. The four
standard outdoor scenes remain unchanged. `outdoor-creature-preview` and
`outdoor-creature-motion-preview` extend those views with independently tinted
GLB validation assets, not final creature artwork.

Use a copied executable for exact-toggle comparisons:

    python3 tools/capture_rendering_review.py /path/to/bloxgloom review --motion --sandbox

The runner records the executable SHA-256, adapter environment, per-case settings,
exit status and raw capture logs. It compares AO off/on in the courtyard, fixed
versus distance-softened outdoor shadows, optional AA-off/on GLB motion, and
optional factory/noon and neon/night scenes. It does not run performance tests.

Scene AO is configured with `BLOXGLOOM_AO` (0–1, default 0.75) and
`BLOXGLOOM_AO_RADIUS` (0.1–5 world blocks, default 1.5). GL retains the lighting
split but skips the unsupported depth-reading AO/TAA passes. Soft shadows work
on GL using comparison-only blocker search. Existing voxel/corner transport is
preserved; a conservative local mean/max irradiance ratio prevents extra screen
AO from compounding that suppression. This ratio is not exact geometric
visibility, and hidden/offscreen geometry cannot contribute to screen AO.

The neutral display mapping and physically separated air exposure are retained.
The new depth comes from local light visibility and caster distance, not a
scene-wide dark overlay or a genre-specific color grade. Hardware frame cost and
hardware motion acceptance remain with the user; no benchmark is claimed.

Final code, test counts, matched-capture observations and remaining limitations
are recorded in [pipeline acceptance](pipeline-acceptance.md).
