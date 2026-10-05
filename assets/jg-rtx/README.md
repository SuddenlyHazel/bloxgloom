# JG RTX materials in Bloxgloom

Selected textures by **Jason J. Gardner**, from [JG RTX](https://github.com/jasonjgardner/jg-rtx). The exact source revision and every imported albedo/map source are recorded in `catalog.json` and `provenance.json`. Upstream contributor acknowledgments are retained in [CREDITS.md](CREDITS.md).

The source checkout's [LICENSE](LICENSE) is **Creative Commons Attribution-ShareAlike 4.0 International**. Its README displays a contradictory “CC-NC-BY-SA” label pointing to that same file; this import preserves the actual license text and records the discrepancy. Imported artwork and our texture adaptations are distributed under that license. This attribution applies to the imported artwork; it does not change the project's code license or unrelated artwork licenses.

## Reproduce

```sh
uv run --with pillow --with numpy tools/jg_rtx/import.py /path/to/jg-rtx
```

The importer replaces matching existing artwork while preserving runtime texture and block identities. New opaque materials live in `assets/textures/blocks/`, cutout leaves and plants in `assets/textures/foliage/`, and the replacement sapling icon in `assets/textures/items/`. Existing stick and seed items have no matching upstream artwork and retain their previous sources. Machine faces use JG bricks, furnace, hopper, and barrel faces.

The explicit append-only identities in `tools/jg_rtx/block_ids.json` must never be reused or renumbered. New selections receive the next unused index. `catalog.json` defines block material assignments and generated Rust embeddings live in `src/content/jg_rtx/assets.rs`.

## Adaptations

- Wholly transparent source albedos are skipped in favor of a visible source. The Java amethyst cluster is blank, so its visible Bedrock TGA supplies the imported material.
- PNG/TGA inputs become RGBA PNGs with matching companion dimensions, retaining source detail up to 256 pixels. Animated vertical strips use their first frame; animation is not imported.
- Grayscale grass and selected leaves receive fixed natural color tints. Grass side overlays are composited over the opaque side. Cube faces are opaque; leaves and plants preserve source alpha.
- Java labPBR normal XY uses the renderer's DirectX tangent convention: green is inverted, blue retains ambient occlusion, and alpha retains height. The renderer identifies these imported materials and reconstructs normal Z independently. Bedrock normal XY retains DirectX orientation, with full ambient visibility in blue and separate height maps used when present. Missing maps receive flat normals and rough dielectric material defaults.
- Java `_s` maps preserve labPBR smoothness, F0/metal category, porosity/subsurface, and emission channels. Bedrock MER is converted to labPBR: smoothness is `1 - sqrt(roughness)`, metalness selects dielectric F0 or full metal, and emission uses alpha 0–254. MERS subsurface is translated into the labPBR subsurface range. MER is never copied directly into `_s`.
- The sunflower head is composited into its upper sprite because Minecraft's extra oriented flower geometry is absent here. Logs retain separate end grain, and tall flowers retain lower and upper material assignments.

This import adds source-backed cube, leaf, and plant materials. It does not imply Minecraft-specific mechanics, stair/slab geometry, animated water, ore crafting recipes, or new custom entity models.

## Available content and generation

The catalog adds **266 block types and corresponding inventory items**: 162 ordinary cubes, 28 directional log/mineral cubes, 13 leaf types, 55 single-cell plants, and 8 two-cell plants. The checked-in manifest lists every namespaced key. All are available through the local admin item browser or `give bloxgloom:<key> <count>`; for example, `give bloxgloom:cherry_log 32` or `give bloxgloom:sunflower 8`.

New builtin worlds generate regional granite, diorite, andesite, tuff, calcite and basalt strata, deep deepslate, small depth-dependent ore deposits, varied forest soils, eight named flowers, and nine named tree species alongside the existing generic tree. Spruce uses layered crowns, acacia uses broad flat crowns, and jungle trees grow taller. Chunk generation and distant summaries share the same material selection and absolute-coordinate tree anchors. Generator version 6 uses the fresh default save folder `world-v29/`.

Tall plants place both halves in one authoritative transaction, consume one item, and harvest once. Named leaves participate in the existing support/decay rules and species sapling drops. Sapling growth and distinct biome ecology for every imported plant remain future work. Vines, lichen, lily pads, petals, and crystal buds currently use crossed sprites: attached, horizontal, waterlogged, and directional custom geometry are not implemented. The broader masonry, color, and fantasy palettes are available to build with; this generation pass does not add dedicated fantasy biomes.
