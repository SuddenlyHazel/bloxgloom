# Foliage texture sources

`source/vegetation-concept-sheet.png` is the retained 3-by-2 pixel-art concept sheet for the flower, fern, grass, and sapling sprites. The six 512-pixel cells were cropped and scaled to 128 pixels with nearest-neighbor filtering. The sapling cell supplies `assets/textures/items/sapling.png`.

`source/broadleaf-leaves-concept.png` is the retained dense canopy artwork used for `leaves.png`, scaled to 128 pixels with nearest-neighbor filtering. The finished cutout averages about 82% opaque pixels.

The generated PNGs preserve alpha. The original hand-drawn SVG drafts were removed after these higher-detail sprites replaced them. The item sapling SVG is a 128-pixel wrapper around its PNG so both asset formats point to the same final silhouette.
