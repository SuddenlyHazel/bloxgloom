# Water terrain

The builtin generator now creates meandering rivers, larger lakes and small ponds. `bloxgloom:water` is a registered, non-solid fluid block. Water surfaces are transparent, with animated normal ripples, sky reflections, light absorption and the existing water rain-impact profile. Sand/gravel shores and sealed upper beds keep generated basins readable. Tree roots and ground plants stay out of generated water; neighboring dry trees can overhang it.

Generation uses absolute coordinates and the world seed, including negative coordinates. A river has a constant surface elevation; each lake/pond has its own level based on surrounding terrain. Channels and basin profiles are shared by authoritative chunks and builtin LOD summaries. The starter area stays dry. Fluid boundaries survive LOD budget reduction; distant and streamed near water now use transparent passes with shared tint, ripples, reflections and lighting. See [distant terrain](docs/lod/README.md) for transition textures and persistent caching.

## Try it

```sh
cd /Users/hazel/src/bloxgloom
cargo run --release
```

This uses a fresh `world-v28/` save. Existing saves stay untouched. Terrain generator revision is **5**, the wire protocol is **33**, and water uses block/state/item ID **16**. Existing builtin texture IDs stay unchanged; the water swatch texture is appended after companion maps at ID **51**.

With the default seed, a river is near **X 64, Z 0**. A lake shore is near **X -80, Z 100** and a pond is near **X -48, Z -108**. Use F3 coordinates and flying to explore, then disable flying in F4 to test ground movement. Water is walk-through: ordinary walking, jumping and gravity remain unchanged. Swimming and buoyancy are deferred. Normal block targeting passes through water to the bed/bank, and solid placement can replace water. F4 can grant `bloxgloom:water` for manual placement.

To render natural examples without opening a window:

```sh
cargo run --release -- water-preview /tmp/bloxgloom-water-preview
```

The command writes `river.png`, `lake.png` and `pond.png` and prints their coordinates and surface heights.

## Scope and integration

This pass adds **static source water**. Water does not spread, drain or refill after edits. Swimming, buoyancy, flow simulation, buckets, currents, drowning, refraction and dedicated swimming animations remain future work. Existing baked locomotion clips remain in use. Creatures and drops also retain their existing movement.

Packages can register cube blocks with `material='fluid', solid=false`; use `replaceable=true` to allow construction to displace them and `sky_attenuation` for per-voxel absorption. Native registrations can choose their fluid swatch. Luau fluids use the default water tint. See [registered content](docs/modding/REGISTERED-CONTENT.md).

The server retains movement and collision authority. Water has no solid collision flag. Missing snapshots defer movement; procedural fallback is never used as movement authority. Fluid mesh seams use captured authoritative neighbors. Workers retain normal revision fencing and seam refresh. Greedy faces omit water/water and water/solid boundaries, while opaque banks and beds stay visible. The transparent pass runs after ambient occlusion, uses depth testing without depth writes, sorts water chunks back to front and marks moving surface pixels reactive for temporal AA. Water does not cast opaque shadows.

## Verification

`cargo test --workspace` passes: 1,866 game tests and 61 host API tests, with 13 game tests ignored. The release build, `cargo fmt --all -- --check` and `cargo clippy --all-targets --all-features -- -D warnings` also pass. One model-startup test hit its existing time limit during an earlier busy run; its isolated retry and the complete clean rerun both passed. GPU-rendered river, lake and pond previews were inspected, including a lake with temporal AA enabled. Behavioral coverage includes deterministic generation across seeds and sampling orders, negative-coordinate chunk seams, dry spawn, bounded basin depths, fluid-preserving LOD reduction, authoritative neighbor meshing, rain impacts, targeting through water and unchanged ground movement. A real nonblocking loopback client negotiates a package-defined fluid catalog and verifies the edit survives a save/restart.

Headless benchmarks on Apple M1 Pro / Metal, 1280×720, radius 6, 300 steady frames, medium sun shadows, temporal AA off:

| Scene | CPU steady median | GPU steady median | Near setup | Near mesh bytes |
| --- | ---: | ---: | ---: | ---: |
| Before water, voxel | 2.563 ms | 3.667 ms | 2657.2 ms | 21,536,856 |
| With water, voxel | 2.802 ms | 3.701 ms | 2878.4 ms | 20,754,456 |
| With water, bounced | 3.076 ms | 3.732 ms | 3221.4 ms | 20,990,472 |
| Before water, 512 m LOD | 3.852 ms | 4.160 ms | 2670.8 ms | 21,536,856 |
| With water, 512 m LOD | 2.931 ms | 4.236 ms | 2894.9 ms | 20,754,456 |

At 512 m, all 77 requested LOD tiles are available. Summary generation changes from 971.64 to 963.54 ms, meshing from 305.03 to 317.54 ms, summary storage from 3,322,970 to 3,369,170 bytes and LOD mesh storage from 58,653,296 to 58,075,992 bytes. Water preserves material boundaries within the existing transport budget.

These are scene comparisons rather than isolated water-pass measurements: wet terrain removes some trees, changes geometry and adds the transparent pass to the benchmark. CPU samples vary, and a short test retry overlapped the near-water run. GPU medians remain close to the previous scene. Setup is measured separately; frame samples exclude live gameplay, GPU presentation and networking.
