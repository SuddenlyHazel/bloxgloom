# Foliage texture sources

`source/vegetation-concept-sheet.png` is the retained 3-by-2 pixel-art concept sheet for the flower, fern, grass, and sapling sprites. The six 512-pixel cells were cropped and scaled to 128 pixels with nearest-neighbor filtering. The sapling cell supplies `assets/textures/items/sapling.png`.

`source/broadleaf-leaves-concept.png` is the earlier canopy concept. `source/leaves-opaque-revision.png` is the current source for `leaves.png`: it covers the full cube face without alpha holes, so the trunk cannot show through leaf surfaces. `source/tall-grass-revision.png` and `source/fern-revision.png` are the current sources for their respective sprites; both keep complete fronds inside the canvas instead of cropping them at an edge.

The plant PNGs preserve alpha; the leaf tile is intentionally opaque. The original hand-drawn SVG drafts were removed after these higher-detail sprites replaced them. The item sapling SVG is a 128-pixel wrapper around its PNG so both asset formats point to the same final silhouette.
