# Authored face layers

These 23 PNGs are byte-exact copies of the approved 32 × 32 expression kit.
`mapping.json` records stable zero-based IDs, names, file paths, SHA-256 hashes,
and the 512 × 256 atlas's head-front rectangle `(32, 32, 32, 32)`.

Eye IDs: 0 classic, 1 cute_glint, 2 kawaii_star, 3 playful_wink,
4 happy_crescent, 5 neon_focus, 6 neon_curious, 7 soft_sleepy.
Mouth IDs: 0 classic, 1 soft_smile, 2 cat_smile, 3 tiny_open, 4 playful, 5 smirk.
Never reorder these identities. Eye layers include their brows and temple accents.

Composition: clean face, selected eye layer, selected mouth layer. Overlay only
the existing head-front UV rectangle. Do not resample the body atlas. Runtime
feature-array order is clean 0, eyes 1–8, mouths 9–14. PNGs use straight RGBA;
color textures are sRGB with nearest filtering, masks are linear/unorm bytes.

A mask's alpha selects iris pixels; its grayscale encodes shading. For requested
8-bit color channel C and mask value S, output `round(C * S / 128)` when S ≤ 128,
otherwise `round(C + (255 - C) * (S - 128) / 127)`. A default/no-tint selection
keeps the original eye-layer colors exactly. Preserve pixels with mask alpha 0,
including pupils, whites, highlights, lids, brows and cyberpunk temple accents.
Closed-crescent masks are empty; wink masks contain only the open iris. Neon
colors are albedo only, not emission or bloom.
