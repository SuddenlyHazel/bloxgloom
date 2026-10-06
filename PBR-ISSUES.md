# PBR material processing issues

The October 5, 2026 audit compared 350 imported JG RTX materials, their source pixels, Bloxgloom's material pipeline, and the BSL reference in `external/shaders`. It found import and filtering defects as well as rendering capability gaps. Predicted visual effects below require gameplay verification.

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

Terrain generator version 7 adds continental coasts, connected ridges, eroded plateaus, directional dunes, contour drainage, exposed rock/snow and regional geology. Regional tree species share stable biome/save tags but have distinct branched crowns and axis-correct branch logs; vegetation forms meadow/forest and flower patches. Column sampling is cached. Neighboring chunks and LOD sample the same deterministic generator. The default save target advances to `world-v32/`; existing saves are untouched.

New leaf cards retain the established face-shell triangle budget, overlap to prevent crown gaps, and share runtime wind with sun/local shadow casters and scene rays. Botanical flags exclude unrelated cutouts; tall plants share an anchored, continuous lower/upper wind coordinate. The transmission model adds bounded directional forward scattering. Reproducible artwork curation changes three bark albedos, caps dry botanical smoothness in 55 specular maps, and regenerates three icon entries. Provenance identifies adapted source textures; the original source pack remains unchanged.

New scene transport builds a triangle BVH on workers, rejects stale revisions after edits/removal, and samples diffuse/GGX paths with emissive surfaces, sun visibility, Russian roulette, and participating media. Loaded opaque/cutout geometry can contribute outside screen bounds. Primary camera extinction/scattering replaces analytic fog when the scene is ready; unsupported/oversized adapters and pending scene revisions use the existing voxel/SSR/atmosphere fallback. Finite sampling and reconstruction are still being validated. Do not interpret compilation as visual or performance acceptance.

The sky now has a true 3D cloud volume, dedicated rain and moon-phase inputs, and unchanged JG RTX sun/moon artwork. BSL tone/sky/star equations have source-based regression checks. **Exact whole-frame BSL parity remains unproven**: the reference disables advanced materials, while the requested enhanced renderer includes PBR/path tracing/new cloud density. The source equations, runtime differences and required matched reference capture are recorded in `BSL_REFERENCE_AUDIT.md`. Final image/performance evidence will be appended after validation.
