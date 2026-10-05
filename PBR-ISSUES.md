# PBR material processing issues

The October 5, 2026 audit compared 350 imported JG RTX materials, their source pixels, Bloxgloom's material pipeline, and the BSL reference in `external/shaders`. It found import and filtering defects as well as rendering capability gaps. Predicted visual effects below require gameplay verification.

## Work and approval sequence

1. Complete and committed: corrected normal orientation, source material encoding, and categorical material filtering, with automated checks and performance comparisons.
2. Paused for the user's live test. Do not begin roughness or color-space changes until the user approves continuing.
3. Pending approval: correct roughness conversion and albedo color-space handling.

Height reconstruction, parallax self-shadowing, and scene reflections are documented here but are outside the two authorized implementation stages.

## Normal orientation

The original importer flipped every Java normal map's green channel. Among comparable non-flat Java and Bedrock maps, 118 pairs had identical XY data, nine had reversed green, and 27 differed otherwise. Most Java maps were already DirectX normals, the convention our derivative UV frame uses. A universal flip therefore made their lighting disagree with surface relief. A universal removal would also miss the source pack's exceptions.

Use canonical Bedrock normal maps resolved through the pack's texture sets and edition name mapping where available. Preserve the authored Java AO and height channels. Decode remaining Java maps as LabPBR DirectX data, recording exceptions and fallbacks. Resize data channels independently; alpha represents height or emission, not image transparency.

During the fix, the shader's UV frame also proved invalid when a crossed plant's upward shading normal lies inside its vertical card plane. Projecting the axes collapses a basis vector to zero. Stage 1 now preserves that intentional upward normal and skips height tracing for unsupported frames, with GPU regression coverage for crossed cards and degenerate geometry.

References: `tools/jg_rtx/import.py`, `src/render/material/relief.wgsl`, upstream `src/scripts/labpbr/normal.ts`, and the [LabPBR normal standard](https://shaderlabs.org/wiki/LabPBR_Material_Standard#Normal_Texture_(_n)).

## Source material encoding

239 of the 338 Java imports had green equal to zero everywhere, including dirt and stone. Ordinary iron, copper, gold, and nether gold ores had no pixels classified as metal, unlike their deepslate equivalents. The runtime correctly interpreted these bytes as dielectric reflectance, but the source exports did not consistently represent the intended materials. Large dielectric values also reduced diffuse color on ore specks.

Resolve original MER or MERS inputs using texture-set semantics, including scalar declarations, PNG/TGA variants, and Java/Bedrock aliases. Encode dielectric reflectance and declared metal identities using the pack's curated rules. Preserve existing red-channel smoothness until stage 2. Keep emission's 255 sentinel distinct from real emission, and keep porosity separate from subsurface scattering. Record exact source inputs and fallback decisions in import provenance.

References: upstream `src/scripts/labpbr/textureSet.ts`, `materials.ts`, and `specular.ts`; `src/render/material/pbr.wgsl`.

## Material mip filtering

The original worker averaged metal IDs and dielectric reflectance bytes together; the shader interpolated those encoded values again. In deepslate iron ore, the metal fraction fell from 16.4% to zero at mip level four, while average dielectric reflectance increased from 3.9% to 19.4%. Averaging the blue channel can likewise convert porosity into scattering or vice versa.

Categorical green and blue channels must be selected from an unfiltered base-level texel before decoding. Continuous smoothness and emission can retain their mip chain. BSL also samples its LabPBR specular data at level zero. This prevents invented material identities; distant categorical patterns remain point sampled rather than integrated over the entire pixel footprint. A future decoded material representation could provide filtered metal coverage at additional memory cost.

References: `src/render/material/companions.rs`, `companions.wgsl`, BSL `lib/surface/materialGbuffers.glsl`.

## Roughness conversion awaiting approval

All 12 Bedrock imports used `1 - sqrt(roughness)` instead of the pack converter's `1 - roughness`. The shader then squared perceptual roughness for GGX, applying the curve twice. Amethyst cluster's mean roughness changed from approximately 0.213 to 0.452, predicting broader, duller highlights.

Stage 2 will correct conversion, reimport affected smoothness channels, and protect it with source-to-runtime regression checks. Inspect the Java exports for the same legacy conversion before assuming their red channels are correct.

References: `tools/jg_rtx/import.py`, upstream `src/scripts/labpbr/specular.ts`, `src/render/material/pbr.wgsl`.

## Albedo color space awaiting approval

Albedo mipmaps average sRGB bytes instead of linear-light color. Equal opaque black and white become 127 instead of approximately 188 in sRGB, predicting excessive darkening of high-contrast textures with distance. This predates the import. Stage 2 will make downsampling linear-light and alpha-aware, and audit importer resizing for the same issue.

References: `src/render/material.rs`, `src/render/pipeline.rs`.

## Rendering capabilities outside this fix

Only 18 of the 350 imported normal maps contained variable height. Most materials therefore have no parallax relief even with parallax enabled. This is primarily absent source height data, rather than discarded imported height. Bedrock conversions without authored height also use flat AO and height; reconstruction from normals is a separate quality improvement.

Bloxgloom currently offsets texture coordinates without height self-shadowing. Its specular environment uses an analytic sky and approximate local transport rather than reflecting scene geometry. BSL has parallax self-shadowing and screen-space scene reflections when advanced materials are enabled. Its checked-in default disables advanced materials, so those features must not be assumed active in every BSL screenshot.

References: `src/render/material/parallax.wgsl`, `companions.wgsl`, BSL `lib/surface/parallax.glsl`, `program/deferred1.glsl`, and `lib/settings.glsl`.

## Validation and live test

Stage 1 corrects canonical normal XY for 344 materials and material encoding for 347. Six normal and three material fallbacks are recorded in `assets/jg-rtx/provenance.json`; the source pack supplies no canonical input for those layers. A pixel audit confirmed all 350 albedos and red smoothness channels remain unchanged. Existing Java AO and height are retained, using independent channel resizing. The worker keeps green/blue mip bytes categorical and the shader selects them from the unfiltered base level, with filtered red smoothness and alpha emission. Texture, block, item, and state identities are unchanged.

The material catalog fingerprint changes with corrected PNGs. The default save directory is now `world-v30/` (`world-v30-fixture/` for lifecycle fixtures), following the repository's prerelease policy. Existing local worlds are not migrated or deleted.

The full `cargo test --quiet -- --test-threads=4` run passed 1,898 tests, with zero failures and 14 ignored. After adding the final crossed-card normal-frame guard, all 15 focused material tests passed again, including production GPU readbacks. `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `git diff --check` passed. Five Python conversion regressions passed. A complete source-pixel audit verified all canonical normal/material maps, including a separate reconstruction check for the composited sunflower.

The release `block-preview bloxgloom:iron_ore` image was inspected for functioning terrain, vegetation, and ore rendering. It is a rendering smoke check; gameplay appearance has not yet been accepted. The user's live test remains the acceptance gate for stage 2.

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

## Live test before stage 2

For the live test, inspect stone and cobblestone relief under changing sunlight; ordinary and deepslate iron/copper/gold ores; foliage and cutout edges; distant ore sheen; and a sealed cave with and without a local light. Roughness and color-space corrections remain pending during this test.
