# Native character performance sample

Measured 2026-10-01 with the release profile on llvmpipe (LLVM 19.1.7,
256-bit), OpenGL, Mesa 25.0.7. This is a software-renderer comparison, not a
hardware GPU or live-game frame-rate claim. Each command ran sequentially after
compilation, at 1280 × 720 RGBA16Float with 30 warmup frames. Timings are ms.

## Earlier five-style catalog: 128 actors, 300 measured frames

Command: `bloxgloom character-perf 300 128 <style>`.

| Style | Triangles/actor | CPU update p50/p95 | CPU submit p50/p95 | GPU pass p50/p95 |
|---|---:|---:|---:|---:|
| classic | 132 | 0.022 / 0.031 | 1.860 / 3.474 | 1.648 / 2.735 |
| 0 none | 72 | 0.240 / 0.539 | 2.493 / 3.799 | 2.256 / 4.059 |
| 1 tousled crop | 192 | 0.238 / 0.359 | 4.601 / 6.378 | 4.299 / 5.803 |
| 4 curly bob | 960 | 0.267 / 0.426 | 17.919 / 23.497 | 17.701 / 23.196 |

## Population cap, 512 actors, 100 measured frames

Command: `bloxgloom character-perf 100 512 <style>`.

| Style | Triangles/frame | CPU update p50/p95 | CPU submit p50/p95 | GPU pass p50/p95 |
|---|---:|---:|---:|---:|
| 1 tousled crop | 98,304 | 0.900 / 1.166 | 15.359 / 18.945 | 15.043 / 18.790 |
| 4 curly bob | 491,520 | 1.011 / 1.333 | 72.660 / 86.400 | 72.286 / 85.150 |

The dense curly-bob mesh is materially more expensive on this software GPU.
That earlier shared kit contained 2,100 triangles, but each admitted actor submits only
its body and selected hair. Catalog expansion does not multiply every actor's
vertex work by all hairstyles. There is no distance LOD in this slice.

CPU update covers joint sampling, instance packing and queue writes; GPU skinning
is part of GPU time. Total CPU submission includes update and possible driver
backpressure. GPU time includes target/depth clear. The fixed XY grid uses full
walk blend and varied face recipes, with identical framing for a given count.
No terrain, post-processing, UI, network or presentation is measured. There is
one final GPU wait/readback, not a wait after each frame. Pixel checksums confirm
readback completed, but are not portable visual goldens. Counts change screen
coverage, so compare styles at the same count rather than deriving a per-actor
scaling law from the two tables.

## Existing terrain benchmark

`bloxgloom perf 300 6` remains separate and excludes avatars. At the earlier five-style
milestone it produced the same 320,236 mesh vertices, 480,354 indices,
18,573,688 mesh bytes and 88,026 visible triangles as the untouched baseline
917af48937261532a57f114b9debfedd7189bedd.

| Sample | Scene setup | CPU steady p50/p95 | GPU steady p50/p95 |
|---|---:|---:|---:|
| untouched baseline | 4487.9 | 37.152 / 50.728 | 34.513 / 46.987 |
| five-style milestone | 4509.0 | 36.485 / 47.371 | 34.268 / 45.872 |

These single sequential samples show no terrain regression and preserve exact
scene/mesh counts; they are not a statistically established speed improvement.

## Complete 13-style catalog verification

The final release build was exercised on the same software adapter identified
above. All 65 native images (13 styles × idle, walk, crouch, left tool and right
tool, with three views in each image) were generated and visually inspected.
The original body, rig and clips remain exact; the shared catalog now contains
22,680 vertices / 7,560 triangles and only selected meshes are submitted.

These are single software-renderer samples on a shared cloud CPU. Timing variance
is high, including in the avatar-free terrain pass, so these numbers do **not**
establish hardware frame rates, timing parity with earlier runs, or a measured
speedup/regression between revisions. Counts and final readback are exact checks.

### 128 actors, 300 measured frames (30 warmup)

| Style | Triangles/actor | CPU update p50/p95 | CPU submit p50/p95 | GPU pass p50/p95 |
|---|---:|---:|---:|---:|
| classic | 132 | 0.023 / 0.051 | 1.980 / 5.302 | 1.660 / 4.580 |
| 0 none | 72 | 0.264 / 1.132 | 4.674 / 14.304 | 4.292 / 12.572 |
| 1 tousled crop | 192 | 0.250 / 1.155 | 6.213 / 17.929 | 5.580 / 18.701 |
| 11 rounded afro | 1,488 | 0.336 / 1.432 | 52.117 / 164.448 | 51.928 / 155.292 |

### Population cap, 512 actors, 100 measured frames

| Style | Triangles/actor | CPU update p50/p95 | CPU submit p50/p95 | GPU pass p50/p95 |
|---|---:|---:|---:|---:|
| 11 rounded afro | 1,488 | 1.215 / 3.090 | 145.625 / 240.067 | 146.159 / 239.408 |

The largest style submits 761,856 triangles at the 512-actor cap. Dense hair has
a material software-GPU cost; there is no distance LOD in this change. At 128
actors, the bald and tousled final pixel checksums remain identical to the earlier
five-style samples (`f29ac7c0c436aa66` and `23cbd0a645fc6cdf` respectively).

### Avatar-free terrain sample

`perf 300 6` retained exactly 320,236 mesh vertices, 480,354 indices, 18,573,688
mesh bytes and 88,026 final visible triangles. Current setup was 7,789.5 ms;
CPU steady p50/p95 was 85.065 / 156.035 ms, GPU steady p50/p95 was
82.845 / 147.152 ms. The visibly higher timing noise extends beyond characters;
use a controlled hardware run before drawing a timing-regression conclusion.

### Reproduce the native visual check

For each hair ID 1 through 13, run `character-preview` for `idle 0.35`,
`walk 0.2`, `crouch 1.0`, `tool_use_left 0.4` and `tool_use_right 0.4`,
with the hair ID as the final argument. Numerical coverage is substantially
denser: 872 original-clip poses plus 1,377 idle/walk blends per style; see
[compatibility evidence](hair_compatibility.md). These sampled tests do not
guarantee arbitrary clip blends, extra head-look or continuous clearance.
