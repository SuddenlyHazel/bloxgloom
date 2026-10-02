# Distant terrain

Bloxgloom streams a separate visual terrain layer beyond nearby voxel chunks.
Graphics settings expose **Distant terrain** (off, 512, or 1,024 blocks) and
**Distant detail** (coarse, balanced, or detailed). Defaults are 512 blocks and
balanced detail. This layer never supplies collision, targeting, inventories,
entities, or world edits. Near chunks retain server authority and voxel lighting.

## Implementation

`src/lod/` represents 32×32 horizontal columns as vertical occupied spans with
catalog state IDs, approximate sky/emission, and explicit examined height
intervals. Unknown terrain is different from examined air. Negative coordinates
use Euclidean division. Parent reduction unions occupied child samples: it can
close fine horizontal air paths, while vertical gaps shared by the samples
survive. Over-budget material simplification merges only contiguous occupied
boundaries and never fills a vertical air gap. Summaries return unavailable if
occupied geometry still exceeds the cap.

`src/server/lod/` uses a dedicated worker without installing distant chunks into
simulation. Builtin terrain has direct scale-dependent sampling. Saved chunks
and committed snapshots awaiting checkpoint override procedural samples,
including high structures outside the builtin terrain band. Registered
contributors use the installed generator composition through full generation;
bounded immutable resident snapshots also supply previously observed heights.
Unobserved heights remain explicit. This accurate fallback is slower, so
contributor worlds clamp the horizon to 512 blocks and the maximum level to 3.
Builtin worlds support 1,024 blocks and level 4. A public coarse contributor
contract is deferred.

Builtin coarse tiles sample representative center columns and can miss narrow
natural features. Saved edit differences anywhere in a coarse cell are
preserved conservatively and can widen a thin structure. These are visual
approximations; only nearby full voxels represent exact occupied geometry.

Committed edits invalidate affected tiles and ancestors. Pending builds are
cancelled only when their horizontal dependencies intersect the edit. Unrelated
edits cannot repeatedly starve distant generation. Large invalidation batches
use one global refresh. Session, request, revision, and build generation fences
reject obsolete replies and meshes across edits, teleports, and reconnects.

`src/client/lod/` requests coarse skyline tiles first and uses movement
hysteresis. A dedicated mesh worker prepares texture-derived linear face colors.
`src/render/lod/` draws camera-relative span geometry into the shared HDR/depth
scene. Complete sibling families refine uploaded parents only when the children
have examined the parent's occupied height intervals. Pending or incomplete
children retain their parent. GPU-ready near chunks, including known-empty
chunks, mask LOD in three dimensions; receipt alone cannot hide terrain.
Exterior boundary walls remain drawable until neighbor selection is known, and
mixed-level boundaries split faces without filling bridges or cave openings.

Daylight, emission, weather, and shelter affect distant shading. Clear-weather
fog ends at the negotiated horizon. Opaque terrain and static leaf canopies use
texture averages; small plants are omitted. Transparent, animated, and moving
content, distant bounced lighting, and screen-space error selection are deferred.
Finer detail can be unavailable within resource caps; drawable coarse parents
remain the fallback.

## Resource limits and caching

| Resource | Limit |
| --- | --- |
| Summary payload | 60 KiB; 3,072 spans; 2,048 examined intervals; 32 spans/column |
| Server admitted builds / worker threads | 4 / 1 |
| Server requests per client / summary cache | 8 / 64 tiles |
| Source snapshot overlay / fallback source chunks | 4 MiB / 4,096 chunks |
| Captured resident snapshots per build | 8 MiB / 2,048 chunks, without cache pins |
| Client requests / retained summaries | 4 / 128 tiles |
| Mesh worker job/result queues | 8 each |
| CPU mesh / pending GPU uploads | 8 MiB per mesh / 8 meshes |
| LOD GPU geometry | 128 MiB, independent of near terrain |
| Uploads | At most one LOD mesh per frame, after near uploads |
| Derived disk cache | 128 tile files, approximately 8 MiB maximum |

`<world>/lod-cache` is disposable derived data. Atomic replacement and checksums
protect same-session reloads. Startup conservatively discards the previous cache,
avoiding stale summaries after a crash between world commit and invalidation.
The cache therefore accelerates same-session eviction/revisit, not restart.
Deleting or corrupting it never changes authoritative save data.

The wire version is 25. Disabled LOD preserves ordinary play with a matching
server; older wire versions still fail the existing handshake version check.
No authoritative world-format conversion was added.

## Reproduce verification

```sh
cargo test
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo run --release -- perf 300 6
cargo run --release -- perf 300 6 bounced
cargo run --release -- lod-perf 300 6 512
cargo run --release -- lod-perf 300 6 1024
cargo run --release -- lod-preview /tmp/bloxgloom-lod-preview 1024
BLOXGLOOM_PERF_IMAGE=/tmp/bloxgloom-near-and-lod.png cargo run --release -- lod-perf 300 6 512
```

`lod-preview` produces day, night, storm, negative-coordinate terrain, bridge,
cave, and ready-near-coverage PNGs through the production GPU pipeline. Its
standalone terrain views intentionally omit near chunks. The integrated
benchmark includes both near terrain and LOD and reports summary construction,
mesh construction, summary/mesh bytes, selected tile count, resident GPU bytes,
upload ramp, and steady CPU/GPU timings separately. It excludes presentation,
networking, server simulation, and live gameplay. Debug client telemetry reports
cached/drawn LOD tiles and GPU bytes; server completions report generation and
queue time.

The optional `BLOXGLOOM_PERF_IMAGE` captures the integrated benchmark's final
frame after all measured samples and timestamp readbacks. PNG capture time is
excluded from the frame measurements. Parent reduction timing uses eligible
already-generated child families without changing the rendered summaries.

Real nonblocking loopback tests cover initial negotiation, two-client committed
edits, contributor composition, high structures, teleport/movement fences,
reconnect sessions, cancellation under full admission, scoped in-flight edits,
and cache corruption/reload. Test saves use isolated temporary directories.
Full suite runs use `RUST_TEST_THREADS=4` to avoid excess contention in the
wall-clock-limited script fixtures.

## Measurements, 2026-10-01

Apple M1 Pro, Metal, 1,280×720 offscreen, seed `0xb10c6100`, near radius 6,
300 steady frames. Each row is one run; these results establish a baseline,
not a guarantee for every camera, world, GPU, or extension.

| Scene | CPU steady p50 / p95 (ms) | GPU steady p50 / p95 (ms) | LOD GPU geometry | LOD selected tiles |
| --- | --- | --- | --- | --- |
| Near only, voxel | 0.312 / 0.581 | 0.265 / 0.450 | 0 | 0 |
| Near only, bounced | 0.346 / 0.658 | 0.277 / 0.934 | 0 | 0 |
| Near + 512 LOD, voxel | 0.946 / 1.362 | 0.952 / 1.359 | 90,212,800 B | 45 of 57 resident |
| Near + 1,024 LOD, voxel | 1.107 / 1.469 | 1.158 / 1.580 | 118,998,400 B | 61 of 73 resident |

Near setup took approximately 2.4 s, with 18,573,688 bytes of near geometry and
88,026 near triangles. LOD setup is excluded from frame samples:

| Horizon | Cold summaries | Eligible parent reduction (12 parents) | Meshing | Summary bytes | LOD generated triangles |
| --- | --- | --- | --- | --- | --- |
| 512 | 381.11 ms | 16.94 ms | 254.45 ms | 2,481,500 | 902,128 |
| 1,024 | 615.34 ms | 17.61 ms | 325.45 ms | 3,151,694 | 1,189,984 |

The integrated upload ramps were 219 and 235 frames versus 163 near-only.
All near mesh uploads retained the existing two-mesh/frame admission, and all
requested default-quality LOD summaries/meshes fit their caps. Cave/overhang
surfaces are deliberately retained; geometry remains a significant cost.
This measurement motivated the separate 128 MiB LOD geometry budget.

Debug real-listener fixtures measured a base tile at 37–39 ms cold and 2 ms from
the same-session disk cache (50,333 bytes), a committed edit delivered to two
clients in 129 ms, and recovery after a 1,200-block teleport in 770 ms. A
128-block composed contributor tile took 2.14 s cold in debug. These timings
measure server delivery, not GPU-visible edit latency. The release local client
joined a unique temporary world and streamed both layers, reporting roughly
60 FPS once initial work settled. The automation tool could not select its
unbundled window; appearance was verified with actual GPU-generated previews
and integrated benchmark images instead.

[Near only](verification/near-only.png), [512 blocks](verification/near-and-lod-512.png),
and [1,024 blocks](verification/near-and-lod-1024.png) show the same camera.
The HUD is a benchmark fixture; timings come from the logs, not its labels.
Full [voxel baseline](verification/perf-voxel.txt),
[bounced baseline](verification/perf-bounced.txt),
[512 log](verification/lod-512.txt), and [1,024 log](verification/lod-1024.txt)
include upload, triangle, and percentile details.

Separate `/usr/bin/time -l` runs recorded peak process RSS of 344,752,128 bytes
for near-only, 299,417,600 bytes for 512, and 260,259,840 bytes for 1,024. These
whole-process counters include graphics-driver allocations and are non-monotonic;
they do not isolate CPU terrain storage. The exact summary, geometry, and queue
budgets above provide the component limits. Full memory-run logs are
[near-only](verification/memory-near.txt), [512](verification/memory-512.txt),
and [1,024](verification/memory-1024.txt).

An opt-in GPU version of the real two-client edit fixture measured **193.342 ms**
from sending the edit to updated terrain mesh construction, upload, draw, and
GPU completion. The [before](verification/edit-before.png) and
[after](verification/edit-after.png) images differ in 12,826 pixels and show the
removed glowstone cube; the platform remains. Device/pipeline/color preparation
and the baseline draw were warmed before timing. Image readback/encoding is
excluded. This uses production CPU meshing on a scoped worker and production
LOD/HDR rendering offscreen; client scheduling and window presentation are
outside this measurement. The [test log](verification/bloxgloom-lod-edit-gpu-qa.log)
records the endpoint.

```sh
BLOXGLOOM_LOD_GPU_QA=/tmp/bloxgloom-lod-edit-gpu-qa cargo test committed_edit_refreshes_two_distant_clients_without_waiting_for_checkpoint -- --nocapture
```

The manual real-listener exploration stress test visited **130 distinct coarse
tiles and revisited the first** in 69.8 s. Worst ping and movement responses were
24 ms. Authoritative residents peaked at 5,416 chunks; pins peaked at 40 and
returned to zero after disconnect. The outbound queue peaked at 47,708 bytes.
Derived caching retained exactly 128 files totaling 5,351,080 bytes, and no
world drops were activated. The slow cold tile work did not delay movement or
ping replies. This test is ignored in the default suite because it intentionally
runs over a minute; execute it explicitly for exploration pressure checks:

```sh
cargo test lod_exploration_revisits -- --ignored --nocapture
```

[Exploration verification](verification/exploration.txt) records the captured
manual test results and its reproducible command.

Final [checks](verification/checks.txt): 1,489 tests passed, seven ignored;
formatting, all-target/all-feature Clippy with warnings denied, release build,
and AST graph refresh passed. The exploration stress and GPU storm-fog tests
were also run explicitly and passed; the GPU edit fixture passed with its
opt-in capture enabled.
