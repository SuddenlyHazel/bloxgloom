# Item texture sources

`stick.png` and its `_n`/`_s` companions come from the user's local NAPP 512x
FREE 5.0.1 pack (`assets/minecraft/textures/item/stick*.png`), by the NAPP team:
https://napplab.com/. The albedo is resized to 128×128 with Lanczos filtering
and retains its transparent silhouette. Companion channels are filtered
independently with box filtering, matching the terrain material data format.
Reproduce using `assets/textures/blocks/source/import_napp.py --natural-only`
with the pack's `textures/block` directory, as documented in the block sources.

`stick.svg` is retained earlier artwork. The runtime catalog uses `stick.png`.
The seed and sapling assets are unchanged; sapling sources are documented in
`assets/textures/foliage/SOURCES.md`.
