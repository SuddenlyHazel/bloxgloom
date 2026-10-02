# Foliage texture sources

`source/vegetation-concept-sheet.png` is the retained 3-by-2 pixel-art concept sheet for the flower, fern, grass, and sapling sprites. The six 512-pixel cells were cropped and scaled to 128 pixels with nearest-neighbor filtering. The sapling cell supplies `assets/textures/items/sapling.png`.

`leaves.png` now comes from the user's local NAPP 512x FREE 5.0.1 pack
(`assets/minecraft/textures/block/oak_leaves.png`), by the NAPP team:
https://napplab.com/. The grayscale leaf art receives a fixed RGB 119, 171, 47
tint and is resized to 128×128 with Lanczos filtering. Alpha is preserved for
the existing cutout leaf material; the renderer's alpha-weighted mipmaps keep
transparent texels from darkening the visible leaves. This pack has no oak-leaf
normal/specular companions. Reproduce with the block importer's
`--natural-only` option, documented in `assets/textures/blocks/SOURCES.md`.

`source/broadleaf-leaves-concept.png` and `source/leaves-opaque-revision.png`
are retained sources for the earlier canopy art. `source/tall-grass-revision.png`
and `source/fern-revision.png` remain the current sources for those sprites;
both keep complete fronds inside the canvas instead of cropping them at an edge.

The plant and leaf PNGs preserve alpha. The original hand-drawn SVG drafts were removed after these higher-detail sprites replaced them. The item sapling SVG is a 128-pixel wrapper around its PNG so both asset formats point to the same final silhouette.
