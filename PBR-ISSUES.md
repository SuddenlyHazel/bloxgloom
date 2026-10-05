# PBR material processing issues

The October 5, 2026 audit compared 350 imported JG RTX materials, their source pixels, Bloxgloom's material pipeline, and the BSL reference in `external/shaders`. It found import and filtering defects as well as rendering capability gaps. Predicted visual effects below require gameplay verification.

## Work and approval sequence

1. Complete and committed: corrected normal orientation, source material encoding, and categorical material filtering, with automated checks and performance comparisons.
2. The user completed the live test and approved continuing on October 5, 2026, reporting a modest improvement but unsatisfactory overall appearance.
3. Complete: corrected roughness conversion and albedo color-space handling, reimported assets, and validated source pixels and runtime filtering. The user reported unsatisfactory cherry bark appearance after stage 2; the follow-up investigation is recorded below. Broader visual acceptance remains pending.

Height reconstruction, parallax self-shadowing, and scene reflections are documented here but are outside the two authorized implementation stages.

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

## Rendering capabilities outside this fix

Only 18 of the 350 imported normal maps contained variable height. Most materials therefore have no parallax relief even with parallax enabled. This is primarily absent source height data, rather than discarded imported height. Bedrock conversions without authored height also use flat AO and height; reconstruction from normals is a separate quality improvement.

Bloxgloom currently offsets texture coordinates without height self-shadowing. Its specular environment uses an analytic sky and approximate local transport rather than reflecting scene geometry. BSL has parallax self-shadowing and screen-space scene reflections when advanced materials are enabled. Its checked-in default disables advanced materials, so those features must not be assumed active in every BSL screenshot.

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

The original gameplay-scene cherry preview was regenerated after the sky correction as an additional comparison to the user-confirmed reproduction. The remaining mottled bark is a source-art limitation, not evidence of a swapped normal or metallic wood encoding.


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
