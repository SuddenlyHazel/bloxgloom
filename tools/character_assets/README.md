# Authored character asset tools

The native `BGC2` meshes and PNGs are generated from the source kit by
`articulated.py`. The local-visibility bake is a separate bounded sidecar; it does
not edit source models, native mesh vertices, texture pixels, UVs, or animation.

After regenerating or changing any native mesh, rebuild local visibility:

```sh
python3 tools/character_assets/articulated.py
python3 tools/character_assets/bake_occlusion.py
```

Before shipping generated assets, verify both conversion and local visibility:

```sh
python3 -m unittest discover -s tools/character_assets -p 'test_*.py'
python3 tools/character_assets/bake_occlusion.py --check
```

The bake uses only the mesh files named by the conversion manifest. It verifies
those files' SHA-256 hashes and never opens a texture. It uses standard Python
only, fixed 32-sample cosine-weighted hemispheres, a 1 mm ray-origin bias, a
10 cm radius, smooth distance falloff, and a maximum 25% indirect attenuation.
There is no random seed or machine-specific acceleration library. A spatial
index and shared-vertex cache keep regeneration bounded; it is an offline step,
not startup work.

Only triangles sharing the receiver's exact material and rigid joint can
occlude it. Different hair styles, body variants, and independently moving limbs
never cast baked shadows on one another. Source-authored face feature surfaces
2–6 receive full visibility. This is a stable local depth cue, not dynamic
self-shadowing or a substitute for ground-contact shadows. Direct sunlight,
torch light, and the sealed-cave floor do not use this factor.

## Sidecar contract

`assets/models/player/articulated/local_visibility.ao` contains:

- 4 bytes `BGA1` and a little-endian `u32` material-record count
- For each material in ID order: the native mesh's 32-byte SHA-256, a
  little-endian `u32` vertex count, and one visibility byte per native vertex
- Visibility is normalized by 255; its minimum byte is 191 (approximately 0.75)

The complete checked-in sidecar is 28,716 bytes. Loading validates the record and
vertex counts, hashes, attenuation bounds, and absence of trailing bytes. A stale
sidecar fails with an explicit request to rerun the bake rather than silently
associating old visibility with new geometry. No rays or mesh-neighborhood search
run in the game.

The GPU vertex layout stays unchanged. `surface` bits 0–7 retain the native
semantic surface, bits 8–15 carry visibility, and bits 16–31 stay zero. The
character shader decodes visibility before vertex lighting and applies it only
to indirect fill and bounced light. Texture sampling remains nearest-neighbor.
