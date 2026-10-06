# PBR material processing issues

The October 5, 2026 audit compared 350 imported JG RTX materials, their source pixels, Bloxgloom's material pipeline, and the BSL reference in `external/shaders`. It found import and filtering defects as well as rendering capability gaps. Predicted visual effects below require gameplay verification.

## Current goal status — October 6, 2026

The separate BSL reference mode targets the checked-in default. The enhanced renderer remains the normal game mode. The goal remains active; implementation, visual acceptance and matched-runtime parity are separate checks.

| Area | Current implementation and remaining acceptance |
| --- | --- |
| World composition | Generator 9 supplies regional terrain, coasts, drainage, tree species/shapes, vegetation patches and continuous cold-region snow retention. Server-authored forest proxies retain narrow stems, rounded cutout crowns and shared seam decisions; root continuation is capped at 3 m. Compact summaries preserve source-thickness snow caps separately from buried material, with all 77 skyline tiles fitting unchanged admission limits. Verified meadow/coast/grove/mountain captures were inspected; floating forest cuboids and the long ocean frontier wall are gone. Coarse terrain steps and distant ocean tonal bands remain visible limitations. |
| Texture artwork | The earlier 91 height-map changes alter only height channels. Later authorized art work also changes three bark albedos, 55 botanical smoothness channels and three icons, and appends nine bark-only wood icons. These later changes are not height-only. Source pack files remain untouched. |
| Foliage | Oblique leaf clusters, rooted runtime wind and species-aware bounded thin-sheet optics are implemented and covered by focused geometry/GPU checks. Verified natural captures retain distinct pink/green crowns and connected trunks. Increased geometry cost is measured separately from frame timing; subjective live appearance remains for the user to judge. |
| Global illumination | Opt-in scene path tracing supports multiple surface/medium bounces and air/cloud/water scattering. Complete 512 m origin transport measures roughly 9.66 seconds per frame at half resolution on M1 Pro; noisy coast captures still fail visual acceptance. Earlier near-only timings exclude this broader workload. Raw first-water moments and independent optical filters are under validation, with unchanged default rendering. |
| Off-screen reflections | Current scene transport includes loaded opaque/cutout/water interfaces, coverage-selected distant geometry, exact current actor poses and animated pickups. Near authoritative coverage suppresses overlapping coarse triangles. Unknown space stays conservative; coarse water intervals describe admitted presentation, not unseen fine voxels. Actual page traversal, pose/material and volume/physics checks accompany the implementation; final integrated capture/performance acceptance is recorded below. |
| Sky and clouds | Enhanced 3D volumetric clouds and source-based sky are implemented. Committed SH9 diffuse sky convolution now preserves solar azimuth; seven-climate/54-normal GPU quadrature and actual terrain/actor/LOD receiver checks pass, including zero-skylight shelter and retained GI energy. Reference mode separately implements the default source cloud, fog, bloom/display, shadow, water, flare and shaft equations. |
| BSL styling/parity | Default and explicitly non-default advanced reference modes are separate. Source equations have independent CPU/GPU checks, including artistic metal treatment. Exact whole-frame parity is **not established**: no matched Minecraft/Iris scene/frame is available, and runtime filtering, lightmap, projection and material classifications differ. See [BSL_REFERENCE_AUDIT.md](BSL_REFERENCE_AUDIT.md). |

Forest/SH integration and its documentation are committed (`5a26918`, `84d5c4d`). The verified release generated four unedited 512 m natural captures in `/private/tmp/bloxgloom-final-verified-world/`: meadow, coast, cherry grove and mountains. Independent image inspection and focused forest, lighting, wire, source-water/LOD draw and shader checks support the current implementation; they do not establish matched-runtime BSL parity or live gameplay acceptance.

The preceding committed checkpoint's integrated `cargo test --quiet -- --test-threads=4` passed **2,041 tests, zero failures, 19 ignored** in 423.38 seconds. Formatting, all-target/all-feature Clippy with warnings denied, and whitespace checks passed. The earlier 2,006-pass/13-failure snapshot is historical; its shader syntax, stale geometry/fingerprint assertions and lighting-dependent studio fixtures are corrected. Final 300-frame default/bounced/512 m horizon measurements are recorded below. Full GI remains opt-in; its timings must not be confused with the default renderer.

Remaining goal work is substantive: stabilize noisy complete-water transport, make scene transport affordable and establish matched-runtime BSL image acceptance. The current upgrade extends targets to admitted distant terrain/forest, real water interfaces, actors and drops. Static near geometry, bounded LOD pages and medium coverage publish atomically; current GPU actor deformation/refit has separate admission and never rebuilds the terrain BVH. Complete target rejection or pending native uploads selects the matching voxel/SSR frame. Default rendering continues to use screen-space reflections and environment fallback. Coverage is finite, bounded by successfully admitted world and presentation targets, and does not represent unloaded geometry. New performance and visual acceptance must accompany this broader tracer.

The final reference landscape exposed an actual second-frame resource hazard: the opaque LOD bind group retained the original depth texture used by water SSR while the opaque pass wrote that same attachment. Commit `ea662cd` separates opaque and water groups. An actual draw regression, run in both normal and reference modes, checks two successive submissions plus resize, opaque/private water depths and refreshed reflection data. The rebuilt reference release successfully generated the 512 m cherry-grove comparison at `/private/tmp/bloxgloom-final-reference-world/03-cherry-grove.png`; pipeline-creation checks alone had missed the alias.

## Work and approval sequence

1. Complete and committed: corrected normal orientation, source material encoding, and categorical material filtering, with automated checks and performance comparisons.
2. The user completed the live test and approved continuing on October 5, 2026, reporting a modest improvement but unsatisfactory overall appearance.
3. Complete: corrected roughness conversion and albedo color-space handling, reimported assets, and validated source pixels and runtime filtering. The user reported unsatisfactory cherry bark appearance after stage 2; the follow-up investigation is recorded below. Broader visual acceptance remains pending.

The user subsequently authorized a coordinated renderer upgrade, including height reconstruction, parallax self-shadowing, scene reflections, atmospheric scattering, and image treatment. Its implementation and validation are recorded below. The earlier two-stage limits describe the historical work, not the current authorized scope.

## Normal orientation

The original importer flipped every Java normal map's green channel. Among comparable non-flat Java and Bedrock maps, 118 pairs had identical XY data, nine had reversed green, and 27 differed otherwise. Most Java maps were already DirectX normals, the convention our derivative UV frame uses. A universal flip therefore made their lighting disagree with surface relief. A universal removal would also miss the source pack's exceptions.

Use canonical Bedrock normal maps resolved through the pack's texture sets and edition name mapping where available. Preserve the authored Java AO and height channels. Decode remaining Java maps as LabPBR DirectX data, recording exceptions and fallbacks. Resize data channels independently; alpha represents height or emission, not image transparency.

During the fix, the shader's UV frame also proved invalid when a crossed plant's upward shading normal lies inside its vertical card plane. Projecting the axes collapses a basis vector to zero. Stage 1 now preserves that intentional upward normal and skips height tracing for unsupported frames, with GPU regression coverage for crossed cards and degenerate geometry.

References: `tools/jg_rtx/import.py`, `src/render/material/relief.wgsl`, upstream `src/scripts/labpbr/normal.ts`, and the [LabPBR normal standard](https://shaderlabs.org/wiki/LabPBR_Material_Standard#Normal_Texture_(_n)).

## Source material encoding

239 of the 338 Java imports had green equal to zero everywhere, including dirt and stone. Ordinary iron, copper, gold, and nether gold ores had no pixels classified as metal, unlike their deepslate equivalents. The runtime correctly interpreted these bytes as dielectric reflectance, but the source exports did not consistently represent the intended materials. Large dielectric values also reduced diffuse color on ore specks.

Resolve original MER or MERS inputs using texture-set semantics, including scalar declarations, PNG/TGA variants, and Java/Bedrock aliases. Encode dielectric reflectance and declared metal identities using the pack's curated rules. Stage 1 preserved red-channel smoothness; stage 2 now regenerates it from canonical perceptual roughness. Keep emission's 255 sentinel distinct from real emission, and keep porosity separate from subsurface scattering. Record exact source inputs and fallback decisions in import provenance.

References: upstream `src/scripts/labpbr/textureSet.ts`, `materials.ts`, and `specular.ts`; `src/render/material/pbr.wgsl`.

## Material mip filtering

The original worker averaged metal IDs and dielectric reflectance bytes together; the shader interpolated those encoded values again. In deepslate iron ore, the metal fraction fell from 16.4% to zero at mip level four, while average dielectric reflectance increased from 3.9% to 19.4%. Averaging the blue channel can likewise convert porosity into scattering or vice versa.

Categorical green and blue channels must be selected from an unfiltered base-level texel before decoding. Continuous smoothness and emission can retain their mip chain. BSL also samples its LabPBR specular data at level zero. This prevents invented material identities; distant categorical patterns remain point sampled rather than integrated over the entire pixel footprint. A future decoded material representation could provide filtered metal coverage at additional memory cost.

References: `src/render/material/companions.rs`, `companions.wgsl`, BSL `lib/surface/materialGbuffers.glsl`.

## Roughness conversion — corrected in stage 2

All 12 Bedrock imports used `1 - sqrt(roughness)` instead of the pack converter's `1 - roughness`. The shader then squared perceptual roughness for GGX, applying the curve twice. Amethyst cluster's mean roughness changed from approximately 0.213 to 0.452, predicting broader, duller highlights.

Stage 2 encodes red as `255 - MER.blue` for all 347 materials with canonical sources, including scalar declarations. This supplies perceptual roughness to the shader, which applies the GGX square once. The audit found the legacy curve in Java exports too: 85 imported smoothness maps changed, rather than only the 12 Bedrock-selected materials. Three materials retain recorded authored Java fallbacks because no canonical MER is available. Amethyst cluster's imported mean perceptual roughness now matches its source at 0.213; pale oak log is 0.802 rather than 0.895. Actual imported amethyst, pale oak, and deepslate iron pixels have regression coverage. The shader's existing minimum roughness remains unchanged.

References: `tools/jg_rtx/import.py`, upstream `src/scripts/labpbr/specular.ts`, `src/render/material/pbr.wgsl`.

## Albedo color space — corrected in stage 2

Albedo mipmaps previously averaged sRGB bytes instead of linear-light color. Equal opaque black and white became 127 instead of approximately 188 in sRGB, predicting excessive darkening of high-contrast textures with distance. This predates the import.

Stage 2 decodes sRGB before runtime mip averaging and edge blending, weights mip colors by linear alpha coverage, and encodes the result back to sRGB. Alpha stays linear; fully transparent input colors contribute no light. Shared transfer tables avoid per-texel exponentiation. GPU albedo storage remains sRGB, while normal/material storage remains linear UNORM.

Importer albedo resizing and grass/sunflower source-over compositing now operate in linear light with alpha weighting. PBR data resizing stays independent of alpha and receives no sRGB transfer. Six base albedo files changed: grass side, jungle leaves, sunflower top, blackstone, blackstone top, and gilded blackstone. Stage 1 metal/reflectance, porosity/scattering, and emission bytes remain identical in every material map. Normal bytes remain identical except the sunflower composite, whose resampled head alpha mask changed. Authored fixed tints and the icon projection retain their existing artistic rules; icons were regenerated from the corrected albedos.

References: `src/render/material/color.rs`, `src/render/material.rs`, `tools/jg_rtx/color.py`, `src/render/pipeline.rs`.

## Rendering capabilities identified before this upgrade

At the initial audit, only 18 of the 350 imported normal maps contained variable height. Most materials therefore had no parallax relief even with parallax enabled. This was primarily absent source height data, rather than discarded imported height. Bedrock conversions without authored height also used flat AO and height. The coordinated upgrade below adds confidence-gated height reconstruction and restores two declared maps.

Bloxgloom previously offset texture coordinates without height self-shadowing. Its specular environment used an analytic sky and approximate local transport without reflecting scene geometry. Both capabilities are implemented in the coordinated upgrade below. BSL has parallax self-shadowing and screen-space scene reflections when advanced materials are enabled. Its checked-in default disables advanced materials, so those features must not be assumed active in every BSL screenshot.

References: `src/render/material/parallax.wgsl`, `companions.wgsl`, BSL `lib/surface/parallax.glsl`, `program/deferred1.glsl`, and `lib/settings.glsl`.

## Stage 1 validation and live test

Stage 1 corrects canonical normal XY for 344 materials and material encoding for 347. Six normal and three material fallbacks are recorded in `assets/jg-rtx/provenance.json`; the source pack supplies no canonical input for those layers. A pixel audit confirmed all 350 albedos and red smoothness channels remain unchanged. Existing Java AO and height are retained, using independent channel resizing. The worker keeps green/blue mip bytes categorical and the shader selects them from the unfiltered base level, with filtered red smoothness and alpha emission. Texture, block, item, and state identities are unchanged.

Stage 1 changed the material catalog fingerprint and advanced the default save directory to `world-v30/` (`world-v30-fixture/` for lifecycle fixtures). Stage 2 changes it again and uses `world-v31/` (`world-v31-fixture/`). This follows the repository's prerelease policy; existing local worlds are not migrated or deleted. Block, item, texture, and state identities remain unchanged.

The full `cargo test --quiet -- --test-threads=4` run passed 1,898 tests, with zero failures and 14 ignored. After adding the final crossed-card normal-frame guard, all 15 focused material tests passed again, including production GPU readbacks. `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `git diff --check` passed. Five Python conversion regressions passed. A complete source-pixel audit verified all canonical normal/material maps, including a separate reconstruction check for the composited sunflower.

The stage 1 release `block-preview bloxgloom:iron_ore` image was inspected for functioning terrain, vegetation, and ore rendering. This was a rendering smoke check. The user subsequently tested stage 1, reported modest improvement, and approved stage 2 without accepting the overall visual quality.

## Stage 1 performance measurements

Before and after measurements use Apple M1 Pro/Metal, seed `0xB10C6100`, radius 6, 1280 by 720 output, 300 steady frames, medium sun shadows, and temporal AA off. Commands: `cargo run --release -- perf 300 6` and `cargo run --release -- perf 300 6 bounced`. Scene setup is excluded from frame timings; CPU measures submit-side work and GPU measures render passes. Presentation and live gameplay are excluded.

| Measurement | Voxel before | Voxel after | Bounced before | Bounced after |
| --- | ---: | ---: | ---: | ---: |
| Scene setup milliseconds | 3700.9 | 3652.7 | 4186.2 | 4035.2 |
| Visible triangles | 92,028 | 92,028 | 96,340 | 96,340 |
| Mesh bytes | 22,891,800 | 22,891,800 | 23,842,728 | 23,842,728 |
| CPU steady p50 milliseconds | 2.483 | 2.334 | 2.564 | 2.780 |
| CPU steady p95 milliseconds | 4.152 | 4.616 | 7.155 | 5.225 |
| GPU steady p50 milliseconds | 3.799 | 3.924 | 3.836 | 3.993 |
| GPU steady p95 milliseconds | 4.571 | 6.229 | 6.945 | 5.702 |

Median GPU time increased approximately 3.3 percent with voxel lighting and 4.1 percent with bounced lighting, consistent with the additional base-level categorical lookup. Mesh size and material-array memory are unchanged. Single-run CPU timings and frame tails vary with machine activity, so these measurements do not establish a stable CPU regression or improvement. An earlier after-change voxel run had CPU p50/p95 3.321/6.035 milliseconds and GPU p50/p95 3.946/7.533 milliseconds; the table records its repeat after other validation work finished.

## Stage 2 validation and limits

The complete production-code test run (`cargo test --offline --quiet -- --test-threads=4`) passed 1,901 tests, with zero failures and 14 ignored. Two later additions passed separately: a catalog-to-mip integration regression and a source-pixel roughness regression. All four focused color tests passed. They cover linear-light averages, alpha weighting, fully transparent colors, seam blending, constant colors across all 256 byte values and repeated mip levels, and the actual catalog mip path. Eight Python conversion regressions passed. The release build, formatting check, clippy with all targets/features and warnings denied, and diff whitespace check passed.

An independent source-pixel audit verified canonical roughness in 346 ordinary layers and reconstructed the composited sunflower's full albedo/normal/material output separately, covering all 347 canonical material layers. Comparison against the stage 1 commit confirmed unchanged material green/blue/alpha bytes in all 350 layers. A before/after albedo contact sheet was inspected to check the regenerated source artwork; this does not verify 3D lighting or gameplay appearance.

The first stage 2 session exposed no suitable Metal graphics adapter. In that session, the release `perf 300 6` command failed before rendering, so GPU timings and a rendered preview could not be obtained. GPU-dependent tests could not provide fresh rendering evidence under those restrictions. The stage 1 performance table above remains historical. A later unrestricted session restored the Metal adapter; fresh results are recorded in the cherry investigation below. The filter adds shared lookup tables totaling roughly 65 KiB and does not enlarge the GPU material arrays.

The first stage 2 session left changes in the working tree because `.git` was read-only and `git add` could not create `.git/index.lock`. Repository writes were restored in the follow-up cherry investigation. The unrelated pre-existing `.gitignore` edit was left alone.

## Live test after stage 2

Launch the release client using the new default `world-v31/`. Inspect amethyst highlights and wood/stone under changing sunlight; ordinary and deepslate ores; foliage and sunflower cutout edges; high-contrast texture brightness at increasing distance; and a sealed cave with and without a local light. Most materials still lack authored height, and the renderer still lacks parallax self-shadowing and scene reflections. These remaining capability gaps need separate work if the corrected processing still falls short of the desired appearance.

## Cherry bark follow-up investigation

The user confirmed that `block-preview 'bloxgloom:cherry_log[axis=y]'` reproduces the bark appearance seen in gameplay. That fixture places a relatively small block in a procedural lawn near a shadow-casting tree, so it cannot isolate material quality from scene lighting.

A new `material-preview <state-key> <directory>` command uses the existing production headless render path with a close camera, flat stone floor, no tree occluders, no HUD, and fixed exposure 1.0. It writes morning/noon variants with albedo alone, normal/AO/height detail without specular, and full PBR. The comparison changes only companion-presence flags; gameplay and custom material pipelines retain full materials. The sample is a real registered block state lit and meshed through the ordinary voxel path. This is an inspection tool, not a replacement gameplay scene.

The cherry bark, regular log top, stripped log side, and plank albedos match byte for byte between Java and Bedrock. The stripped end-grain albedo differs slightly between editions. All five normal XY maps match exactly; the inspected bark uses matching art and normal data. Bark has F0 byte 10, no metal classification, and a flat height alpha of 255. Its dense gray/white/red mottling remains in the albedo-only capture. Normal/AO detail changes its appearance less than adding full specular in the inspected noon view. These observations do not establish that the imported source art is a good stylistic choice; no cherry artwork was replaced during this investigation.

The comparison exposed another renderer defect: `bg_environment_radiance` returned blue horizon radiance for downward reflection rays, treating the ground hemisphere as sky. Both the reflected direction and the broad roughness approximation could therefore import an inappropriate cool sheen into bark and stone. The sky fallback now fades smoothly to zero below the horizon. Existing local transport remains separate; this fix does not synthesize ground reflections or implement scene reflections. A production-WGSL GPU regression covers downward rays, the horizon transition, upward sky color, and environment intensity while retaining the existing palette, cave, AO, and local-light checks.

Matched noon captures before/after the sky correction were inspected. The blue/gray wash is reduced, while the source bark mottling remains. The albedo-only control is byte-for-byte identical before and after, isolating this change to reflected lighting. The new material preview and the original gameplay-scene reproduction remain separate references; neither constitutes user acceptance of the artwork.

Follow-up validation includes all 26 material tests with the Metal adapter, all 20 existing preview tests, and both daylight calibration checks with the normally ignored GPU test explicitly enabled. Formatting, all-target/all-feature clippy with warnings denied, the release build, and all eight Python conversion regressions pass. The final full suite passed 1,903 tests, with zero failures and 14 ignored; the ignored daylight GPU regression was also run explicitly and passed. Sealed-cave, lamp, bounced-lamp, and pond captures were inspected after the shared sky change: the cave remains dark, local illumination remains warm, and the water surface still renders its sky reflection.

The original gameplay-scene cherry preview was regenerated after the sky correction as an additional comparison to the user-confirmed reproduction. The remaining mottling belongs to the source albedo, but these captures do not establish that the artwork is the cause of the unsatisfactory overall result. An albedo-only preview is not a reference for the appearance of the complete shader/material system. The subsequent renderer upgrade preserves the artwork and addresses lighting, display mapping, relief, reflections, and filtering together.


### Follow-up performance measurements

Commands and capture conditions match the stage 1 benchmark above (Apple M1 Pro/Metal, radius 6, 300 steady frames, 1280×720, medium sun shadows, temporal AA off). Both “before” runs already include the stage 2 roughness/color corrections and precede only the sky-hemisphere fix. Before captures overlapped targeted preview validation; after captures were taken after the full suite finished, so CPU and frame-tail differences are not an isolated shader-cost comparison.

| Measurement | Voxel before sky fix | Voxel after sky fix | Bounced before sky fix | Bounced after sky fix |
| --- | ---: | ---: | ---: | ---: |
| Scene setup milliseconds | 3993.9 | 3653.1 | 3963.9 | 4063.6 |
| Visible triangles | 92,028 | 92,028 | 96,340 | 96,340 |
| Mesh bytes | 22,891,800 | 22,891,800 | 23,842,728 | 23,842,728 |
| CPU steady p50 milliseconds | 2.888 | 2.469 | 2.567 | 2.376 |
| CPU steady p95 milliseconds | 6.742 | 7.759 | 7.119 | 4.990 |
| GPU steady p50 milliseconds | 3.951 | 3.964 | 3.962 | 3.985 |
| GPU steady p95 milliseconds | 5.450 | 7.888 | 6.130 | 8.913 |

Median GPU differences are approximately 0.3 percent and 0.6 percent, within single-run variability. An initial voxel after run measured CPU p50/p95 3.165/7.884 ms and GPU p50/p95 4.355/10.143 ms; the repeat in the table resolved the unexpectedly high median but does not establish stable frame-tail behavior. Mesh size, GPU material-array size, and scene identities are unchanged. These measurements exclude presentation and live gameplay.


## Coordinated renderer upgrade — October 5, 2026

The accepted goal is to improve the existing JG RTX pack through the production renderer, produce matched previews and measured performance, document the remaining approximation limits, and commit a build for a live play test. Three focused subagents implemented reflections, material relief/filtering, and atmosphere/water; the main agent integrated lighting, display mapping, previews, and validation.

### Implemented changes

- **Shaded material readability:** the old tone-map toe subtracted almost all low radiance (`0.02` became `0.0025` before display transfer). The new hue-preserving shoulder retains proportional dark detail, black, and HDR highlight gradation. Shared sun radiance is scaled by 2.4 and the broad sky fill is modestly recalibrated; exposure remains explicit, with no automatic cave brightening. Terrain, characters, foliage, water, and atmosphere share this basis.
- **Environment and scene reflections:** deterministic GGX sky convolution and an integrated BRDF approximation replace the three-direction roughness blend. Half-resolution SSR resolves actual visible opaque geometry for block and water receivers. Opaque lit radiance is captured after AO and before water; its mip chain carries depth-masked geometry coverage so background sky cannot illuminate a rough geometry hit. Misses keep the existing sky/local transport fallback. Water receivers store their own linear distance rather than imprecise half-float device depth. Screen/range confidence and plane/normal-aware reconstruction limit edge artifacts. Reflections resolve before foreground fire/rain, then atmosphere and temporal AA.
- **Stable material detail:** normal mips average vector directions and transfer unresolved normal variance into roughness. Source AO/height and categorical material channels retain their independent contracts. No new material GPU arrays were added. Temporal AA is now enabled by default on supported backends; explicit `BLOXGLOOM_TAA=0` disables it. Reactive water, rain, and fire intentionally do not accumulate history.
- **Surface relief:** eight-sample parallax self-shadowing affects direct sunlight only. Flat and alpha-cutout maps skip relief marching. The importer restores two declared source height maps and infers height for 89 structural materials using a bounded, confidence-gated periodic Poisson solve. Four inferred candidates were rejected. There are now 109/350 variable-height maps: 20 source/authored and 89 inferred. Provenance distinguishes inferred data from authored height. Albedo, normal RGB/AO, specular, icons, and content identities are unchanged; 91 normal alpha maps changed. Default parallax depth is 0.125, retaining the 0.15 safety cap.
- **Atmosphere and water:** half-resolution, 12-step shadowed single scattering uses a stable height profile and depth-aware upsampling. It starts beyond three metres, stops at water receiver surfaces, and contributes nothing for blocked/unknown shadow volumes, disabled maps, or night. Near and distant water share sun shadows and GGX highlights, correcting glints under bridges/canopies. Rebuilt sun/local shadow bindings refresh water and LOD consumers.

### Reproducible inspection

`showcase-preview <directory>` captures the same cherry grove, masonry shelter, planks, ore samples, and stream in noon and early-morning lighting, with fixed bark/overview/stream cameras. `showcase-motion-preview <directory>` checks moving actors, camera motion, disocclusion, and a first-person cut. `material-preview` remains the separate unobstructed companion-map comparison. All use production GPU paths and modify no world save.

The baseline executable is preserved at `/private/tmp/bloxgloom-wow/bloxgloom-before`; matched baseline/final captures are under `/private/tmp/bloxgloom-wow/before` and `/private/tmp/bloxgloom-wow/after`. Set `BLOXGLOOM_PREVIEW_CONFIG` to an explicit configuration path for a read-only capture with saved lighting/exposure/bloom/parallax. This user's saved values include exposure about 1, bloom strength 1, and parallax depth 0.07; they are preserved. At 0.07, inferred cherry bark relief is about 1.42% of a block; the new 0.125 default produces about 2.53%.

### Remaining approximation limits

- SSR can reflect visible opaque hits; off-screen, hidden, and beyond-range geometry retains the analytic sky/local-light fallback. It is not ray tracing or a scene probe. Materials rougher than 0.82 use fallback only; rough SSR uses one cone with HDR mip filtering, not a full distribution of geometry rays. There are no multiple reflection bounces.
- Inferred heights are conservative normal integration, not recovered authored geometry. They cannot repair nonintegrable artwork or change cube silhouettes; confidence/depth bounds reject unsuitable fields. Relief still fades with distance, mip level, and grazing angle.
- Atmosphere is conservative, sun-visible, single scattering within the rendered shadow volume. It does not implement full ambient volumetric transport, colored volumetric transmission, volumetric clouds, or path-traced GI. It skips GL, shadow-off, and night, preserving existing fog and sky.
- Supported-backend temporal AA stabilizes opaque surfaces; reactive translucent shading retains current-frame detail. Categorical G/B remain base-level point sampled rather than footprint-integrated material mixtures.
- Preset metals retain the [LabPBR specification's albedo tint](https://shaderlabs.org/wiki/LabPBR_Material_Standard). BSL's default disables `ALBEDO_METAL` and also remaps conductor Fresnel and highlight colors, so its preset is an artistic reference rather than an identical material contract. Our Schlick approximation applies preset tint to normal-incidence F0 and approaches white at grazing angles; preserving tint across the complete reflection lobe remains a refinement.
- GPU correctness and inspected headless captures do not establish exact BSL preset parity or the user's subjective visual acceptance. The existing source artwork remains intact for the live test.

### Validation and performance

The full `cargo test --offline --quiet -- --test-threads=4` run passed **1,912 tests, zero failures, 14 ignored**. The normally ignored production daylight GPU calibration was explicitly enabled and passed. Two final preview-only adjustments keep the motion camera in the showcase and report the explicit capture configuration; all 20 preview tests passed after these adjustments. Release build, `cargo fmt --all -- --check`, all-target/all-feature clippy with warnings denied, and `git diff --check` passed. All ten Python conversion regressions passed. `graphify update .` refreshed the repository graph.

GPU regressions exercise real geometry reflections, water receivers above an opaque floor, geometry-only rough reflection mips, black cave fallback, shadowed/lit water, disabled/stale sun maps, water-bounded atmosphere, relief self-shadowing, normal variance filtering, and HDR dark-detail retention. An independent comparison against the pre-upgrade commit confirms alpha-only changes in exactly 91 normal PNGs, with unchanged normal RGB/AO, albedos, material maps, icons, and content identities.

Final production captures include six fixed showcase views, six with the user's saved configuration, six cherry material controls, 30 motion frames, four lighting fixtures, 12 daylight/sky fixtures, and three natural water scenes. Inspected comparisons show more readable shaded bark/masonry, warmer lit foliage, visible grove reflections, and relief on the close stone floor. Inspected motion frames retain clean first-person/return camera cuts; dark and lamp-lit caves retain their distinct lighting. The fixture's simple tree shapes and flat ground remain diagnostic geometry, not a claim that world composition has been redesigned. The user still judges the live appearance.

Local comparison artifacts (left before, right final; original rendered pixels, matched cameras):

- [Cherry bark](/private/tmp/bloxgloom-wow/comparison-bark.png)
- [Early-morning grove and stream](/private/tmp/bloxgloom-wow/comparison-grove.png)
- [Close stream reflections](/private/tmp/bloxgloom-wow/comparison-stream.png)
- [Motion/camera-cut contact sheet](/private/tmp/bloxgloom-wow/motion-contact-sheet.png)
- [Cherry bark with the user's saved settings](/private/tmp/bloxgloom-wow/after/live-settings/noon-bark.png)

The images and logs are local temporary artifacts; the documented commands regenerate them. The baseline binary contains commit `9aef20d` plus the same fixed showcase geometry, with the previous renderer and embedded assets. Baseline and final stills both request temporal AA; final saved-settings captures separately use the user's configuration. Gameplay settings and saves were not rewritten.

Isolated release benchmarks use Apple M1 Pro/Metal, seed `0xB10C6100`, radius 6, 1280×720, medium sun shadows, and 300 steady frames. The following paired runs have AA off, using `perf 300 6` and `perf 300 6 bounced`. Heavy validation/capture jobs had finished before the final timing runs. Setup, mesh size, submit-side CPU time, and timestamped GPU work are reported separately.

| Measurement | Voxel before | Voxel final | Bounced before | Bounced final |
| --- | ---: | ---: | ---: | ---: |
| Scene setup milliseconds | 3737.8 | 3628.4 | 4084.3 | 4074.5 |
| Visible triangles | 92,028 | 92,028 | 96,340 | 96,340 |
| Mesh bytes | 22,891,800 | 22,891,800 | 23,842,728 | 23,842,728 |
| CPU steady p50 milliseconds | 3.102 | 5.912 | 2.605 | 5.953 |
| CPU steady p95 milliseconds | 4.687 | 9.157 | 5.178 | 10.303 |
| GPU steady p50 milliseconds | 3.989 | 6.010 | 4.049 | 6.019 |
| GPU steady p95 milliseconds | 6.901 | 7.096 | 8.413 | 7.814 |

A separate voxel comparison with **AA on in both binaries** measured GPU p50/p95 **4.364/4.767 → 6.483/6.915 ms** and CPU p50/p95 **4.135/5.261 → 6.423/8.640 ms**. Setup was 3644.6 → 3645.5 ms; triangle/mesh counts remained identical. Final gameplay enables AA by default, while the benchmark preserves its existing explicit opt-in.

The coordinated effects add approximately **2.0–2.1 ms median GPU time** (about 49–51% over the previous renderer at equal AA settings). This is a measured quality/cost tradeoff, not a speedup. Submit-side CPU time also increases; it includes command encoding and queue submission and is not a measurement of pure CPU computation. Single-run tails vary, and the benchmark excludes presentation, server work, and live gameplay; these results are not a promised game FPS.

SSR adds approximately **25.2 MiB** of render targets at 720p and nine passes (opaque capture, six radiance mips, half-resolution tracing, composition). Atmosphere adds approximately **2.64 MiB**, two passes, and at most 12 shadow comparisons per half-resolution pixel. Existing temporal resources are used when AA is enabled. Normal variance adds temporary CPU processing during material construction without enlarging material GPU arrays. Controls in `README.md` allow reflections, atmosphere, and AA to be isolated during a live comparison.

The implementation is committed as `dfb5d59`. Restart the release client for the live test. Existing parallax depth 0.07 remains active; 0.125 is the new-config default, so the saved-settings image is the direct reference for this user's current configuration. No new world is required for these rendering changes.


## Final visual/world work — in progress

The user authorized all remaining areas on October 5, including world composition, texture artwork, foliage geometry/wind/transmission, path-traced multi-bounce GI and volume transport, off-screen reflections, volumetric clouds, and BSL styling/parity. The checked-in BSL default is the reference. The goal remains active until validation and outstanding requirements are resolved.

Terrain generator version 8 retains continental coasts, connected ridges, eroded plateaus, directional dunes, contour drainage, exposed rock/snow and regional geology. Regional tree species share stable biome/save tags but have distinct branched crowns and axis-correct branch wood with bark on all six faces; placed logs preserve their cut endgrain. Nine conventional species wood blocks append stable IDs without changing existing artwork or identities; vegetation forms meadow/forest and flower patches. Column sampling is cached. Neighboring chunks and LOD sample the same deterministic generator. The default save target advances to `world-v33/`; existing saves are untouched.

New leaf cards use intersecting oblique sheets and share runtime wind with sun/local shadow casters and scene rays. Fully buried leaves away from timber are culled; timber-adjacent interior leaves remain, with extra slanted sheets covering log ends. Botanical flags exclude unrelated cutouts; tall plants share an anchored, continuous lower/upper wind coordinate. The transmission model adds bounded directional forward scattering. Reproducible artwork curation changes three bark albedos, caps dry botanical smoothness in 55 specular maps, and regenerates three icon entries. Provenance identifies adapted source textures; the original source pack remains unchanged.

New scene transport builds a triangle BVH on workers, rejects stale revisions after edits/removal, and samples diffuse/GGX paths with emissive surfaces, sun visibility, Russian roulette, and participating media. Loaded opaque/cutout geometry can contribute outside screen bounds. Primary camera extinction/scattering replaces analytic fog when the scene is ready; unsupported/oversized adapters and pending scene revisions use the existing voxel/SSR/atmosphere fallback. Finite sampling and reconstruction are still being validated. Do not interpret compilation as visual or performance acceptance.

The sky now has a true 3D cloud volume, dedicated rain and moon-phase inputs, and unchanged JG RTX sun/moon artwork. BSL tone/sky/star equations have source-based regression checks. **Exact whole-frame BSL parity remains unproven**: the reference disables advanced materials, while the requested enhanced renderer includes PBR/path tracing/new cloud density. The source equations, runtime differences and required matched reference capture are recorded in `BSL_REFERENCE_AUDIT.md`. Final image/performance evidence will be appended after validation.

### Findings during final acceptance

The first scene-GI cherry captures failed visual inspection: noisy cloud scattering and dark red/black canopy streaks persisted after increasing history length. Two concrete filtering errors were corrected: primary mapped normals were not consistently facing the viewer, and a deterministic negative sky-specular correction was being spatially filtered and remodulated by neighboring diffuse colors. Specular removal now occurs at the exact full-resolution receiver after reconstruction, using a separate primary transmittance target. Shader/GPU tests protect this contract; it is not a claim of final visual acceptance.

Source audits rule out accidental cherry-leaf metal decoding or black texture AO: visible leaf green is dielectric F0 approximately 0.0392, perceptual roughness is at least 0.498, and AO has median 0.957. Bedrock MERS subsurface strength was being interpreted directly as calibrated sheet transmission, causing repeated pink diffuse-albedo absorption. A separate bounded reflected/transmitted optical model now distinguishes source scattering strength from species optical thickness. This runtime calibration leaves source artwork and categorical channels intact.

Generated meadow captures exposed another failure: distant trees and terrain turned white despite no snow in the sampled world. Distance fog used the legacy bright horizon palette while the sky used the BSL linear-radiance equation and tone exposure four. Fog now obtains directional source HDR air color from the BSL equation rather than that incompatible palette. Exposure settings are unchanged.

The initial near-axis leaf shell retained stepped canopy walls. Boundary voxels now use intersecting oblique cards with correct geometric normals; enclosed voxels and loaded chunk seams are culled. Shadows, camera geometry and scene rays use the same cards and wind.

The initial full-scene GI performance probe also failed to finish within three minutes and was canceled. It has no usable timing result. An isolated same-build GI-off `perf 300 6` control measured scene setup 5602.2 ms, 156,972 visible triangles, 34,739,184 raster vertex/index bytes, 25,202,496 retained CPU ray triangle bytes, CPU steady p50/p95 8.962/10.200 ms and GPU steady p50/p95 9.941/10.176 ms. These are intermediate measurements before the final canopy/fog/transport changes, not final performance acceptance. Worker-payload admission figures include retained ray triangles and must not be mistaken for GPU draw-buffer size.

Ray foliage deformation now runs in a once-per-triangle GPU compute pass before traversal; the previous path repeated wind trigonometry for every candidate triangle intersection. The scene BVH conservatively includes wind bounds. Final captures, full checks and timing comparisons remain pending.

The optimized same-scene GI probe (16-bin SAH, GPU wind preparation, calibrated optics, quarter-size targets) completed, but GPU steady p50/p95 was 202.670/204.386 ms for ten frames. CPU submit p50/p95 was 105.473/106.693 ms and ray BVH setup 111.2 ms. This is an unacceptable gameplay cost, not a successful performance result. Full path transport is therefore explicitly opt-in with `BLOXGLOOM_GI=1`; default voxel lighting, SSR and atmosphere remain available. Higher-quality half-size transport is the default within that experimental mode; `BLOXGLOOM_GI_SCALE=4` trades spatial sampling for lower cost. The goal remains active while GI performance and whole-frame BSL parity are unresolved.

For cost isolation, `BLOXGLOOM_GI_PROFILE=1` enables headless-only steady-frame GPU stage timestamps (deformation, transport, filter, composite); `BLOXGLOOM_PERF_PRELOAD=1` excludes the normal upload ramp from diagnostic samples. Neither reduces bounce count or changes the scene. Actual path tracing must also be requested with `BLOXGLOOM_GI=1`. Normal benchmarks retain the upload ramp.

The ray/raster alpha mismatch is now classified explicitly: moving opaque foliage uses ordinary negative reactive visibility, while water writes the distinct −2 marker. A filtered leaf accepted by raster alpha but rejected by mip-zero ray alpha retains its raster surface fallback and still receives primary medium extinction/scattering. It cannot become a glossy water receiver or index an invalid triangle. GPU regressions use actual imported cherry alpha and protect retained full-resolution specular/medium signals.

A natural grove exposed 449 upward-facing log ends beneath generated leaves. Culling discarded 434 of those adjacent leaf cells, letting sparse boundary cards expose the cut timber. Retaining timber-adjacent cells and adding cap coverage increases leaf triangles from 9,014 to 16,484 in the same 64-chunk grove; raster payload rises from 2,001,792 to 2,987,832 bytes, and debug mesh-building time from 93.1 to 100.5 ms. This is an intentional density cost; it exceeds the old cube-shell triangle budget. Fresh natural-crown images must establish whether the geometry fixes the appearance.

An earlier preloaded quarter-resolution GI probe measured whole-frame GPU median 193.669 ms. Its reported deformation (0.530 ms), transport (96.428 ms) and filtering/composition (99.812 ms) timestamp intervals overlap on Metal and cannot establish exclusive pass costs. Filtering now runs at transport resolution, followed by material-aware full-resolution reconstruction. Cloud-free primary air uses closed-form optical depth and conditional scattering-distance sampling; weak-medium continuation uses compensated probability sampling, preserving expected scattered energy. Finite cloud integration and multi-bounce paths remain. Fresh performance measurements are pending; these changes are not yet a gameplay-performance claim.

Enhanced primary directional diffuse was missing the Lambert `1/π` conversion while secondary ray diffuse and specular BRDFs treated the same source as irradiance. Opaque, thin-sheet and botanical directional diffuse now use consistent units. Independent actual-GPU angle checks compare primary and secondary material helpers; ambient/local light, emission, specular, source intensity, exposure and BSL reference equations are unchanged. Botanical wrap/forward shaping remains an intentional difference from secondary Lambert lobes.

A final correctness review identified three additional integration issues. Whole-scene GPU buffer initialization and initial pipeline compilation now run on the ray worker; live frame admission swaps prepared objects and rejects obsolete revisions, with the first request queued before readiness is evaluated. The headless fixture path retains explicit synchronous preparation off the window thread. Actual GPU startup/admission/resize/removal verification passed. Traced sky formerly remained zero for an indoor receiver even when its secondary ray escaped through an actual opening. A compact loaded-3D-chunk mask now certifies upward paths through complete loaded space; unknown, missing, lateral and downward paths keep the conservative voxel fallback. GPU tests distinguish a closed room, opened roof, missing vertical cell and missing lateral column. This does not establish absence of saved roofs beyond streamed interest; that guarantee requires authoritative ceiling metadata.

Fire/rain sprites are drawn after primary transport. They now have GI-only air/cloud visibility attenuation using the traced air law and deterministic cloud integration; source RGB remains unchanged and alpha fades by transmittance. This is emissive billboard contrast fading over the already resolved background, not exact absorbing-sheet transport or per-particle multiscattering/front-segment in-scattering. Actual GPU compositing checks passed for air/cloud extinction, emission, fallback, scissored background and zero alpha.

The user explicitly selected a separate BSL reference mode alongside enhanced rendering. The optional reference air fog now follows checked-in default distance, night/rain density, height, exterior/minimum-light and bedrock equations, with independent source goldens. Engine shelter/skylight is a stated proxy for Minecraft eye brightness. Reference cloud work uses the user's local BSL noise file at runtime; no restricted BSL artwork is bundled or relabeled as redistributable. Runtime/frame equivalence remains unresolved; source underwater composition and explicit nearest-front depth are implemented at the later checkpoint below.

Fresh enhanced natural-crown images show fuller pink/green foliage but still conspicuous rectangular branch end faces. The meadow's bright middle stripe samples actual sand/gravel rather than snow. The current quarter-resolution GI profile after filtering/air/cap/unit changes measures whole-frame GPU p50/p95 **179.535/203.789 ms** and CPU submit p50/p95 **2.458/3.862 ms**, in a ten-frame preloaded diagnostic. This remains unacceptable for gameplay. Metal pass timestamp intervals overlap; raw timestamp order is being checked before attributing exclusive filter/transport costs. These are intermediate evidence, before the additional coverage/particle/worker/reference-cloud changes, not final acceptance.

The natural branch defect is now traced to stepped horizontal log cubes exposing repeated cut endgrain, including builtin oak branches incorrectly retaining the vertical log state. Nine append-only species `*_wood` blocks reuse existing bark artwork on all six faces. Generated living boughs use their horizontal-axis states; trunk logs and placed logs retain existing identities and endgrain. This changes neither branch voxel counts nor canopy density. Existing block definitions, texture entries and icon entries remain equivalent; nine new icons are appended. Generator 8/default `world-v33` separates the changed generated content from old local saves.

A source-basis lighting audit found clear-noon legacy ambient fill about **9.00 times** the luminance of the actual sky's upward Lambertian convolution; downward fill was about **428 times** its nearly dark ground-masked sky. The legacy zenith reflection fallback was about **19.0 times** the visible source sky. Enhanced diffuse fill now uses a reproducibly generated 46.9 KiB sky-convolution table. The first correction only reached the point-sky helper; a subsequent actual-caller audit found that PBR reflections still used the legacy palette. The shared source-sky prefilter now reaches raster PBR/water, SSR replacement and ray-composite replacement with explicit HDR sky, rain and lunar phase inputs. Independent GPU quadrature and production camera/environment checks pass. The existing up/down normal blend remains a broad approximation and omits azimuthal sky structure. Reference modes, legacy style, cave masks, local light, emission, intensity controls, texture pixels and exposure are preserved. Fresh rendered acceptance is pending.

The worker-prepared live GI fixture passed actual GPU startup, admission, resize-during-build and removal checks. Final exact-zero path termination checks preserve current direct/emissive radiance, tiny positive energy and the 12-bounce maximum; there is no epsilon cutoff. An opaque-only sunlight precheck has actual-GPU equivalence coverage for alpha holes, colored foliage sheets and segment bounds. It stays disabled unless explicitly requested with `BLOXGLOOM_GI_OPAQUE_PRECHECK=1` until same-build profiling establishes a benefit.

The medium-only denoiser incorrectly treated equal-distance sky samples as points on a surface plane, although those endpoints form a sphere. Angular/spatial medium weights and explicit sky/foreground segment classification now protect both filtering and reconstruction. Production GPU checks pass at transport scales 2/4/8 with the unchanged 0.16 tolerance; the scale-8 sky fixture error falls from 0.1891 to 0.0108. Scale 8 is a sampling tradeoff, not equivalent image quality.

Cloud-free secondary air now samples exact optical depth and inverts the existing height-density integral; actual scattering events and subsequent multi-bounce paths remain. Sunlight extinction integrates air analytically and clouds only inside the clipped slab at the original nominal spacing. Twelve production transport checks pass, including closed rooms, loaded openings, missing-space conservatism and exterior scattering witnesses. A 64-ray cloud comparison improves mean transmittance error from 0.01234 to 0.00709 against dense quadrature.

Same-build preloaded ten-frame diagnostics after these changes measured scale-8 whole-frame GPU p50/p95 **79.580/81.904 ms** with stackless traversal and **69.802/70.804 ms** with optional near-first traversal (about 12.3% lower median). CPU submit p50/p95 was **2.984/3.386 ms** and **3.031/3.728 ms**, respectively. Scene setup was approximately 5.5 seconds, with 160,756 visible triangles, 35,389,680 raster bytes and 25,675,584 retained CPU ray-triangle bytes. A preceding scale-4 probe measured GPU **157.676/175.197 ms**. These remain unsuitable gameplay timings and are intermediate diagnostics, not the final 300-frame acceptance benchmark. `BLOXGLOOM_GI_NEAR_FIRST=1` selects a validated 16-entry near-first stack with a correctness-preserving stackless overflow fallback; it remains optional until broader timing comparisons. Exact texture alpha, wind, finite limits and triangle/UV/normal ties are covered by actual GPU tests. Raw Metal timestamps confirm overlapping stage intervals: filter and composite start before transport's reported end, so their intervals are not exclusive costs.

The final same-build exact-optimization comparison used the M1 Pro at 1280×720, `perf 10 6`, preloaded geometry, GI enabled and near-first fixed on. Scale-8 whole-frame GPU p50/p95 was **72.016/75.063 ms** with both controls disabled, **69.297/94.262 ms** with material fastpaths only, **64.195/66.997 ms** with tighter stationary bounds only, and **63.229/64.465 ms** with both. Paired repeats measured **71.764/72.851 ms** disabled and **63.630/75.475 ms** enabled: the median gain repeats at approximately **11–12%**, while tails remain variable. Scale-4 controls measured **149.083/171.737 ms** disabled and **114.661/116.243 ms** enabled, a **23%** median reduction. CPU submit medians remain approximately **2.6–3.3 ms**; scene generation takes **5.4–5.6 s** and BVH construction **126–141 ms**, excluded from frame samples. Raster/worker payload remains **61,065,264 bytes** and visible geometry **160,756 triangles**; tighter partitioning adds **768 BVH bytes**. The optimizations skip unnecessary opaque material work and remove wind allowance only from catalog-confirmed stationary geometry. Fourteen actual GPU transport fixtures pass with both enabled, including caves, alpha, wind, media and traversal limits. Both now default on within opt-in GI, with `BLOXGLOOM_GI_MATERIAL_FAST=0` and `BLOXGLOOM_GI_TIGHT_BVH=0` retaining explicit controls. Near-first remains optional. Twelve bounces, roulette, volumetric scattering and the scene are unchanged. These are short diagnostic comparisons, not final 300-frame acceptance; full GI remains impractical on this hardware, and overlapping Metal pass intervals do not establish exclusive stage costs.

Fresh renders expose two more integration problems. Natural headless captures had a 38–135 m near-only fade while default gameplay's distant-terrain horizon is 512 m; natural previews now include the production LOD pipeline, ready-chunk mask and matching horizon. They also honor explicit saved bounced-lighting settings, while named cave controls retain their declared mode. Loaded empty preview chunks now remain in ray coverage. Separately, the earlier point-sky correction did not reach actual PBR fallback callers, which still used the legacy GGX horizon/zenith palette. Material highlights, water, SSR replacement and ray-composite replacement now share one explicit source-sky prefilter. Independent actual-caller GPU checks pass. Fresh bark captures show less blue specular contamination, but shaded bark and foliage remain dark/cool; this is an observed acceptance concern, not a final visual-quality claim.


The 512 m natural capture revealed unresolved enhanced water waves aliasing into strong distant moiré bands. The shared near/LOD fragment helper now integrates sinusoidal slopes over the pixel footprint and fades frequencies before Nyquist. Unresolved slope variance increases GGX roughness rather than vanishing. An actual fragment GPU test compares anisotropic derivatives and filtered slopes against numerical Gaussian integration, including exact far-wave removal; original water visibility/cave fixtures still pass. A fresh natural render then caught a missing fourth sun-shadow bind-group layout in the new reference LOD lighting branch before drawing. The layout and draw binding were corrected; an actual production-pipeline layout regression now covers this integration rather than relying on WGSL parsing. Final image inspection is pending the rebuilt binary.

Reference near/cutout albedo now converts the actual hardware-linear sample back through the sRGB transfer and into source power 2.2. Independent ideal input goldens and actual hardware-decoder consumer baselines pass separately; hardware dark-toe precision is documented. Reference LOD now applies that transfer, source forward lighting/desaturation and material-aware source sun shadows, retaining coarse face-material identities without enabling their detailed texture samples. Linear mip/average colors, quantized LOD color, source lightmap proxies and geometry remain runtime differences.

Source-default reference water now has green-channel counterflow noise, four-step parallax normals, source palette/basic SSS/Fresnel/specular and reflected source clouds. A private sqrt-space HDR target preserves ordered source alpha layering before returning to the engine's linear HDR scene. Near/LOD parser, 24 actual noise/parallax GPU cases and 0/1/2-layer blend/resize checks pass. Source-default lens flare now precedes reference display gamma, with independently checked ghost/ring/FOV/moon equations and submitted-only depth-visibility history. Source-default current-frame water SSR now samples a private RGB10_A2 encoded opaque reflection mip chain and immutable opaque depth before the nonlinear water blend; independent production-helper GPU checks cover source marching, masks, border fade, decode and translated world coordinates. Reference-only near/LOD water writes a copied nearest-front depth attachment for the implemented underwater composite, while original opaque depth stays unchanged; actual source-fragment GPU draws verify occlusion and ordered layers. Enhanced post remains separate. Whole-frame source parity still requires matched runtime inputs and images.

### Procedural forest horizon geometry

Coarse forest samples formerly expanded a single sampled log or canopy span over an entire 4/8/16 m cell, producing floating leaf cuboids and oversized stems. Builtin server summaries now carry bounded 27-byte source-tree descriptors. Metre-wide connected bark stems/boughs and rounded, double-sided leaf contours retain real cutout coverage at every distance; their dimensions never scale with the sample width. Adjacent tiles clip the same deterministic source shapes, and the outer ring selects one actual tree per global 24 m cell rather than enlarging trees. The server certifies the same root-support/omission decision for both clipped crown halves. A stem may continue down at most 3 m to certified exterior opaque terrain; larger cliff gaps, water and unknown support omit only the coarse proxy. Source anchors, canonical tree occupancy and authoritative fine forests remain intact. Its extra support interval participates in parent-retirement coverage.

Wire version **34** and disposable LOD cache identity **v3-forest** invalidate older summaries. Saved block IDs, artwork, generator version 8 and `world-v33` remain unchanged. Package/snapshot summaries retain their exact existing span representation. Any actual saved edit conservatively disables forest proxies for that builtin tile; unchanged cached snapshots do not. Unknown crown/support coverage and incomplete descriptor families cannot retire their parent.

The **60 KiB packet**, **8 MiB mesh**, 32 spans per column, and 2,048 coverage intervals remain fixed. The cumulative span allocation bound now derives from the existing wire-byte allowance; the decoder charges coverage, spans and tree descriptors against that allowance before allocating each group. This admits four previously rejected coastal refinements without discarding cave gaps or raising the packet budget: 3,106–3,235 spans, 55,862–57,612-byte packets, 478–744 KiB meshes and 9,200–14,320 triangles. Source/species geometry, negative seams, root/cave/unknown support, edited snapshots, parent retention and malformed allocation groups have focused regression checks.

The actual cherry-grove 512 m CPU probe admits **77/77 tiles** (previously 73/77), with 3,629,085 summary bytes and 63,091,788 mesh bytes/1,259,914 triangles, compared with the prior 49.2 MB/946,050 triangles. Real tree contours and recovered refinements cost additional geometry; tile budgets remain unchanged. The source-support check omits 0/1/19/59 descriptors at levels 1/2/3/4; every retained continuation is at most 3 m. Large cliff gaps are preserved as source metadata, not invented trunks. The verified release grove capture measures 1,285.57 ms summary generation, 20.56 ms parent reduction and 418.91 ms meshing, compared with the prior 1,320/395 ms summary/mesh control. These are scene setup costs, separate from frame timing. Independent inspection of `bloxgloom-final-verified-world/03-cherry-grove.png` and `02-coast.png` confirms rounded distant crowns, narrow ground-connected stems, no visible clipped crown seam or invented tall trunk, and the disappearance of the long ocean frontier strip. Near pink/green foliage remains distinct. Distant ocean tonal/wave bands and stepped coarse terrain remain separate visual limitations; these captures do not establish whole-frame BSL parity.

The ocean's long dark seam was independently identified as the near residency box's unknown-neighbor fluid sidewall: its source edge projects exactly to the captured seam. Both near and distant ocean caps remain at y=17 and enhanced waves change normals only. The separate near-water mesh fix withholds horizontal frontier sidewalls until an authoritative neighbor confirms exposure; actual known shores/water/solid boundaries retain their normal behavior.

### Previous committed release verification

The integrated suite passes **2,041 tests**, with **zero failures and 19 ignored**, in 423.38 seconds. `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `git diff --check` pass. The actual LOD depth/reflection lifecycle regression also passes separately with `BLOXGLOOM_BSL_REFERENCE=1`. These checks include rendered GPU readbacks, not only shader parsing. Generated images were inspected; subjective gameplay acceptance remains with the user.

The following serial measurements use Apple M1 Pro/Metal, seed `0xB10C6100`, 1280×720, radius 6, medium shadows, temporal AA off and 300 steady frames. No compiler, other benchmark or GPU fixture ran concurrently. Commands are `perf 300 6`, `perf 300 6 bounced`, and `lod-perf 300 6 512 voxel`, using frozen release binaries. The preceding checkpoint already includes generator 8, bark branches, foliage, clouds and eight-wave water; this comparison isolates the final directional sky, fluid-frontier and distant-forest changes. It does not measure the entire overhaul against the original game.

| Scene | CPU p50/p95 before, ms | CPU p50/p95 final, ms | GPU p50/p95 before, ms | GPU p50/p95 final, ms |
| --- | ---: | ---: | ---: | ---: |
| Voxel, near scene | 10.432 / 11.664 | 10.749 / 12.062 | 11.603 / 12.738 | 12.023 / 13.105 |
| Bounced, near scene | 10.481 / 11.795 | 10.879 / 12.154 | 11.638 / 12.819 | 12.062 / 13.140 |
| Voxel, 512 m horizon | 11.227 / 12.436 | 11.638 / 12.777 | 12.491 / 13.506 | 12.907 / 13.847 |

Near-scene generation/lighting/meshing is excluded from frame samples: voxel **5,302.7 → 5,380.3 ms**, bounced **5,595.7 → 5,694.0 ms**. Withholding unknown frontier water walls removes 106 visible triangles and 18,576 raster bytes; retained ray geometry and cutout counts are unchanged. Final voxel worker payload is **61,046,688 bytes**, comprising **35,371,104 raster bytes** and **25,675,584 CPU ray bytes**; final bounced payload is **63,385,968 bytes**. Visible triangles are **160,650 voxel** and **166,758 bounced**.

The 512 m benchmark keeps **77 resident / 28 selected tiles**. Total distant mesh storage changes **70,351,736 → 83,822,444 bytes**; total generated distant triangles change **1,352,918 → 1,658,962**, while final visible near+distant triangles change **620,542 → 669,952**. Its separate source-summary/parent-reduction/mesh setup changes **1,484.38/27.66/509.80 → 1,484.98/28.68/493.55 ms**. Rounded forest geometry costs memory and triangles; median GPU cost rises approximately **3.3%** in this scene, versus **3.6%** in the near controls. These single paired runs do not establish universal performance or live FPS. CPU samples include submission/driver work and can reflect queue backpressure; GPU stage intervals must not be summed as exclusive costs.

Final experimental GI uses `BLOXGLOOM_GI=1 BLOXGLOOM_GI_SCALE=4 BLOXGLOOM_GI_NEAR_FIRST=0 BLOXGLOOM_GI_MATERIAL_FAST=1 BLOXGLOOM_GI_TIGHT_BVH=1 BLOXGLOOM_PERF_PRELOAD=1`, `perf 300 6`. GPU p50/p95/p99 is **135.168/173.223/189.449 ms**; CPU submission is **68.602/85.891/97.027 ms**. Scene setup is **5,413.8 ms**, BVH preparation **128.2 ms** and preload **55.9 ms**, all excluded. The BVH stores **30,152,304 bytes**. This longer default-traversal run differs from the earlier short near-first diagnostics; it confirms unsuitable gameplay cost. The benchmark's HUD FPS label is fixture text, not measured performance. Its near-only, AA-off image is a diagnostic, not the natural 512 m visual acceptance scene. Inspected GI and voxel control images retain the same dithered near-residency edges; those gaps are not a GI-only defect. GI changes the shaded valley's diffuse light and air contrast but this static capture does not establish temporal convergence or live quality.

Frozen binary SHA-256: preceding checkpoint `d56222bdb3e8111e66f50ef6268e8da4e45900927bfe856fdcf208b454a814c3`; final `99c0f7cd34eecd206e06627ba0a3129f355d48dfd3efef8d0a33201ec92b0258`. Raw logs are `/private/tmp/bloxgloom-final-perf-{before,after}-{voxel,bounced,lod512}.log` and `/private/tmp/bloxgloom-final-perf-gi4.log`.

Inspected comparison images: enhanced landscapes in `/private/tmp/bloxgloom-final-verified-world/`, updated bark in `/private/tmp/bloxgloom-final-angular-bark/`, and the successfully rebuilt checked-in-default comparison at `/private/tmp/bloxgloom-final-reference-world/03-cherry-grove.png`. The latter uses the final binary and submitted reference history. Enhanced captures predate only the reference-specific bind-group split; their enhanced rendering path is unchanged. Reproduce them with `BLOXGLOOM_TAA=1 cargo run --release -- landscape-preview <output-dir>` and add `BLOXGLOOM_BSL_REFERENCE=1` for the reference mode. No matched Minecraft/Iris frame has been provided, so exact whole-frame parity remains unproved.

### Complete water ray paths — physics checkpoint

Enhanced GI now admits the near raster mesher's actual fluid interfaces and the selected server LOD water geometry. Each triangle preserves its outward normal so entry and exit are distinct even when the intersection normal faces the ray. The shared raster/ray wave spectrum uses the same clock; the camera's first water vertex also carries the raster pixel footprint and unresolved-slope roughness. Reference-mode water stays on its separate source-default renderer.

Medium membership comes from authoritative near voxel masks (512 bytes for a mixed 16³ chunk, compact classification for a full wet chunk), followed by selected coarse coverage/wet intervals. Loaded dry cells override overlapping coarse water. Unknown neighbors and vertical holes remain unknown. The production traversal now carries exact crossed boundaries: GPU tests caught both cancellation in negative coarse-column indexing and a reconstructed diagonal crossing that one float32 ULP did not clear. Strict negative, corner, zero-plane and shallow X=512 crossing tests pass after those fixes.

Water paths use an IOR of 1.333, exact Fresnel/Snell/TIR, GGX reflection and transmission, reciprocal radiance-mode eta factors, spectral channel-mixture free-flight sampling, and molecular phase scattering. Representative pure-water RGB extinction at 650/550/450 nm is `[0.3406803, 0.0579, 0.0125513]` per metre, combining published absorption with a wavelength-dependent molecular scattering approximation. These three wavelengths are an RGB approximation, not full spectral reconstruction or a conversion of raster alpha into absorption. Actual dielectric vertices connect to a finite solar disc with MIS; the generated-disc NEE uses its known PDF to avoid a rounded support-edge `0/0`. Straight shadow rays do not pretend they refract through water.

The nearest admitted primary interface selects complete positive camera-path radiance, including the water volume and opaque objects reached through it. That signal is denoised separately and replaces the immutable raster HDR baseline at the exact full-resolution center pixel. It does not subtract a filtered ambient or specular approximation. Water markers without matching admitted geometry keep their raster fallback. Missing volume coverage does not invent a dry exit or certify exterior sky.

Verification: three scene/occupancy/seam CPU tests pass; production shader parsers pass; independent GPU optics/free-flight/solar tests and actual appended-volume lookup tests pass. A real voxel-pool GPU fixture checks nearest primary interfaces, dry emitter energy, three-metre spectral refraction/absorption, opaque blocking and refracted solar escape versus a genuine sealed enclosure. Its 4,096 statistical paths per camera group are distributed across GPU fragments rather than one unusually long invocation. Existing primary cutout/fallback regression passes. Six GPU filter/compositor tests pass, including exact-center complete-water replacement at half, quarter and eighth resolution.

This checkpoint proves these transport contracts. Fresh complete-scene water images and frame-cost acceptance are still pending. Narrow refracted caustic paths can have high variance at gameplay sample counts; supported finite paths alone do not establish stable caustic image quality. Saved/unknown space beyond admitted coverage remains a stated finite-scene limitation.

### Integrated scene capture acceptance — current investigation

The complete production path can reflect a real, nonempty admitted LOD emitter behind the camera; removing its pages or adding a near opaque blocker correctly removes that radiance (actual GPU test, 19.87 s). Current-pose/material/receiver, page/revision/quota, water occupancy/physics and immutable HDR cancellation tests also pass independently.

However, the first 32-frame full-scene GI coast capture at `/private/tmp/bloxgloom-ray-complete-gi-world/02-coast.png` **fails visual acceptance**: water has severe coarse binary speckle. The default enhanced captures at `/private/tmp/bloxgloom-ray-complete-world/` were rendered and inspected separately. Passing transport/energy fixtures does not make the experimental water appearance acceptable. That captured primary-water estimator chose reflection versus transmission stochastically and kept only a six-frame history. The subsequent conditional primary-lobe splitting/history checkpoint below has not yet been accepted visually. Underwater refracted solar paths can also have high variance. This initial image is diagnostic evidence, not a proposed finished result or a BSL parity comparison.

### Full-scene transport integration — default-renderer cost checkpoint

The release `bd37c9a18e617734572b3dfdbd93b5e9c19095e8833739677918671a4d78606e` includes LOD pages, fluid occupancy and current-pose actor/pickup targets. Serial AA-off, GI-off M1 Pro/Metal runs use the same 1280×720 radius-6 fixture and 300 steady frames. These timings do **not** measure the new complete tracer.

| Scene | CPU steady p50/p95, ms | GPU steady p50/p95, ms | Near setup, ms | Retained near worker payload, bytes |
| --- | ---: | ---: | ---: | ---: |
| Voxel | 10.840 / 12.313 | 12.047 / 13.363 | 5325.0 | 61,150,260 |
| Bounced | 10.825 / 12.424 | 12.117 / 13.536 | 5630.6 | 63,489,540 |
| Voxel, 512 m horizon | 12.014 / 13.585 | 13.398 / 14.535 | 5286.2 | 61,150,260 |

The first voxel run may overlap a briefly started and stopped CPU shader-test compilation; its CPU value is diagnostic rather than a strict isolated paired result. The bounced and LOD runs have no other compiler or GPU job. Visible geometry remains 160,650/166,758 near triangles and 669,952 near+distant triangles. Distant geometry remains 77 resident/28 selected tiles, 83,822,444 GPU bytes and 1,658,962 total generated triangles. The LOD setup reports source summary/reduction/meshing 1478.66/28.75/506.84 ms separately; static ray preparation is excluded from measured steady frames. Added authoritative fluid masks and actual fluid triangles increase retained near worker bytes by 103,572. Raw logs are `/private/tmp/bloxgloom-ray-complete-perf-{voxel,bounced,lod}.log`.

These measurements precede the camera-water conditional-sampling change and the single captured raster/LOD/ray water clock. The sampling change runs only with experimental GI; fresh complete-tracer timing and image acceptance remain required.

### Primary-water variance correction — fixture checkpoint

The first camera water interface now samples reflected and transmitted GGX lobes independently, retaining the actual Fresnel allocation and each conditional PDF in BSDF weights and solar MIS. This removes the binary first-interface Fresnel choice without boosting radiance or clamping throughput. The first interface consumes one of the twelve path vertices; both continuations have at most eleven remaining. Camera air integrates once before their combined signal, using deterministic extinction and the existing unbiased in-scattering estimator. An air-scatter continuation also accounts for its first vertex. Compatible complete-water planes accumulate up to 32 frames; depth, geometry-class, roughness and normal disocclusions still reject old history.

Independent GPU tests compare the old mixed sampler and new split estimator at normal, oblique, grazing and total-internal-reflection angles, with smooth closed-form Fresnel/eta radiance oracles and rough GGX cases. The actual voxel-pool fixture verifies two first-water continuations, one camera-air integration, age-eight to age-nine history accumulation, and rejection after depth/class/normal changes, alongside real refraction, Beer absorption, blockers and sealed/open solar paths. Exact authoritative-volume GPU traversal also passes. These tests validate the estimator and history contract; the severe failed full-scene coast image remains the visual baseline until a fresh production capture demonstrates improvement. Secondary dielectric caustics and spectral absorption roulette may still produce variance; no finished quality or frame-cost acceptance is claimed here.

### First primary-water revision — image acceptance

The first-interface lobe split, shared per-frame wave time and compatible 32-frame history are now verified by optical mean-energy, actual pool, stale-history and primary-medium regressions. The frozen release `08a3b967e37ab05c6d43921e9779a9d79f4c0290027d04e1ace4489c0cb4479c` produced `/private/tmp/bloxgloom-water-split-world/02-coast.png` using the unchanged full-decoder traversal controls. Independent inspection still **fails visual acceptance**: the coarse water grain decreases but remains severe, with sparse bright caustic/glint patches. This is not the finished water result.

The next estimator samples molecular scattering rather than absorption-plus-scattering events, and applies the unchanged spectral absorption analytically through Beer weights. Independent tests preserve physical RGB survival/collision energy and check the new proposal CDF. Its tested survival variance is less than 6% of the previous estimator over 3/12/30/100 m. A fresh production image is still required; expected-energy and variance tests alone do not establish a satisfactory rendered coast.

### Water absorption variance — scattering-only checkpoint

The split-only 32-frame coast capture (`/private/tmp/bloxgloom-water-split-world/02-coast.png`) still fails visual acceptance: coarse ocean grain and sparse bright glints remain. The previous extinction-mixture estimator survived a 12 m segment on only 45.87% of draws, producing intermittent blue survival weights. The new estimator samples free flights from the unchanged molecular scattering coefficients and weights absorption analytically. Collision weighting is `exp(-sigma_t*s)*sigma_s / mean(sigma_s*exp(-sigma_s*s))`; survival weighting is `Beer(L) / mean(exp(-sigma_s*L))`. Both retain the same spectral expected energy. At 12 m the new survival probability is 97.87%, with analytically predicted blue survival variance 1.85% of the old value; this is an estimator result, not an image-quality claim.

Independent GPU oracles pass at 3/12/30/100 m for the scattering-only event CDF, physical Beer RGB, integrated collision RGB, unchanged old/new mean energy, and predicted survival variance. Existing optical/volume checks pass (five-test group, 1.18 s), actual pool/refraction/blocker/sealed-cave/history checks pass (40.13 s), secondary-air checks pass (three tests, 0.47 s), full transport shader validation passes, and six temporal/filter/compositor GPU checks pass (3.58 s). Raw logs and preserved binary/source hashes are consolidated in `/private/tmp/bloxgloom-water-scattering-acceptance-evidence.json`. Fresh full-scene acceptance and measured tracer cost remain pending; narrow dielectric solar paths can retain high variance.

### Water wave-normal diagnosis

The fresh scattering-only coast capture still fails image acceptance; the survival-variance improvement alone does not cure the production grain. A GPU probe of the actual shared wave/interface/sampling functions covers 64 coast-coordinate positions/azimuths, four view cosines (0.6/0.2/0.05/0.01), six clock times and 128 samples per lobe. Every interface faces its incoming view and all 196,608 transmission samples remain valid. Reflection acceptance averages 89.7–100%, depending on grazing angle. The existing view-facing fallback already prevents invalid mapped-normal incidence, and rejected reflected rays retain the geometric no-leak contract. Zero incidence or simultaneous rejection of both lobes does not explain the observed black cells, so no wave-normal boost or guard removal was applied. Raw GPU diagnostics are `/private/tmp/bloxgloom-water-interface-probe.log` (0.82 s); production grazing-plane temporal reprojection is being investigated separately.

### Grazing-water temporal history and large-coordinate camera rays

The full-scene coast capture remained severely noisy after conditional reflection/transmission splitting and scattering-only absorption. Investigation found two concrete reconstruction defects. Complete-water history compared the previous low-resolution texel's radial depth to the current jittered sample's radial depth. These samples can lie on the same horizontal interface while differing by metres in range at grazing angles. The original synthetic constant-depth history fixture did not exercise this geometry.

The correction reconstructs the previous texel's actual ray from the previous forward view-projection matrix, using the production sample center (`history_pixel * stride + floor(stride / 2) + 0.5`). It validates the stored point against the current geometric interface plane while retaining material class, normal, roughness, age and foreground/parallel-layer rejection. Generic opaque history retains its existing validation. `BLOXGLOOM_GI_WATER_PLANE_HISTORY=0` selects the legacy water check for same-build diagnosis.

This exposed a second defect at the actual coast origin (`eye=(-617.5,37,-2015.5)`). Float32 world-inverse unprojection and far-point/eye subtraction perturbed a previous camera ray enough that a stored distance of 267.5 m landed **0.131 m off the known water plane** when reconstructed from stable forward-matrix rows. Ray-frame inverse projection now removes camera translation before inversion; primary rays, opaque distances and compositor world positions use camera-relative vectors and add the eye only for world positions. The uniform layout is unchanged; companion fixtures follow the same semantics.

The real-camera GPU regression preserved all **1,728** valid coast samples across eight TAA jitters, transport strides 2/4/8, and coast/negative/distant translated origins (offsets up to 32 km); the legacy radial test rejected **654** of them. Six real disocclusion/class/normal/roughness/age controls still rejected history. Scale 2/4/8 reconstruction and foreground/parallel-depth-edge denoise checks passed. The broader trace reconstruction checks passed **63 tests, two ignored**, including actual pool refraction/Beer/blockers/solar escape, primary alpha fallback, sky/cloud transport and live worker revision/resize admission. The first run exposed a stale near-first traversal fixture (four bindings against the new eight-binding layout); updating its empty LOD/dynamic resources and 304-byte frame/48-byte coverage payload restored its alpha/wind/finite-segment/tie/overflow oracle.

### Production coast history comparison

The frozen release `c4fe3aae814045570099deb1e44eceadfa36530611e5b8a7afefdb1e798e7823` rendered the same coast for 32 frames at quarter-size transport, with TAA and the same camera-relative rays. Plane history yields mean water age **31.618**: 17,157 of 17,378 water pixels reach age 32, and 137 remain at age one. The legacy radial control yields mean age **24.211**: 12,899 reach age 32, and 3,340 remain at age one. `BLOXGLOOM_GI_DIAGNOSTICS=1` reports the final water history histogram outside frame timing.

Both production images were inspected: `/private/tmp/bloxgloom-plane-history-world/02-coast.png` and `/private/tmp/bloxgloom-plane-history-legacy-world/02-coast.png`. Broad grain is substantially reduced, but bright cyan caustic speckles and quarter-resolution blur still **fail visual acceptance**. The histogram proves the history correction works in the integrated renderer; it does not establish satisfactory caustic sampling or BSL parity. Unbiased refracted-sun guiding is being validated separately.

### Underwater diffuse caustic guiding — correctness checkpoint

Plane-aware production history now accumulates broad ocean illumination (mean age 31.618; 17,157 of 17,378 sampled water pixels reach age 32), but `/private/tmp/bloxgloom-plane-history-world/02-coast.png` still fails visual acceptance because sparse bright cyan paths remain. Underwater opaque diffuse continuations now mix 50% cosine sampling with 50% sunlight guidance through reciprocal Snell refraction. Two real first-interface queries refine the direction using the shared wave normal; full-mixture PDF weighting preserves the same Lambert/PBR response and optical energy. The cosine component retains support for every diffuse direction. Actual opaque first hits and unknown water disable the proposal; traversal records dynamic dependencies. `BLOXGLOOM_GI_CAUSTIC_GUIDE=0` disables guidance for a same-build comparison.

Independent GPU tests retain old/new angular means within measured Monte Carlo error and check normal-incidence transmitted directional irradiance plus constant-environment support. Solar variance falls from 2,294.10 to 17.63 at normal incidence, 264.25 to 1.411 at solar cosine 0.5, and 0.9259 to 0.004948 at cosine 0.1. A real submerged voxel-floor fixture reports blue means 0.01805/0.015384 (within its statistical uncertainty) and variance 0.046532/0.0017595, a 96.2% reduction. Actual first blockers/unknown state disable guidance and a genuine six-sided underwater enclosure stays exactly black. The pure fixture distributes samples across fragments after an excessively long single-fragment Metal invocation returned zero output. Raw logs and preserved binary/source hashes are in `/private/tmp/bloxgloom-water-caustic-evidence.json`.

This proposal reduces diffuse bottom-caustic variance; it does not guarantee stable 32-frame whole-scene output. Molecular scattering-to-sun paths, direct dielectric glints and other proposals can still contribute bright sparse samples. Fresh fixed-wave coast inspection and measured guide-query cost remain required; no radiance clamp, energy boost or visual acceptance claim is made here.

### Live dynamic poses and static transport history

Source review found an integrated convergence defect that static screenshots do not exercise: the first-person owner remains a secondary-ray target, its native idle/breathing joints change every presented frame, and any changed dynamic packet discarded the entire GI history. Idle players and spinning pickups could therefore keep otherwise static water and terrain at age one.

Transport now accumulates actor/drop-free lighting separately from an immediate signed correction: `current complete scene - matching static scene`. Both samples start from identical RNG state. The complete scene runs first; successful dynamic closest-hit queries mark dependencies, including ordered Sun visibility, camera-medium scattering and caustic-guide queries. Unaffected paths are identical to the static reference and need no replay. Affected paths replay with current dynamic traversal disabled; the complete path's RNG state is restored afterward. Current native joints, primitive animation, appearance, item transforms and secondary visibility remain exact. Admission/appearance and environment changes still invalidate history conservatively.

Generic static history stores incident lighting and in-scattering separately from additive retained-HDR/ambient cancellation; its existing receiver normalization still depends on the current material basis. Current retained ambient, specular fallback and HDR air extinction cancel at the exact full-resolution reconstruction pixel. Complete water already stores positive full-path radiance and cancels the current HDR baseline there. Signed dynamic corrections are neither temporally accumulated nor spatially denoised; they use the current compatible reconstruction samples. Primary actor samples carry a distinct negative age, bypass accumulation/filter mixing and cannot supply history when their old pixel exposes static terrain.

Actual production-GPU verification passed in 26.81 s: builtin GLB breathing changes current joints while most static receiver ages exceed nine; off-screen catalog pickup movement changes reflected correction at identical RNG; removal zeros it immediately; the paired static replay exactly matches the completely absent actor/drop scene's HDR sample at the same RNG; primary actor disocclusion rejects its old history; current Sun and camera-medium shadow dependencies retain negative correction/energy and restore exactly after caster removal. Seven temporal/filter/compositor tests pass (6.33 s), and the real primary alpha/water/fog fallback regression passes (17.61 s). Its fog check follows the new representation: current signed transmission carries extinction even when static incident radiance is zero. Logs are `/private/tmp/bloxgloom-paired-final-{gpu,primary}.log` and `/private/tmp/bloxgloom-paired-denoise.log`.

These checks establish current-target/history contracts, not finished image quality or cost. Dynamic corrections can remain noisy, primary moving actors are deliberately unaccumulated, and dependent paths cost an additional replay. Fresh complete-scene captures and measured frame cost remain required. Native builtin and authored player raster Sun casters preserve their world-body poses before camera framing. A confirmed primitive-owner caster omission is now fixed: camera-visible model prefixes and complete shadow-caster ranges share the same bounded current-pose instance buffer. An actual GPU regression verifies identical first/third-person primitive-owner caster depth, owner-only camera hiding, correct later moving-model ranges and immediate caster movement (0.98 s); native world-rig and public creature/rigid caster regressions also pass (0.68/0.66 s). Logs are `/private/tmp/bloxgloom-primitive-owner-gpu.log`, `/private/tmp/bloxgloom-native-caster-final-gpu.log` and `/private/tmp/bloxgloom-public-caster-final-gpu.log`.

### Guided production coast — visual acceptance

The frozen paired/guide release `88d5e4d27680546fd1f65968a0592c081438a087bcab4fc8dfdecfef4ac66d41` rendered guide on/off with the shared wave clock fixed at zero, quarter-size transport and 32 TAA samples. Both actual images were inspected: `/private/tmp/bloxgloom-guided-coast-world/02-coast.png` and `/private/tmp/bloxgloom-unguided-coast-world/02-coast.png`. Both retain mean water history age 31.618. The diffuse proposal improves the measured pool estimator but **still fails whole-scene visual acceptance**: cyan speckles, low-resolution blur and distant water/coast bands remain. Many large speckles survive at similar positions in both captures, motivating contribution attribution for molecular-scattering and other secondary water-connected paths before another sampling change. No optical coefficients, emitted energy or throughput clamps were changed.

### Water contribution attribution — diagnostic checkpoint

The fixed-wave guide-on/off coast captures still fail visual acceptance, so a compile-time diagnostic now partitions actual path radiance without changing the estimator. `BLOXGLOOM_GI_WATER_COMPONENT=0` keeps normal rendering; `1` retains contributions before any real water-medium collision; `2` retains contributions after such a collision. Both modes still evaluate every original random draw, visibility query, path weight and termination. Contributions include secondary environment/finite-disc Sun, dielectric Sun connections, opaque emission/direct light and air in-scatter; primary camera-air direct light belongs to the phase-free component. The default selector compiles out the diagnostic filter.

The complete production shader and isolated air helpers pass validation. A real submerged-pool GPU regression passes in 26.58 seconds: 256 identically seeded paths reproduce their original linear RGB as the sum of both partitions within `2e-5` relative/absolute tolerance, and the collision-tagged partition contains nonzero energy. Raw logs and frozen binary/source hashes are in `/private/tmp/bloxgloom-water-component-evidence.json`. This establishes trustworthy attribution, not improved water quality. Fixed-clock production component images remain pending; tone mapping/bloom means displayed PNGs cannot be added directly. No optical coefficients, emitted energy or proposal widths changed.

### Enhanced decorative Sun direction

The unchanged JG Sun artwork’s principal bright core is not at the center of its 512×512 image. Its independently measured 75%-peak linear-sRGB luminance-weighted centroid is `(227.6215, 216.7756)`. Centering the whole image on the directional light placed the visible core about **3.25 degrees away from the light**, compared with the traced physical solar disc’s 0.266-degree radius. Enhanced projection now anchors that core to the actual light direction; both reference variants preserve the original centered source projection. Texture pixels, quad extents, gain 6.5, solar irradiance and traced disc radius are unchanged.

The actual production sky HDR GPU regression passes four Sun directions, including the zenith/basis transition, at near/coast/32 km camera origins. Core error is below 0.75 pixels (approximately 0.11 observed), translated captures are identical, and both reference variants retain exact old pixels. The actual PNG checksum and independent centroid measurement protect the projection metadata; atmospheric parser and reference underwater SunGlare checks also pass. Evidence is `/private/tmp/bloxgloom-sun-core-*.log`. This corrects direction rather than caustic noise.

Visible decorative artwork, capped raster point-GGX and the physical traced disc remain different source representations. A read-only sRGB/quad quadrature finds decorative RGB irradiance `[0.0293444, 0.0305716, 0.0327890]` times directional solar channels; its peak luminance is 4.958 times solar, versus the physical disc’s 14,721.34 factor. The traced factor is correctly normalized to preserve total irradiance. The peak mismatch alone is not an overenergy/import defect, and copying BSL’s artistic highlight compression would change the enhanced transport model.

### Water interface consistency and remaining caustic families

Actual phase-attribution captures show bright cyan points in both phase-free and phase-hit water. A generated-coast GPU probe rules out the suspected hidden water-height offset: 248 real neighbor-aware near mesh triangle centers and 167 server-provided coarse wet spans share the integer ocean top at `y=17`; offsets of `±0.006` and `±0.0001` classify above the interface as dry and below it as wet. Enhanced waves change shading normals only, without moving these interfaces.

Guiding now also covers molecular Rayleigh events and the submerged opaque GGX branch. Each uses a 50/50 mixture with its complete original proposal, and divides the unchanged physical response by the full mixture PDF. Diffuse guidance remains. The proposal cone derives interface-normal compression from Snell's differential, `1 − cosAir / (IOR × cosWater)`, rather than using the uncompressed normal spread. Original phase/GGX support retains every tail; no absorption coefficient, BRDF energy, Sun flux, path budget or throughput clamp changed. `BLOXGLOOM_GI_PHASE_GUIDE=0` and `BLOXGLOOM_GI_GGX_GUIDE=0` provide family controls alongside the overall caustic-guide switch.

Independent angular GPU tests distribute 4.19 million samples per case across fragments, retain normalized phase and f64 hemispherical GGX energy oracles, and test normal transmitted-flux energy. At zenith, phase variance decreases `763.256 → 0.332557` and GGX variance `1.23390 → 0.000526337`; oblique/grazing solar cases also pass. An actual submerged-pool fixture reports roughly 94.8% lower phase variance and 99.0% lower GGX variance with means agreeing within Monte Carlo uncertainty. Real first blockers and unknown fluid state disable the proposal; genuine closed underwater rooms remain exactly black. A separate 12-case dielectric/conductor grazing regression preserves the pre-guide GGX direction-dependent ratio, including its existing denominator regularization.

An actual native-blocker GPU regression submits the production skin/BVH pass and proves both query families mark the dynamic dependency, disable current guidance, recover it in static replay and clear it exactly on actor removal. The full production shader validates. Logs and frozen hashes are in `/private/tmp/bloxgloom-water-guide-lobes-evidence.json`.

The extended-guide production release `fb8795694c17a75b0a396f89d21e0fbcbc5fd1e536822110694311237900edc5` captured `/private/tmp/bloxgloom-expanded-guide-world/02-coast.png` with the shared wave clock fixed at zero, quarter-size transport and 32 samples. Inspection still **fails visual acceptance**: cyan speckles and soft water reflections remain. Static complete-water history reaches mean age 31.618. The explicit prefilter HDR readback reports zero non-finite pixels, mean RGB `[0.06097, 0.08799, 0.13154]`, luminance p99/p99.9 `0.22712/1.08555`, maximum `26.54463`, and 13/2/0 samples above 1/10/100 times directional solar luminance. Its brightest samples are warm reflected Sun; a remaining blue sample is `[0.10852, 0.59766, 2.30078]`. These statistics cover accumulated static complete-water samples, excluding primary actor samples; they are neither post-filter/display values nor visual acceptance.

The first half-size 32-sample capture exceeded the headless tool's 30-second batch readback deadline. GI headless capture now permits a bounded 180-second batch wait, retaining 30 seconds for other captures. Live rendering and measured frame intervals do not use this wait. Higher-resolution acceptance and final runtime measurements remain separate requirements.

The longer-wait release `a86515e28a5a1b94b06102f6b556c04c0b263246d8c0722e91ba0bcd3d595ee9` completed `/private/tmp/bloxgloom-final-half-world/02-coast.png` with a black, zero-alpha readback and no valid static-water samples. It **fails execution and visual acceptance**. Kernel evidence in `/private/tmp/bloxgloom-half-gpu-system.log` records two hardware GPU restarts 2.016 seconds apart, followed by denial of further submissions for that application. A longer readback deadline does not fix this GPU failure. The same binary's default GI-off meadow/coast/grove/mountain images in `/private/tmp/bloxgloom-final-enhanced-world/` were inspected and render normally. Headless GI now rejects an entirely zero-alpha readback rather than saving it as a successful image. Bounded transport scheduling is being implemented and must prove identical output and a successful real coast capture; no integrator energy/bounce reduction is authorized as a substitute.

### Bounded submissions — integrated checkpoint

Experimental transport now submits globally aligned 64×64 scissor tiles in separate command buffers, preserving the full viewport, frame uniforms, RNG, water clock and all twelve path vertices. Four transport attachments clear once and retain contents between tiles; filtering/reconstruction follow after every tile. Actual production GPU comparison passes bit-for-bit for all four attachments, changing baseline HDR, filtered radiance and final composite across three frames with moving pickups and accumulated history. Odd-size coverage/derivative alignment and the live native-pose/history/removal/disocclusion regression pass. `BLOXGLOOM_GI_TILE=0` retains an untiled comparison for bounded fixtures. Transport timestamps begin on the first tile and end on the last; elapsed measurements include scheduling gaps.

Release `a819f613c5b76cef8d597b21a5e3262db1bc2a2afa5d6b009a2843304ff30303` completed the half-size coast image at `/private/tmp/bloxgloom-tiled-half-world/02-coast.png`. It is sharper than the quarter-size image but still **fails visual acceptance** because water remains visibly grainy. Its 69,550 static-water samples have mean history age 29.369, with 59,298 at age 32 and zero non-finite radiance samples. A full-window kernel review records **two** GPU restarts during this capture, each followed by a tiled-application clean-slate message; a valid final PNG does not establish execution safety. The earlier short log window missed these later events. Raw evidence is `/private/tmp/bloxgloom-tiled-{half-world.log,gpu-system.log,process-sample.txt}`. No successful safety or gameplay-ready full-GI claim is made.

Fresh 300-steady-frame GI-off measurements use the same frozen release and Apple M1 Pro/Metal adapter at 1280×720. No compiler or other GPU job overlaps these runs:

| Mode | CPU p50 / p95 (ms) | GPU p50 / p95 (ms) | Scene generation (ms) |
| --- | --- | --- | --- |
| Voxel, radius 6 | 10.846 / 12.242 | 12.193 / 13.297 | 5453.9 |
| Bounced, radius 6 | 10.830 / 12.348 | 12.025 / 13.566 | 5759.7 |
| Voxel + 512 m horizon | 11.988 / 13.618 | 13.373 / 14.587 | 5441.3 |

Visible near triangles remain 160,650/166,758; the horizon run draws 669,952 near+distant triangles, with 77 resident/28 selected tiles and 83,822,444 distant GPU bytes. Its summary/reduction/meshing setup is 1494.97/28.85/493.53 ms. Retained near worker/raster/ray payload sizes are unchanged from the complete-scene checkpoint. These normal-renderer costs remain comparable to the previous 300-frame measurements; they are not measurements of full GI or live gameplay. Logs are `/private/tmp/bloxgloom-tiled-perf-{voxel,bounced,lod}.log`.

### Controlled diffuse-floor regression after guide expansion

The first expanded-guide full suite finished **2,088 passed, one failed, 19 ignored in 570.64 seconds**. The older mixed pool control reported blue mean/variance `0.009430872 / 0.0034265148` versus guided `0.020784633 / 0.00029387695` (`n=1,024`); its mean assertion failed while variance reduction passed. There was one failure in the older pool fixture: its all-guides switch now changed diffuse, GGX and phase proposals together, while its statistical comparison was intended for the floor's diffuse component. The fixture now casts to the actual floor, decodes its real material, retains the physical reflected diffuse/Fresnel/metal allocation, and compares only original cosine versus guided diffuse sampling. Expanded downstream transport is identical in both controls. No production code or statistical tolerance changed. Independent normal-incidence transmitted-irradiance and phase/GGX angular energy oracles remain passing.

The corrected actual-pool test passes in 42.91 seconds: blue means `0.012472961 / 0.011603426` agree under the original five-standard-error assertion; variance decreases `0.033560593 → 0.00005274494`. Real first-blocker/unknown-state checks and the genuine sealed-room zero assertion are retained and pass. Log: `/private/tmp/bloxgloom-diffuse-guide-controlled-gpu.log`. This fixes a confounded regression control. A fresh complete-suite rerun remains pending; no whole-suite pass is claimed. The actual production coast still fails visual acceptance, and half-scale transport has triggered Metal GPU resets; bounded scheduling is being validated separately.

### Exact indexed medium traversal

The complete-scene medium directory now optionally appends a bounded 16-metre XZ candidate index. It preserves original tile order, shader-rounded bounds, vertical interval precedence, authoritative loaded-chunk overrides and unknown gaps. Known-distance queries avoid water classification when only coverage is needed. The existing storage layout and bindings remain unchanged; an oversized index or insufficient remaining storage retains the ordered scan, rather than rejecting a previously admissible scene. `BLOXGLOOM_GI_VOLUME_INDEX=0` selects the scan control.

Actual GPU comparison against the preserved original traversal passes **96,030** cases bit-for-bit, including mixed fine/coarse coverage, overlaps, vertical gaps, negative and distant origins, actual coast seams and unindexed fallback. A 77-tile, 256-ray shallow-water microbenchmark reports median **36.203 → 31.369 ms**; the timings vary substantially, so this is a modest local improvement, not a whole-frame speedup claim. Existing medium-state, shader-validation and exact worker-budget tests pass. Logs: `/private/tmp/bloxgloom-indexed-medium-{proof,existing-volume,worker-budget,budget,naga}.log`.

While relocating CPU tests into adjacent files, an existing untracked tiled GPU fixture was accidentally overwritten. Its original assertions were restored unchanged into `gpu/paired_tests/scheduling.rs`; the adjacent scheduling CPU tests remain separate. Compilation and coverage pass. The restored actual GPU fixture passes all four MRTs, baseline/filter/composite, signed dynamic correction and three-frame history assertions **bit-for-bit** in 91.97 seconds. This longer fixture completion time is not a frame-cost measurement. Log: `/private/tmp/bloxgloom-restored-tiled-proof.log`. The fresh complete suite and indexed production coast acceptance remain pending.

### Complete-scene correctness checkpoint

The fresh suite for the indexed-medium/tiled complete-scene checkpoint passes **2,094 tests, zero failures, 20 ignored in 581.81 seconds** (`/private/tmp/bloxgloom-indexed-full-tests.log`). Formatting, warnings-denied all-target/all-feature Clippy, and diff whitespace checks pass for that checkpoint. The inspected kernel log contains no GPU restart during this suite. This result predates the subsequent diagnostic history, guide-query ordering, and source-default AO/entity/wind/cloud comparison changes; those require their own checks and a final suite. It does not supersede the failed production water acceptance or earlier half-scale resets.

### Bounded indexed production coast acceptance

The next frozen release (`4c78ee7dad519dc614448820a13064015779e7bcf174b0cf4996d54848a11242`) completed a 32-sample, 640×400 transport / 1280×800 coast capture with one pending captured frame, indexed medium lookup and exact first-interface guide bounds. Every sample reported roughly **8.1–8.6 seconds** of completion wait; submit calls were 0.5–4.7 ms. These are diagnostic completion waits, not isolated GPU active-time measurements. The renderer is still unsuitable for gameplay. Final water history has 69,550 pixels, mean age 29.703, 60,519 pixels at age 32, and no non-finite radiance. Prefilter peak channel is 23.953125; five pixels exceed ten times solar luminance, none exceed one hundred times. These distributions establish convergence bookkeeping and finite values, not satisfactory image quality.

The actual image `/private/tmp/bloxgloom-bounded-indexed-half-world/02-coast.png` was inspected and still **fails visual acceptance**: cyan grain, coarse reconstruction and distant bright/tonal bands remain. The system log contains `Clean slate ... 1 GPURestarts in 0 submissions` at local 04:45:02 during the capture interval, but the broader log contains no actual restart-stamp event. This records restart state; the available evidence does not establish when the hardware restarted or prove a reset-free capture. Do not attribute safety to frame checkpoints, tiled commands, medium indexing or guide bounds independently. Logs: `/private/tmp/bloxgloom-bounded-indexed-half-world.log` and `/private/tmp/bloxgloom-bounded-indexed-half-gpu-broad.log`.

The complete-water radiance is reconstructed from filtered path samples, including textured bottom radiance. Only subtraction of the full-resolution raster baseline remains exact and unfiltered. This is not full-resolution preservation of traced bottom artwork. Longer static sample controls are opt-in diagnostics; ordinary live history retains its normal bounds. Source-default reference mode remains separate and disables path GI.

### Source-default reference input and consumer checkpoint

The checked-in BSL default remains an explicit `BLOXGLOOM_BSL_REFERENCE=1` branch; enhanced rendering stays the normal mode, and advanced artistic materials remain a separately named non-default comparison. The new reference work covers source AO, actor forward shading, botanical wind, source-class encoded-color emission, rain transfer/sqrt blending, selected-stack dynamic light, source celestial decoding/intensity and below-bedrock stars. Actual inventory supplies only declared placeable-block emission; completed voxel fields supply rain raw light. Iris-sign camera-minus-player-eye metadata fixes third-person held light and grass bending. Reference elapsed seconds no longer inherit the enhanced hourly wrap. Source equations and runtime engine proxies are distinguished in `BSL_REFERENCE_AUDIT.md`.

Focused actual GPU checks pass: AO and all three actor layouts; 72 rain helper rows plus real depth/overlap/sqrt composition; 64 handlight math rows plus actual 320-byte near/LOD camera packets at coast and ±32 km; authoritative inventory/sign and unknown-light cache guards; 384 wind cases for helper and production vertex against verbatim GLSL at the existing 1e−5 bound; 36 default-emission color/material rows with advanced/untagged-LOD isolation; 1,680 celestial/star source/f64 rows plus actual Sun/Moon, star and enhanced Sun-preservation fragments. Reference vertex/helper invocation can differ by one float32 ULP under Metal specialization; both independently meet the original GLSL oracle bound. Enhanced wind and pickup guards remain bit-exact. No production tolerance or material energy changed to accommodate this compiler detail.

The indexed first-interface guide query has 61,800 bit-exact GPU comparisons against its original ordering, including hit identity/distance/UV/normal, enabled/touched state and restored random state. Its isolated 77-tile/256-query alternating microbenchmark reports upper medians 7.543→7.065 ms with overlapping ranges; this is a local traversal result, not a whole-frame performance claim. Headless tile checkpoints retain exact four-MRT, baseline, filter/composite and history outputs across 32/64/128 tiles, groups 1/4 and six advancing frames/moving actors. They surface incomplete frames before composition/readback and never add live-window waits. The source reflected-cloud helper now uses actual camera altitude and the source shortened reflection fade, with independent source/f64 GPU cases. The 64-sample progressive history proof passes independently of the ordinary live 32-frame limit.

Both final mode captures from release SHA-256 `face9ffc09ed7727c73714280e0e3be03a4200312e320d86de0b4f4a0d3b737b` were rendered and inspected: `/private/tmp/bloxgloom-source-default-final-coast/02-coast.png` and `/private/tmp/bloxgloom-enhanced-final-coast/02-coast.png`. Reference water has clean source wave detail and the brighter source botanical treatment; enhanced retains its separate thin-sheet/PBR/cloud rendering. Both retain coarse distant shoreline bands; no matched Minecraft/Iris frame establishes whole-frame parity. These current code changes do not modify texture pixels, catalog numeric identities or saved worlds. A final combined suite is still pending at this documentation checkpoint.


The next frozen full-suite snapshot finished **2,120 passed, one failed, 21 ignored in 585.05 seconds** (`/private/tmp/bloxgloom-reference-final-full-tests.log`). The sole failure was shader parsing in the older underwater-sky-glare fixture: its handwritten source assembler omitted the new celestial helper. Production uses the complete assembler, and both mode coast captures succeeded. The fixture now calls that production assembler directly; its independent glare goldens are unchanged. A new full-suite run remains required after this correction and the subsequent generator-9/opt-in GI work. Formatting and warnings-denied Clippy passed for the earlier frozen source checkpoint; no current full-suite pass is inferred from those results.

### Continuous cold-region snow cover

The white stair bands on the distant coast appear in both rendering modes. The surface solver forces tile zero along every 32-metre region's four-metre perimeter, and the old Tundra palette maps that tile to snow. Terrain probes along the affected view directions are genuinely cold (temperatures roughly −0.51 to −0.84); they establish the biome context, not exact per-pixel hit materials.

Generator 9 replaces nonshore Tundra cover, including rocky cold ground, with absolute-coordinate 48/13-metre snow drifts, temperature/altitude/slope retention and separate 61-metre bare geology. It preserves authoritative shore treatment and other biomes' solver constraints. Snow is evaluated only for top voxels; subsurface rules remain unchanged. The default fresh save directory is `world-v34`; world metadata and LOD cache identities already distinguish generator revisions. There is no wire/storage-schema conversion or numeric content-ID change.

Five snow and five adjacent terrain tests pass. Negative 16/32-metre seams and exact near/LOD samples agree. Across three seeds, perimeter-versus-interior snow fractions differ by less than 0.007 and ordinary-versus-seam transition rates by less than 0.0012. All original composition/palette/direct block seam checks pass; four frozen surface hashes change in chunks with 255/256/256/143 nonshore Tundra columns, while five bedrock/sky hashes stay unchanged. The updated fingerprint regression passes in 14.83 seconds (`/private/tmp/bloxgloom-generator9-fingerprint-final.log`). Fresh generator-9 reference and enhanced coast images were inspected and still show the white bands. The snow correction removes an independent grid bias; it has not resolved the observed distant cliff artifact. An isolated before/after generation-cost comparison remains pending.

### Distant snow material extrusion — confirmed cause

A CPU diagnostic casts the actual coast camera's rays into production-selected LOD meshes, applying the same parent retirement and near-ready masking. Twenty affected/control pixels contain twelve snow hits: eight vertical sides and four top caps. The admitted render summary merges contiguous solid spans and repeatedly replaces their state with the highest span's material. Consequently one-voxel snow cover becomes a snow-colored solid column: pixel (500,230) hits a snow side at height 16.917 in a span extending from −64 to 17, behind the water plane at 17. Other affected snow sides extend through hills from −41 to 21. Dark gravel control hits remain dark. This is geometry/material evidence; it does not independently predict final shader RGB or enumerate nearer cutout occluders.

The diagnostic passes in 24.15 seconds (`/private/tmp/bloxgloom-generator9-coast-material.log`). A focused render-summary correction is in progress to preserve the top cap separately from the underlying solid material, without removing cave gaps or water boundaries. Summary budgets, mesh costs and new rendered coast output must pass before accepting the correction.

Preserving caps on every solid component fails the unchanged 60 KiB budget: only six of 77 coast tiles are admitted. Preserving only the highest solid component improves this to 35 of 77, with rejected tiles reaching about 71.6 KiB. Neither candidate is accepted. The latter also exposes a skylight regression from including the exposed cap's light in its buried body; the sealed/dark LOD assertion remains unchanged and must pass. A lossless tile-local state palette and packed four-bit sky/glow fields are being implemented to retain the surface skin within the original byte cap. Coordinate bounds, occupancy, unknown gaps, fluid boundaries and stable material identities remain exact; protocol versioning and cache invalidation must accompany the compact encoding.

The integrated compact candidate passes all 154 coast/origin admissions under the unchanged budget. It preserves one source-thickness cap on the highest opaque component, weighted buried material and opaque/cutout/fluid class boundaries. Buried lighting excludes the exposed cap; dark-cave and current disk-cache tests pass. Canonical packets use a sorted full-ID state palette, one- or two-byte indices and packed light nibbles. Wire version 35 rejects version 34; cache identity v5 discards earlier summaries. World save schema and content identities are unchanged.

The coast packets total 3,685,400 bytes, with a largest tile of 54,950 bytes; its mesh has 59,000,768 bytes and 1,177,974 triangles. Against the same generator-9 pre-fix coast, packet bytes increase 0.72% while mesh bytes decrease 0.22% and triangles decrease 0.21%. The origin scene's largest packet is 51,966 bytes and its mesh has 87,812,588 bytes/1,735,974 triangles; the earlier origin benchmark is generator 8 and is not a clean material-only comparison. The debug diagnostic's setup times are not release-performance measurements.

Protocol tests pass 49/49 and focused summary tests 6/6. The wider LOD group passes 60 tests with one obsolete budget-fixture assertion: its former four-layer input now fits without simplification. The fixture now uses genuine excess payload, preserving its exact body, gravel-cap and water assertions; the corrected executable regression now passes in 0.59 seconds (`/private/tmp/bloxgloom-compact-budget-fixture-final.log`). A fresh full suite remains pending. Warnings-denied Clippy passes for the integrated source. Release `2ab845e64d5641cc7686ec4a6d5ff977536cfc5375c0d10b2c51b72ed8677ed8` renders both `/private/tmp/bloxgloom-compact-caps-enhanced-coast/02-coast.png` and `/private/tmp/bloxgloom-compact-caps-reference-coast/02-coast.png`; both were inspected. Snow no longer paints full-depth cliffs, but bright source surface caps remain visible. No claim that every white band or overall visual quality is resolved follows from this fix.

### First-water lobe split — production host proof

`BLOXGLOOM_GI_WATER_LOBES=1` remains opt-in. It separates first-water reflection from complete static water radiance in an RGBA16 attachment, storing primary transmission in alpha; the normal path retains its R16 transmission attachment. The enabled real host pipeline passes its ten-frame history, signed current dynamic correction, full HDR and exact transmission-channel comparison against the default in 114.90 seconds. Native idle/off-screen pickup movement/removal and phase/GGX blocker replay/removal fixtures pass in 29.37 and 18.55 seconds after repairing the supplemental test bind group. Logs: `/private/tmp/bloxgloom-water-lobes-stage1-{host,paired,water-guidance}.log`.

The earlier 256-seed pool split preserves the complete radiance estimator, RNG and dependencies. These checks establish exact split infrastructure, not reduced production noise. Filtering and moment accumulation have not yet changed; their next experimental pass must use raw samples and the primary pass's exact validated history pixel and optical normal, rather than reconstructing them from half-float depth.

An actual M1 preflight for four RGBA16 attachments plus a three-layer RGBA32 storage target passes in 0.42 seconds. It checks exact float32 HDR/tiny/signed packets, history coordinates and normal metadata, scissor sentinels, subsequent sampled loads and next-frame invalidation against an explicit off-mode shader/layout. This proves the proposed raw-packet storage mechanism on this adapter; production reconstruction and its memory/time/quality acceptance remain pending. Log: `/private/tmp/bloxgloom-water-storage-gate.log`.

The new wire-version snapshot also passes a real nonblocking reactor/client test with authoritative chunk streaming and restart in an isolated temporary save (1.29 seconds, `/private/tmp/bloxgloom-wire35-real-listener.log`). The corrected full-suite rerun remains pending.

### Single-frame transport localization

The complete 512-metre origin scene has now been measured with a fixed 19.75-second wave clock, preload, half-resolution transport, 64-pixel tiles and one frame in flight. Five four-frame runs use the same frozen compact-cap release and no concurrent compiler/GPU workload:

| Query control | Transport median (ms) | Whole-frame GPU median (ms) |
| --- | ---: | ---: |
| Baseline first | 9,623.867 | 9,654.675 |
| Known-space certificate | 9,802.589 | 9,860.247 |
| LOD root ordering | 10,161.386 | 10,218.923 |
| Both | 10,227.087 | 10,293.999 |
| Baseline repeat | 9,644.320 | 9,663.343 |

The bracketed baseline drifts by less than 0.22%; none of the candidate runs establishes a speedup. Both switches remain off. This is a small comparison, not a tail-latency or gameplay benchmark. Transport measures the first-to-last tiled GPU span. Filter/composite timestamp starts overlap the preceding transport end on this backend; those spans are not exclusive costs and must not be added. Initial candidate setup takes 66.9–70.3 seconds versus about 0.85 seconds for cached baselines, outside frame timing; the evidence does not isolate the reason. Exact sanitized environments, commands, SHA and scene metadata are in `/private/tmp/bloxgloom-compact-query-perf-{baseline-a,certificate,roots,combined,baseline-b}.log`, with `/private/tmp/bloxgloom-compact-query-perf-summary.json` collecting results. These origin timings do not replace the distinct coast-camera measurement.

The generator-9 release (`aca0b7a03914aeacaa74f6f6df38572f230cdacacabb9cbd0b338006ec7992e2`) completed a one-sample coast diagnostic with 640×400 transport, 64-pixel tiles and one tile in flight. Seventy completion waits total 8,433.085 ms; the largest is 208.061 ms. The resulting image still has severe expected single-sample water noise and is not a quality acceptance capture. These waits include scheduling/driver overhead and do not isolate active shader cost. The narrow kernel log for this one capture contains no event; this does not establish reset-free longer captures or gameplay safety. Logs: `/private/tmp/bloxgloom-gen9-tile-localization.log` and `/private/tmp/bloxgloom-gen9-tile-localization-kernel.log`.

### Raw water reconstruction — experimental, acceptance pending

`BLOXGLOOM_GI_WATER_RECONSTRUCTION=1` adds a separately admitted experiment. It captures pre-temporal total/reflected radiance, the actual mapped normal, roughness and exact accepted primary history coordinate in a three-layer float32 storage target. Float32 reflection/remainder luminance moments reuse that coordinate; unsupported history restarts confidence. Reflection and transmission are reconstructed independently, with mapped-normal/camera-direction support for reflection and conservative 3×3 transmission support. Signed current dynamic correction and exact raster-baseline cancellation remain outside this filter. There are no additional rays or radiance clamps.

The raw and ping-pong moment/guide images add 112 bytes per transport pixel (28,672,000 bytes at 640×400), before existing transport/filter outputs. Admission requires fragment storage writes, the actual float32 usage flags and resource limits; allocation is bounded to 256 MiB. Unsupported adapters retain the split-only path. Ordinary GI has no added storage binding or images. `BLOXGLOOM_GI_WATER_FILTER=0` provides the old-filter control within this experiment. Shader parsing and the actual three-mode host/moment oracle pass; cached-filter visual comparison, final suite and performance acceptance are still pending. This is not a recommended gameplay setting.

A separate test-only secondary-path counter probe uses the exact scalar production assembler, real coast near/LOD admission and private per-invocation counters. Actual GPU validation passes in 187.24 seconds, preserving HDR bit-for-bit and all final RNG bits via two exactly representable 16-bit halves. Its 32 directions × eight seeds contain 269,094 near triangles and 822,642 selected LOD triangles across 60 tiles/two pages. Totals for node fetches, triangle tests, known-space steps, cloud proposals and entered path vertices are `[163208, 37828, 752, 476, 613]`; maxima are `[2326, 621, 34, 20, 6]`. The test duration includes material/scene preparation and pipeline compilation, not a frame-cost measurement. The probe does not cover raster-primary attachments, conditional first-water splitting, paired replay, temporal reconstruction or whole-frame timing. Consequently it cannot attribute the separate nine-second origin frame cost. Log: `/private/tmp/bloxgloom-real-coast-cost-counters.log`. Lossless vector LOD record loads are also under development as an explicit off-by-default experiment; fewer source loads alone are not a measured speedup.

The first three-mode raw/moment host run stops at 397.91 seconds on a fixture history-coordinate assertion. The fixture expected uninterrupted history when it introduced an actor at frame three; actual topology/appearance admission correctly invalidates it. The repaired oracle independently checks invalid frames 0/3/7, valid moving frames 4–6 and original primary history ages. Existing raw/moment/guide tolerances remain unchanged. Supplementary single-output Metal pipeline compilation accounted for much of the first run's wait; it is not an isolated production startup measurement. The revised independent probe retains the original four-output entrypoint, disables history through its runtime uniform and bounds GPU readback.

The repaired host passes in 299.34 seconds (`/private/tmp/bloxgloom-water-runtime-oracle-host.log`). Ten real frames compare default, split and raw-enabled modes: original static means, geometric age, primary transmission, signed current correction and final HDR retain the original bounds. Topology invalidations and advancing movement history follow the explicit admission oracle. At frames zero and nine, independent unaccumulated float32 total/reflection probes and actual mapped-guide probes pass the unchanged 2e−5/1e−5 bounds. CPU moments consume genuine current samples and exact accepted history pixels; at least twelve pixels distinguish those raw samples from already accumulated means. This establishes the data needed by the new filter, not visual acceptance of its reconstruction.

### Vector LOD loads — opt-in with explicit scalar fallback

`BLOXGLOOM_GI_LOD_VECTOR_LOAD=1` is an off-by-default record-load experiment. Default scalar shader bytes and resource layouts remain unchanged. Its actual GPU node/triangle decoding proof passes exact CPU bit oracles for all four pages, high-bit unsigned metadata and padding (0.85 seconds, `/private/tmp/bloxgloom-vector-record-proof.log`). The first fixture exposed a Metal packed-float3-to-uint3 cast error in the test decoder; widening to a four-component vector fixes that fixture without changing production decoding or expected bits.

The native ordered-root variant fails one query: scalar returns a one-metre page-two hit from `(3,0,0)` along +X, while the vector variant misses. Direct triangle, fixed-order casts and isolated ordered casts agree; this context-sensitive interaction has no confirmed compiler-cause diagnosis. The implementation now explicitly returns the unchanged scalar shader whenever vector loads and root ordering are requested together, with a once-only warning. Native root-ordering vector execution remains rejected. Log: `/private/tmp/bloxgloom-vector-hit-proof.log`.

The strict actual GPU hit matrix passes in 1.66 seconds across native fixed-order vector loads and the explicit ordered-root scalar fallback. It covers all four pages, masked/unmasked authority, alpha decisions, ties and positive/negative large coordinates. Its empty-page root probe now returns an explicit empty node rather than comparing undefined out-of-bounds behavior; expected valid hit words remain exact. A separate complete paired transport proof passes in 216.47 seconds, comparing all four attachments, filtering, signed moving-dynamic corrections, history, final HDR and all RNG bits across three frames with observable distant emissive geometry. The substantial test wall time includes Metal compilation; it is not frame performance. Logs: `/private/tmp/bloxgloom-vector-fallback-matrix-final.log` and `/private/tmp/bloxgloom-vector-paired-final.log`.

Three serial four-steady-frame origin/512 m controls on frozen release `f3f69e3d7ddbac7e9fbf559000499840cc627136b3bfc5576987feab777a9a3f` measure scalar/vector/scalar transport medians of **9,651.689 / 5,261.494 / 9,646.456 ms**, and whole GPU medians of **9,686.695 / 5,292.777 / 9,740.269 ms**. This is a 45.47% transport reduction against the bracket mean, with scalar drift −0.054%; whole GPU reduction is 45.51%, with +0.553% scalar drift. Every run uses the same 77 admitted/28 selected tiles, 1,735,974 distant triangles, 171,612,576 ray bytes and 496 resident near chunks. No competing compiler/GPU workload runs during the measurements. Frame completion interval is one and tile-group checkpoints zero; fixed waves, scale two, profile/preload on, root/certificate/lobes/raw off are identical.

Submit-side CPU medians vary 8.342 / 9.997 / 16.213 ms; they exclude GPU completion and do not show a CPU speedup. BVH setup is separately 835.7 / 817.8 / 814.0 ms. Transport timestamp spans include queued tiles/idle gaps and overlap filter/composite ranges. This small origin comparison establishes a benefit, not tail latency, gameplay affordability, coast quality or raw/vector combined acceptance. The option remains off by default. Exact environments and results are in `/private/tmp/bloxgloom-vector-perf-{scalar-a,vector,scalar-b}.log` and `/private/tmp/bloxgloom-vector-perf-summary.json`.

### Certified-empty actor transport — scoped experiment

`BLOXGLOOM_GI_EMPTY_DYNAMIC=1` prepares a second transport pipeline with unreachable actor casts/material dispatch/replay removed at compile time. The original generic pipeline and all resource bindings remain. Selection requires the actual current uploaded dynamic node count to be zero; admitted actors immediately choose generic, while pending/rejected/removed geometry clears that header. Existing history/admission rules remain unchanged. This experiment is restricted to the ordinary R16 transmission mode; split/raw reconstruction requests retain generic transport with a warning. It adds pipeline compilation/storage when requested and is off by default.

Independent review finds no certificate/dispatch defect. Source/default-byte, scope-gate and all-mode parser checks pass. The actual R16/default GPU proof passes in 67.47 seconds: nine phases compare all four attachments, baseline, filtering and final HDR bit-for-bit across empty/history, pending upload, admitted/moving actor, removal, re-admission and removal. A separate final empty submission preserves all 32 RNG bits and zero dynamic-dependency flags; a nonzero signed correction proves the generic actor fallback is exercised. Log: `/private/tmp/bloxgloom-empty-dynamic-proof.log`, with exact sanitized environment and frozen test hash `5ca478b262d7caa4d863f89dc21826b1e32e1c2f0c1ccdf72332ae59b7576bdf`. This duration includes setup/compilation, not frame cost. Adapter-limit rejection is reviewed through existing header-clearing logic, not claimed as a submitted phase in the new fixture. The sparse primary counter harness explicitly rejects this setting because it instruments generic source. Bracketed release performance remains pending.

### Cached water filter comparisons

The optical filter accumulates weighted differences from the center sample with the same normalized weights, preserving constant HDR regions without changing support or clamping radiance. Its actual GPU fixture passes in 1.49 seconds, retaining strict 1e−5 mapped-normal edge and 2e−5 family/known-mean bounds, including radiance above 14,000. This repairs the original constant-region rounding failure (100 became 100.000015); the assertion was not relaxed.

`BLOXGLOOM_GI_WATER_FILTER_DIAGNOSTICS=1` exports six float32 views from one immutable history: legacy/optical × combined/reflection/remainder, plus the guide and `metrics.json`. It adds no transport rays or changed seeds/poses. The low-resolution diagnostic PNGs use a fixed HDR display mapping and omit signed current correction; they are not production postprocessed frames. Optical family views retain combined fallback outside valid first-water guides; family metrics use the valid guide mask. Neighbor differences measure a grain proxy, not Monte Carlo variance. On enhanced static coast/water previews with TAA off and no posttrace particles, `production-legacy.png` and `production-optical.png` additionally use the same current correction, baseline cancellation and unchanged production bloom/exposure/display pipeline.

Frozen release `f3f69e3d7ddbac7e9fbf559000499840cc627136b3bfc5576987feab777a9a3f` completed one 32-sample, half-resolution coast capture with fixed 19.75-second waves, TAA off, reconstruction on and vector/root/certificate controls off. All six diagnostic maps and both production frames were independently inspected at `/private/tmp/bloxgloom-cached-optical-32-coast/02-coast-water-filters/`. **Visual acceptance fails:** conspicuous cyan grain remains; optical output is only slightly sharper. This is not an accepted quality improvement or a recommended default. All 69,470 complete-water samples reach age 32; 69,467 guides are supported, with zero nonfinite values.

Supported mean RGB changes by less than 0.052%. Family recombination max absolute error is 4.04e−6 for legacy and 1.32e−6 for optical. Neighbor squared-difference proxies increase by 14.4% combined, 14.2% reflection and 42.7% remainder; reflected structure dominates the global measure, which also includes legitimate Sun glints. Peaks increase from 23.019 to 23.604 rather than being suppressed. Raw per-family statistics and region-specific analysis must separate structure from noise before another reconstruction claim. Log: `/private/tmp/bloxgloom-cached-optical-32-coast.log`; exact environment and actual binary hash are stored with the capture.

### Actual primary transport counters

Test-only instrumentation reuses the actual raster-primary resources and production shader on 64 selected derivative-aligned quads. It counts near/distant/dynamic traversal, material/alpha, ordinary medium-directory lookup, cloud density/shadow quadrature, conditional first-water paths, camera medium, guide geometry and dynamic replay. Release builds include no counter hook or resources. The first coast run stops on a one-ULP unquantized HDR difference (0.12127799 versus 0.12127798); RNG halves, classification and history controls agree. That failure is preserved in `/private/tmp/bloxgloom-real-coast-primary-counters.log`, and does not prove a compiler defect.

The revised probe exports all raw float32 evidence and ULP differences, then compares the production binary16 storage values strictly, alongside exact float32 RNG/class/history controls. An independent GPU binary16 boundary/tie/subnormal oracle checks that storage contract. It changes neither production formats nor shading and does not claim raw float32 bit identity.

The coast probe passes in 314.34 seconds, including both independent 16-case storage oracles. Raw HDR differs by at most 9/10/7 ULP in RGB (relative error below 7.8e−7), and transmission by at most 49 ULP/3.65e−6; all stored values agree exactly. Raw geometry, classification, age, RNG and current correction agree exactly. Evidence, including both complete shaders and raw/stored packets, is retained at `/var/folders/66/mh6l6w6j5kx99pv48cdwxrjm0000gn/T/bloxgloom-primary-packets-85839-1791284194813096000`; log: `/private/tmp/bloxgloom-real-coast-primary-stored-counters.log`.

The 64 sparse coast rays cover 16 complete-water, 16 media-only, 15 unknown and 17 opaque samples, with no actors or dynamic replay. Totals in the fifteen-counter order below are `[37654,7569,309,453,167,705,2848,2132,111,195,64,64,16,49,0]`: about 588 node fetches, 118 triangle tests, 33 cloud-density calls, 11 medium-region calls and 45 directory iterations per ray. Density calls include cheap early returns outside the cloud-height slab; they do not each imply a full noise evaluation.

The preloaded radius-six/512 m origin probe passes in 139.61 seconds. Both binary16 oracles pass, and every raw float32 HDR/geometry/transmission/current/RNG/class/age value is also bit-identical. All 64 sampled primaries are opaque. Totals are `[76289,13631,74,375,204,583,3592,626,304,115,64,64,0,0,0]`: about 1,192 nodes and 213 triangle tests per ray, with a maximum of 3,010 nodes. This sample contains no primary water split, guide query, actor, media-only primary or dynamic replay. Evidence: `/var/folders/66/mh6l6w6j5kx99pv48cdwxrjm0000gn/T/bloxgloom-primary-packets-86601-1791284352372991000`; log: `/private/tmp/bloxgloom-real-origin-primary-stored-counters.log`.

Counter order is BVH nodes, triangle tests, known-space DDA steps, cloud delta proposals, path vertices, medium-region calls, directory iterations, cloud-density calls, surface material calls, alpha evaluations, primary lighting calls, camera-medium calls, primary water splits, guide geometry queries and static dynamic-replay calls. Neither sparse first-frame probe measures exclusive execution cost, convergence or visual quality. Their rendered outputs and printed benchmark timings bypass full-frame transport and are intentionally invalid as GI performance results. Release optimization measurements remain pending.

### Generator-9 compact-cap default performance

Three serial 300-steady-frame runs of the frozen compact-cap release `2ab845e64d5641cc7686ec4a6d5ff977536cfc5375c0d10b2c51b72ed8677ed8` use M1 Pro/Metal, 1280×720, radius six, AA/GI/reference mode off, the ordinary upload ramp and no competing compiler, benchmark or GPU fixture. They validate the current generator-9/default renderer checkpoint, not opt-in water reconstruction or vector loads.

| Scene | CPU steady p50/p95 (ms) | GPU steady p50/p95 (ms) | Near setup (ms) |
| --- | ---: | ---: | ---: |
| Voxel | 10.878 / 12.237 | 12.276 / 13.370 | 5,413.8 |
| Bounced | 10.869 / 12.434 | 12.143 / 13.411 | 5,762.1 |
| Voxel, 512 m horizon | 12.087 / 13.822 | 13.499 / 14.683 | 5,442.3 |

Near visible triangles remain 160,650/166,758. Voxel retained worker payload is 61,150,260 bytes, comprising 35,371,104 raster bytes and 25,779,156 CPU ray bytes; bounced payload is 63,489,540 bytes. The LOD scene admits 77/77 tiles and selects 28; distant mesh storage is 87,812,588 bytes with 1,735,974 generated triangles and 692,496 visible near+distant triangles. Separate summary/reduction/meshing setup is 1,513.23/34.60/641.33 ms, outside frame timing.

The earlier generator-8 512 m checkpoint reports 83,822,444 distant bytes, 1,658,962 generated triangles, 669,952 visible triangles and 1,494.97/28.85/493.53 ms setup. Current horizon geometry and mesh setup cost more; this comparison includes both generator and cap changes and cannot isolate either cause. Median GPU time changes from 13.373 to 13.499 ms in these single runs. CPU submission excludes GPU completion/presentation, and GPU timestamps exclude CPU staging/upload copies. Logs: `/private/tmp/bloxgloom-compact-default-perf-{voxel,bounced,lod}.log`. These normal-renderer measurements do not improve or supersede the roughly nine-second complete-GI result.


## Current priority: MacBook raster renderer (October 6)

Full path-traced GI is deprioritized. Preserve the opt-in actor-free experiment,
with its existing scope and acceptance limits. Finish these three tasks in the
normal GI-off renderer:

1. Saved Performance/Balanced/Quality presets, a truthful Custom state, and world
   render resolution independent of native UI resolution. Preserve old settings
   and player identity. Validate the actual Retina workload, rather than infer
   live performance from the historical 1280×720 headless benchmark.
2. Remove distracting coarse distant terrain steps and ocean tonal seams while
   preserving shorelines, real cliffs, cave mouths, tree support, and residency
   coverage. Keep added mesh/setup cost measured separately from frame cost.
3. Correct raster-water grain and depth response. Keep wave/shading normals
   separate from geometric receiver planes; account for half-float depth
   precision. Use consistent near/distant water absorption so deep coarse beds
   do not remain as transparent as shallow banks.

The local M1 Pro has a 3456×2234 display (8.38 times 720p pixels). At task start,
its saved config used fullscreen, high 4096-pixel sun shadows, view radius six,
60-step parallax out to 80 metres, and bounced voxel lighting. Full GI is a
separate environment opt-in and is disabled for this task's measurements. The
user confirms an M1 Pro release build at about 12 FPS. Resolution/settings are
concrete performance leads; final isolated measurements are recorded below.

Initial raster-water regression: the old mapped-normal plane rejection loses
constant reflected radiance in 2,641 of 6,144 distant rippled-water pixels. The
corrected geometric-plane/precision-aware reconstruction retains all 6,144
within the existing half-float output bound. This addresses raster SSR holes,
not the stochastic variance of the deferred complete-GI experiment.

The goal service refused to create this new goal because the previous paused
full-overhaul goal is unfinished. These priorities are tracked here without
falsely marking that older objective complete. An initial Retina benchmark was
terminated when a separate user release build started; its partial output is
not accepted performance evidence.

### Implemented presets and raster corrections

Choose **Settings → Graphics → Quality preset → MacBook** for the M1 Pro test.
Existing configurations remain Custom until a preset is selected; changing a
budget manually reports Custom again. Config writes stay on the existing worker.
Preset changes preserve profile/save identity, controls, exposure, artwork depth,
and color grading. Full path-traced GI is not enabled by any preset.

| Preset | World scale / Retina extent | Near radius / distant horizon | Main budgets |
| --- | --- | --- | --- |
| MacBook (`performance` in config) | 35% / 1210×782 | 3 / 512 m, coarse | 1024px Sun shadows; POM, SSR, local shadows and bloom off |
| Balanced | 50% / 1728×1117 | 4 / 512 m, medium | 2048px Sun shadows; 16-step POM within 20 m; SSR; one 256px local map; bloom |
| Quality | 100% / 3456×2234 | 6 / 512 m, fine | 4096px Sun shadows; 32-step POM within 32 m; SSR; two 512px local maps; bloom |

All bundles use ordinary voxel lighting. World depth, temporal history, water,
sky and camera projections follow the world extent; menus/HUD remain native.
The BSL reference presentation now correctly upscales the whole viewport.
Block outlines are drawn in HDR before display upscaling, so they also receive
tone mapping/bloom rather than remaining a display-space overlay.

Distant dry natural terrain reconstructs gentle shared corners in the upper
solid shell at coarse levels. Unknown/mixed-level seams, wet shores, constructed
blocks, tree roots, thin cave roofs and real cliffs retain voxel geometry.
Underground geometry below the deformation depth remains unchanged. The final
77-tile coast fixture grows from 58,952,304 to 59,290,856 mesh bytes (+0.574%) and
from 1,174,764 to 1,175,350 opaque triangles (+0.0499%); water triangles remain
2,278. Near terrain is still voxel-shaped.

Water reflections now intersect geometric water-face planes instead of wave
shading normals and allow the projected binary16 distance error. Distant fluid
color uses RGB9E5 without increasing the 20-byte vertex stride; phantom
translucent faces at unknown residency frontiers no longer double-blend. Near
and distant enhanced water share depth-dependent RGB absorption using the
opaque background/depth snapshot. This adds 12 bytes per world pixel and one
fullscreen snapshot pass, with no additional rays. BSL reference water retains
its separate treatment; unsupported GL and underwater-exit cases retain the
legacy fallback. Multiple overlapping water layers retain existing draw-order
limitations.

Actual coast material probes attribute the remaining shallow green/blue patches
to generated grass, moss and coarse dirt under different water depths, including
near geometry, rather than a LOD color-boundary defect. These corrections do not
remove intentional wave highlights, visible shallow bed steps, or the deferred
path-traced water variance. No texture artwork, world generator, save format or
network protocol changes are included.

### Isolated M1 Pro measurements

The user kept the game closed. Final preset runs use frozen release SHA256
`b6bc1d293f478bee5af157b0bc497a572edb0790f5e2f883e005a4c46abbe4ee`,
3456×2234 native output, TAA on, full GI explicitly off, and 300 steady frames
after upload ramp. CLI radius/horizon match each preset. The starting-config
control uses the earlier frozen raster release and saved starting settings;
this is a settings/renderer comparison, not an isolated shader optimization.

| Settings | CPU submission p50 / p95 (ms) | GPU p50 / p95 (ms) | GPU frame capacity |
| --- | ---: | ---: | ---: |
| Starting settings, native, radius 6, bounced | 84.976 / 92.322 | 98.332 / 105.402 | 10.2 FPS |
| MacBook, radius 3, 512 m | 8.049 / 10.386 | 15.373 / 16.400 | 65.0 FPS |
| Balanced, radius 4, 512 m | 22.839 / 26.118 | 25.775 / 27.550 | 38.8 FPS |
| Quality, radius 6, 512 m | 88.772 / 92.556 | 103.204 / 106.376 | 9.7 FPS |

MacBook trades world sharpness and effects for roughly 6.4× lower GPU frame
time. Quality remains unsuitable for native Retina M1 Pro performance; it is a
comparison preset for faster hardware/lower output resolutions. These are
headless rendering measurements, including Sun shadows and final HUD but
excluding presentation, live server/network/gameplay and local shadow-map
passes. They are not a promised live FPS. CPU submission excludes GPU completion.
Scene setup/mesh storage are separate from frame timing. Exact logs are
`/private/tmp/bloxgloom-raster-retina-custom.log` and
`/private/tmp/bloxgloom-macbook-final-{performance,balanced,quality}.log`.

Final fixed-wave GI-off previews were inspected at
`/private/tmp/bloxgloom-macbook-final-previews/` (meadow, coast, cherry grove,
mountains). The matched terrain-shaping-off control is at
`/private/tmp/bloxgloom-macbook-terrain-control/04-mountains.png`. The native-output
MacBook benchmark capture is
`/private/tmp/bloxgloom-macbook-final-performance.png`; its canned test HUD FPS
is not a measurement. Live visual/performance acceptance remains the user's
play-test.

The standard 1280×720, TAA/GI-off `perf 300 6` regressions also complete on the
same final release. Ordinary voxel GPU p50/p95 is 11.977/12.746 ms and bounced
is 12.017/12.836 ms, versus the preceding checkpoint's 12.276/13.370 and
12.143/13.411 ms. Near worker payloads are unchanged at 61,150,260/63,489,540
bytes, with 160,650/166,758 visible triangles. Setup is separately
5,765.4/5,786.0 ms. These single-run comparisons establish no performance
regression in this fixture, not an isolated water or LOD speedup (horizon is
zero). Logs: `/private/tmp/bloxgloom-macbook-final-{voxel,bounced}.log`.

Native graphics-menu previews were generated and inspected at 1280×720 and
640×360 in `/private/tmp/bloxgloom-macbook-final-ui/`. Small windows scroll;
pointer/tab/scroll regressions exercise the newly added controls.

### Verification and completion

Implementation of the three raster priorities is complete; live play-test
acceptance is pending. Full GI remains deprioritized, and the previously paused
overhaul goal remains separate.

`cargo test -- --test-threads=1` completed in 1,778.85 seconds with 2,172 passing,
three failing and 28 intentionally ignored tests. All three failures were the
same obsolete angular-consumer shader fixture: it constructed the production
LOD vertex with a floating-point color after that input became packed `u32`.
The fixture now supplies packed white (`0xffffffffu`); its independent lighting,
foliage-motion and dark-shelter assertions are unchanged. The targeted rerun of
all three passes in 1.16 seconds. Thus all 2,175 non-ignored tests have passing
evidence; the entire long suite was not repeated after this test-only repair.
Logs: `/private/tmp/bloxgloom-macbook-final-tests.log` and
`/private/tmp/bloxgloom-macbook-angular-rerun.log`.

Final `cargo fmt --all -- --check`,
`cargo clippy --all-targets --all-features -- -D warnings`, shader/GPU
regressions, isolated release benchmarks and `graphify update .` pass. Release
production code did not change after the frozen measurements. The user's
existing `.gitignore` edits are excluded from the integration commit.
