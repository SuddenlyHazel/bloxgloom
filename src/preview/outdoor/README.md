# Outdoor lighting acceptance fixture

From the checkout root:

```sh
BLOXGLOOM_SUN_SHADOWS=high cargo run --release -- outdoor-preview outdoor-previews
```

The command writes four 1280×800 PNGs and `capture-settings.txt`. Keep the
adapter/backend and shadow quality identical for before/after comparisons.
Every view uses the same authored scene, noon, exposure 1.0, bloom 0.12,
no bounced light, and an idle animation sampled at 0.35 seconds. No save is
loaded or written. Camera transforms are fixed in `outdoor.rs`.

Inspect:

- `01-overview.png`: open grass retains texture; tree masses separate from sunlit ground
- `02-canopy-character.png`: layered leaves read as a volume, with readable shaded characters and contact grounding
- `03-cave-entrance.png`: the sunlit exterior transitions into progressively darker stone without broad ambient fill
- `04-dark-interior.png`: the deep unlit interior stays dark at the same exposure

These are visual acceptance references, not automatically scored image tests.
The adjacent tests protect authored geometry, scene independence, and light
separation. Adapter details and sampled actor light levels are printed to stdout.
Software GPU images do not establish hardware performance.

Keep the existing industrial and neon scenes as regression checks:

```sh
BLOXGLOOM_SUN_SHADOWS=high cargo run --release -- sandbox-preview sandbox-regressions all all hero idle
BLOXGLOOM_SUN_SHADOWS=high cargo run --release -- calibration-preview calibration-regressions
cargo test preview::outdoor
```

Do not raise exposure or add emissive blocks to make the interior shot readable.

For a matched stationary temporal-AA comparison, repeat into a different folder
with `BLOXGLOOM_TAA=1`. Each image accumulates eight jittered samples at the same
camera and frozen animation pose on supported backends. `capture-settings.txt`
records the requested toggle; GL adapters report a disabled fallback on stderr.
This does not establish moving-character or moving-foliage ghosting behavior; see
`docs/rendering/temporal-aa.md` for current limits and regression checks.

## Motion and disocclusion

Use the same executable and adapter for both runs:

```sh
BLOXGLOOM_SUN_SHADOWS=high BLOXGLOOM_AO=0 BLOXGLOOM_TAA=0 cargo run -- outdoor-creature-motion-preview motion-off
BLOXGLOOM_SUN_SHADOWS=high BLOXGLOOM_AO=0 BLOXGLOOM_TAA=1 cargo run -- outdoor-creature-motion-preview motion-taa
```

Each sequence has 28 frames with persistent history, one submitted sample per
frame. Frames 00–07 warm up a stationary scene; 08–19 pan the camera while real
character walk clips play and characters plus registered/skinned GLB creatures
translate across the ground. Explicit animation steps make both runs independent
of wall-clock rendering time. Characters and GLBs use previous submitted joint
palettes; foliage deformation explicitly rejects history. The
production foliage cutout vertices undergo deterministic deformation, including
in their shadow pass, to expose silhouette/disocclusion trails. This tests moving
foliage geometry without introducing a runtime wind feature. Vertex/actor light
samples are frozen to isolate temporal behavior. `outdoor-motion-preview` remains
available without the preview-only GLB registration. Scene AO is disabled above
to isolate AA; repeat with it enabled for the complete composed pipeline.

Frames 20–23 cut to the moving character's first-person eye and exercise head
clipping; frame 23 resizes from 640×400 to 800×500; 24–27 cut back under the canopy.
Compare limb silhouettes, newly uncovered sky between leaves, trunk boundaries,
and the first frame after each cut/resize. Do not equate ordinary one-pixel jitter
differences with persistent ghost trails. The saved sequence notes describe the
same phases. These software-GPU captures are not performance measurements.

For the fixed noon fixture, `tools/check_temporal_motion.py motion-off motion-taa`
(Pillow + NumPy) checks newly exposed sky outside the one-pixel current silhouette
neighborhood and verifies all 28 frame sizes. It deliberately does not score
same-depth interior trails or replace visual inspection. Current bounded evidence
and limitations are recorded in `docs/rendering/temporal-aa.md`.
