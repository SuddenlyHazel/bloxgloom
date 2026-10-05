# PBR material processing issues

The October 5, 2026 audit compared 350 imported JG RTX materials, their source pixels, Bloxgloom's material pipeline, and the BSL reference in `external/shaders`. It found import and filtering defects as well as rendering capability gaps. Predicted visual effects below require gameplay verification.

## Work and approval sequence

1. Correct normal orientation, source material encoding, and categorical material filtering. Run automated checks and performance comparisons, then commit the changes.
2. Pause for the user's live test. Do not begin roughness or color-space changes until the user approves continuing.
3. Correct roughness conversion and albedo color-space handling after approval.

Height reconstruction, parallax self-shadowing, and scene reflections are documented here but are outside the two authorized implementation stages.

## Normal orientation

The original importer flipped every Java normal map's green channel. Among comparable non-flat Java and Bedrock maps, 118 pairs had identical XY data, nine had reversed green, and 27 differed otherwise. Most Java maps were already DirectX normals, the convention our derivative UV frame uses. A universal flip therefore made their lighting disagree with surface relief. A universal removal would also miss the source pack's exceptions.

Use canonical Bedrock normal maps resolved through the pack's texture sets and edition name mapping where available. Preserve the authored Java AO and height channels. Decode remaining Java maps as LabPBR DirectX data, recording exceptions and fallbacks. Resize data channels independently; alpha represents height or emission, not image transparency.

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

Stage 1 is in progress. Gameplay appearance has not yet been accepted. Automated tests must cover canonical normal slopes, ore classifications, sentinel emission, and filtering that cannot invent metal IDs or confuse porosity with scattering. Run Rust tests, formatting, Clippy, and voxel/bounced release benchmarks. Compare scene setup, mesh size, CPU timing, and GPU timing separately.

For the live test, inspect stone and cobblestone relief under changing sunlight; ordinary and deepslate iron/copper/gold ores; foliage and cutout edges; distant ore sheen; and a sealed cave with and without a local light. Roughness and color-space corrections remain pending during this test.
