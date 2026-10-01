# Articulated renderer performance

The approved geometry is materially denser than the removed seven-joint kit.
This change preserves that geometry and provides no distance LOD. A hardware-GPU
crowd test is recommended before choosing a production population budget.

The only player renderer uploads two static body meshes and thirteen hairstyles
once. Each admitted actor submits its selected body plus selected hair, never the
whole catalog. Admission is nearest-first and capped at 512 actors. Every actor
updates 30 joint matrices (1,920 bytes), plus a compact instance record.

## Measurements

2026-10-01, release build, llvmpipe LLVM 19.1.7 / Mesa 25.0.7 (software OpenGL),
1280×720 RGBA16Float. These are single shared-cloud CPU samples, not hardware
frame-rate claims. Shader/asset setup is excluded. Concurrent validation work
increases timing variance, so timings do not establish a speedup/regression.

`character-perf 300 128 <hair>` uses 30 warmup frames, 300 measured frames,
a nonoverlapping grid, equal numbers of both bodies, and full walk blend.

| Selection | Triangles/actor (mean) | CPU joint update p50/p95 | CPU submission p50/p95 | GPU pass p50/p95 |
|---|---:|---:|---:|---:|
| No hair | 2,954 | 0.575 / 1.059 ms | 78.008 / 124.879 ms | 76.706 / 122.407 ms |
| Rounded afro | 4,538 | 0.610 / 7.650 ms | 190.444 / 375.770 ms | 182.131 / 361.522 ms |

GPU time includes the character pass and target/depth clear. CPU submission may
include driver backpressure; it is not completion latency. No terrain, network,
post-processing, UI or presentation is measured. Final HDR readback completed
with checksums `f71fab9de11eed2f` and `d744ccea3878ef6c`, respectively.

The full 512-actor admission cap is covered by bounded-buffer/grouping GPU tests.
This is a safety cap, not a promise that rendering 512 articulated characters
meets a hardware-specific frame budget.

## Terrain-only comparison

`perf 300 6` is measured separately because it excludes characters. It retains
320,236 mesh vertices, 480,354 indices, 18,573,688 mesh bytes, and 88,026 final
visible triangles. Feature sample: scene setup 4,832.7 ms; steady CPU p50/p95
70.264 / 88.153 ms, GPU p50/p95 66.624 / 85.024 ms. Fresh-main (`519a2cd`) sample: setup 4,764.3 ms; steady CPU p50/p95
41.314 / 50.988 ms, GPU p50/p95 38.683 / 47.630 ms. Exact scene and mesh counts
match. The slower feature timing was captured under concurrent validation load;
these samples do not isolate a terrain-code regression, and no terrain renderer
code was changed.
