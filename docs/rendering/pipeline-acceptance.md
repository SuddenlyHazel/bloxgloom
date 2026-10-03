# Larger rendering pipeline acceptance (2026-10-03)

The implementation checkpoint is `226a327a82a5434062cfa24bbbf4ea6e1a9664bb` on
`dot/outdoor-lighting-sprint`, following the earlier `7f9cc97` lighting pass.
All acceptance captures below use one copied executable, SHA-256:

    ecaccf3b6e88b2af97086afb3dde133d77ad581c3f7d431cb8886c42f595f058

## Automated checks

- Workspace/all-feature test executables: 1,708 application tests and 52 host-API
  tests passed; zero failed; 11 application tests intentionally ignored
- Final suite ran serially with the SwiftShader Vulkan ICD selected; previews
  explicitly printed the SwiftShader/Vulkan adapter identity
- `cargo clippy --all-targets --all-features -- -D warnings` passed
- `cargo fmt --all -- --check` passed; workspace/all-feature doc tests passed
- Explicit GL AO fallback test passed. Portable soft-shadow tests passed on GL
  and Vulkan. One separate GL first-person color-readback fixture returned black
  buffers in the isolated shadow check; its Vulkan counterpart and the complete
  integrated suite passed
- Independent review covered submitted-pose history, jitter cancellation,
  layer/skin palettes, indirect normalization, and particle ordering. Fixes include
  contact/baked-visibility overlap, foliage averaging metadata, thin-caster
  search coverage, and invisible-particle temporal masking

No performance benchmark was run. Graphify query/update could not run because
that executable is unavailable in this environment.

## Matched images

`tools/capture_rendering_review.py` records raw logs, settings, adapter environment,
exit status, and executable identity in its output manifest. Exposure is 1.0,
bloom is 0.12, and the geometry/camera are fixed within each pair.

- **Courtyard AO:** `depth-ao-off` versus `depth-ao-on` isolates AO strength 0
  versus 0.75, radius 1.5. Player feet, steps and wall creases gain local depth.
  99,091 pixels differ, maximum local RGB reduction 52 code values; whole-image
  mean reduction is 0.266. The inspected sky and emissive-inset regions are
  pixel-identical. This camera shows players and structure; the registered GLB
  actor behind the foreground player is occluded, so it is not GLB image evidence
- **Soft sun:** `outdoor-shadow-fixed` versus `outdoor-shadow-soft` isolates
  softness 0 versus 1 with AO fixed at 0.75. The canopy view changes 166,450 pixels,
  maximum absolute RGB change 13; the deep-interior image is pixel-identical.
  The synthetic aligned-blocker fixture additionally confirms contact stays
  tight while detached shadows broaden, including a thin diagonal caster
- **Generic visible-mesh AO:** an extra `outdoor-ao-off` capture compared with
  `outdoor-shadow-soft` holds soft sun/contact/AA settings fixed. The canopy view
  changes 48,592 pixels with maximum reduction 44, including the visible GLB
  validation meshes. The dark-interior view remains visually unchanged; 423
  pixels differ by at most one code value, so this AO pair is not bit-identical
- **Object motion:** `motion-aa-off` versus `motion-aa-on` holds AO at 0.75 and
  high soft sun/contact settings fixed for 28 frames. Inspected overlap frames
  12/19 and first-person/resize/return cuts 20/23/24 show no obvious retained
  silhouettes. The reproducible sky-disocclusion check covers 8,940 exposed
  pixels outside the current one-pixel edge neighborhood, with zero dark remnants
- **Sandbox regressions:** factory/noon and neon/night images were inspected;
  authored materials, local emission and scene-specific palettes remain intact

The simple colored GLB figures are validation assets, not final creature art.
The raw full-size images are the evidence; local crops or display scaling are
not substituted for source captures.

## Remaining limits

Scene AO can only see current depth-buffer geometry. Its local mean/max light
ratio is a conservative suppression estimate, not exact geometry visibility.
Soft sun filtering is a bounded comparison-depth approximation (up to 79 shadow
comparisons per Medium/High receiver). Unsupported deformation/particles reject
TAA rather than providing fabricated motion. TAA remains opt-in.

The software-GPU images and numerical regressions do not establish frame cost,
all possible same-depth ghosting behavior, or acceptance on the user's hardware.
Hardware timing, fast motion, mod combinations and representative live-world
review remain user-owned validation; no merge or remote publication is implied.
