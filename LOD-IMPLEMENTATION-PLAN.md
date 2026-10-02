# Distant terrain LOD implementation plan

Status: Implemented. The five phases below are complete. See
[Distant terrain](docs/lod/README.md) for the shipped architecture, limits,
verification commands, measurements, and screenshots. This document retains
the original implementation sequence and explicitly deferred follow-up work.

Add a server-supplied distant terrain layer that extends the visible landscape
without extending full voxel simulation and replication to the same radius.
Keep existing chunks, lighting, and gameplay authority for the nearby world.
Use vertical terrain spans and progressively coarser horizontal tiles, following
the useful architectural ideas in Distant Horizons.

The integrated default is a configurable 512-block horizon. Builtin worlds also
support 1,024 blocks; worlds with generation contributors clamp to 512 blocks
while preserving their accurate generation fallback. Release benchmarks and
GPU previews verify both builtin horizons within the independent 128 MiB LOD
geometry budget. Public coarse contributor generation remains deferred.

## Existing integration points

The pre-LOD engine provided these integration foundations:

- [World generation](src/world/generation.rs) composes builtin terrain with
  registered contributors. Summaries must include this composition and edits.
- [Chunk loading](src/server/chunk_loader.rs) runs off the coordinator and
  accounts for newer committed snapshots that have not reached checkpoint files.
- [Streaming](src/server/streaming.rs) bounds loading and publication work.
  Distant terrain needs its own interest and budgets.
- [Meshing](src/render/mesh.rs) already greedily merges matching voxel faces.
  LOD must reduce terrain data and generation costs as well as triangle counts.
- [Client workers](src/client/workers.rs) and renderer uploads reject obsolete
  work and prioritize immediate edits. Preserve those guarantees.
- [Fog](src/render/fog.wgsl) originally blended to background between 38 and
  135 blocks. The distant layer needs configurable clear-weather fog distances.
- [Projection and visibility](src/render/visibility.rs) currently use a
  4,096-block far plane and fixed chunk bounds. Add tile bounds without
  extending the far plane unnecessarily.

## Scope and invariants

LOD is a visual approximation of server-owned terrain. It must never supply
collision, movement, block targeting, placement checks, inventory, drops, or
entity ownership. Its absence or staleness can affect the picture, not gameplay.

All terrain generation, reduction, cache I/O, network I/O, and CPU mesh building
run on workers. The window thread accepts bounded completions, selects ready
coverage, and submits bounded uploads and draws.

Summaries derive from committed world state. Never publish speculative edits or
regenerate only builtin terrain when contributors or saved edits can change it.
Keep procedural estimates confined to explicitly identified preview fixtures.

LOD work must not activate distant owner systems, tick entities, subscribe to
inventories, or pin the gameplay chunk cache across the entire horizon.
Use independent resource caps and give nearby gameplay precedence.

Initial scope includes opaque terrain, cliffs, overhangs, static block structures,
and simplified leaf canopies. Omit small decorative plants at distance. Moving
entities, drops, water transparency, custom animated materials, and distant
bounced lighting are deferred. Unsupported material appearances receive a
documented static fallback rather than invoking arbitrary material shaders.

## Data model and module boundaries

Use a horizontal tile key `(level, x, z)`. A proposed tile contains 32 by 32
columns. Level zero has one-block horizontal samples and a 32-block footprint;
each following level doubles sample width and tile footprint. Use Euclidean
division for negative coordinates and checked arithmetic for bounds.

Each column contains ordered, nonoverlapping vertical spans with bottom and
top coordinates, a catalog block-state identity, and approximate sky and emitted
light. Keep vertical coordinates independent of builtin terrain height limits:
contributors and players can build elsewhere. Start with explicit span structs;
only pack them after measuring memory and bandwidth. Do not inherit Minecraft's
bit widths or height limits.

Represent unknown coverage separately from known empty columns. Preserve
vertical coverage intervals so a tile built from nearby chunks does not imply
that all heights in that horizontal footprint have been examined. Initial
snapshots can populate partial summaries; publish broader coverage only after
its source is known.

Bound columns, spans, payload sizes, queues, and resident bytes. Keep accurate
base summaries where available and derive bounded render summaries from them.
When reduction must remove detail, use a deterministic geometric/material error
rule. Never silently truncate a column or turn unavailable data into air.

Suggested new modules:

| Location | Responsibility |
| --- | --- |
| `src/lod/` | Tile keys, bounds, coverage, spans, validation, deterministic reduction |
| `src/server/lod/` | Snapshot capture, workers, invalidation, summary cache, interest and publication |
| `src/protocol/lod.rs` | Negotiation, bounded request and response encoding |
| `src/client/lod/` | Tile replicas, scheduling, worker revisions, cancellation |
| `src/render/lod/` | Selection, boundary geometry, GPU resources, shader and draw submission |
| `src/preview/lod.rs` | Deterministic visual scenes and preview entry point |
| `src/preview/perf.rs` | Existing benchmark integration and separate LOD measurements |

Keep tests adjacent to each focused module. Existing large entry modules should
only wire these components together.

## Phase 1 Establish summaries and reduction

1. Define keys, world bounds, span/coverage types, and validation. Document the
   distinction between known empty and unavailable terrain.
2. Extract spans from immutable authoritative chunk snapshots, including
   catalog material classification. Join matching spans across vertical chunk
   seams only when both source intervals are known.
3. Build parent summaries by combining four child columns. Define deterministic
   occupancy, material, and light selection, retaining prominent surfaces and
   gaps at the nearest LOD levels. Track geometric error for later selection.
4. Keep source revisions separate from rendering revisions. Define a tile
   generation counter that changes when any dependency becomes obsolete.
5. Establish exact caps for base data, render spans, and encoded tiles from
   fixture measurements before admitting them to the protocol.

Acceptance: fixtures cover negative coordinates, vertical chunk boundaries,
empty and missing columns, a bridge with open space beneath it, trees, cliffs,
and sealed cavities. Repeated reduction produces identical results. Invalid
coordinates, spans, identities, and excessive inputs are rejected or handled
by the documented reduction policy.

Suggested commit: `feat(lod): add terrain summaries and deterministic reduction`.

## Phase 2 Build and inspect the renderer

1. Mesh visible span surfaces with tile bounds and neighbor information. Add
   explicit handling for coarse/fine boundaries. Prefer boundary splitting or
   stitching; restrict any skirts to exterior terrain seams so they cannot
   close cave openings or bridge gaps indiscriminately.
2. Prepare a static face-color table from catalog textures off the window thread.
   Average colors in linear space and account for cutout alpha. Catalog swatches
   are an initial fallback. Keep color/material preparation separate from world
   summaries so appearance changes need not regenerate server terrain data.
3. Add a compact distant shader using normals, approximate light, dynamic
   daylight, and shared weather scattering. Do not default unknown lighting to
   full skylight. Preserve darkness in covered spaces at retained detail levels.
4. Draw into the existing HDR scene/depth pipeline. Use camera-relative positions
   for distant geometry and verify agreement with the existing camera transform.
   Start with the current depth format and far plane; measure before considering
   reversed depth or a separate distant depth pass.
5. Add a `lod-preview` entry point with fixed seeds, cameras, and mixed levels.
   Make clear-weather fog range configurable without removing storm scattering
   or indoor shelter behavior.

Acceptance: inspect generated images of mixed-detail cliffs, forest silhouettes,
bridges, cave mouths, and day/night/weather views. Check seams, winding, shading,
depth occlusion, and precision. Measure summary construction, mesh bytes,
triangles, draw count, CPU frame time, and GPU frame time separately.

This phase uses preview data only. It is not complete gameplay integration.

Suggested commit: `feat(render): add distant terrain meshes and visual previews`.

## Phase 3 Generate authoritative distant summaries

1. Capture resident immutable chunks and their revision dependencies without
   holding gameplay cache pins for the duration of LOD work.
2. For uncached terrain, use a separate bounded worker path that applies the
   installed generator composition and saved edits. Reuse authoritative loader
   semantics for pending committed snapshots and revision checks; reading only
   checkpoint files is insufficient while checkpointing is behind.
3. Define how requested vertical coverage is discovered. Builtin generation has
   known bounds, but extensions and edits must not be constrained to them. Add
   a conservative server coverage contract/index as needed; if coverage cannot
   be established, return partial or unavailable data explicitly. Never claim a
   complete column after examining only the player's vertical streaming band.
4. Generate and reduce without installing distant chunks into active simulation.
   Share summary builds across players requesting the same tile.
5. Hook invalidation to committed block publication. Invalidate affected base
   tiles, ancestors, and boundary-dependent neighbor meshes. Coalesce repeated
   edits and reject builds whose captured dependencies changed.
6. Separate cheap resident extraction from expensive distant generation. Bound
   concurrency and bytes, cancel abandoned work, and expose queue age and
   generation cost in telemetry.

Acceptance: edits during construction never install superseded results; recent
uncheckpointed edits appear in summaries; contributor terrain and high player
structures survive. Distant requests do not increase active entity counts or
exhaust near-chunk loading/cache capacity.

Suggested commit: `feat(server): build and invalidate authoritative LOD tiles`.

## Phase 4 Stream tiles and transition between ready meshes

1. Add LOD capability/version negotiation and server-clamped horizon settings
   independently of full chunk view distance. Follow existing protocol version
   rules when wire compatibility changes.
2. Define bounded tile requests, responses, invalidations, and unavailable
   results. Include world/session identity, key, revision, and coverage. Validate
   decoded lengths and allocation bounds before constructing data structures.
3. Prioritize coarse tiles that fill missing skyline, then refinements near the
   full chunk boundary. Give edits and gameplay traffic precedence and cap LOD
   bulk admission so already queued packets cannot create a long gameplay delay.
4. Add client replicas and dedicated bounded mesh scheduling. Fence results by
   world session, tile revision, requested generation, and material revision.
   Teleports, disconnects, and world changes retire obsolete work and resources.
5. Select levels by distance initially, with hysteresis. Keep a ready parent until
   all required children are uploaded. Never draw parent and child over the same
   coverage unless using an intentional, tested transition technique.
6. Mask LOD against actual ready near meshes using three-dimensional coverage.
   Keep LOD visible where near meshes are still pending or vertical chunks are
   absent. A received chunk or configured radius alone cannot remove coverage.
   Remeshing an edited near chunk must retain its existing displayed mesh until
   replacement, consistent with current behavior.
7. Bound pending uploads and GPU residency separately from near meshes, with
   near work taking precedence. Keep installation and eviction coherent so
   resource eviction cannot leave a selected tile without drawable coverage.
8. Add enabled/horizon/quality controls and diagnostics through focused config
   and UI modules. Disabled LOD retains existing play with a matching server;
   older wire versions follow the normal handshake rejection rule.

Acceptance: use the real nonblocking listener with loopback clients and a unique
temporary world. Cover initial join, rapid movement, teleport, slow receipt,
edits during upload, multiple players, reconnect, and server pressure. Verify
bounded queues and no terrain disappearance during parent/child replacement.
Inspect the release client window when available, plus deterministic previews.

Suggested commit: `feat(lod): stream distant tiles and preserve ready coverage`.

## Phase 5 Cache and measure the integrated feature

1. Persist server summaries as disposable derived caches, keyed by world
   identity, generator identity, catalog identity, summary format, and relevant
   committed source revisions. Use atomic replacement. Startup must detect
   summaries made stale by a crash between world commit and cache invalidation;
   use durable source stamps or conservatively rebuild affected cache data.
2. Apply disk and memory limits with eviction. Missing, stale, or corrupt caches
   rebuild from authoritative state and never alter save data. Client disk
   caching is optional and should follow demonstrated network demand.
3. Extend the benchmark with comparable near-only and near-plus-LOD scenes at
   512 and 1,024 blocks. Report cold construction, warm load, reduction,
   bandwidth, CPU/GPU memory, uploads, draw counts, and steady frame times.
4. Measure live startup, movement, edit-to-visible latency, and teleport recovery
   separately from the headless benchmark. Compare with the same near radius,
   camera, lighting mode, and machine. Set final default caps from these results.
5. Increase the horizon only after near gameplay remains responsive and LOD
   resources remain bounded under sustained exploration.

Suggested commit: `perf(lod): cache terrain summaries and add integrated metrics`.

## Later generation optimization

Full generation followed by reduction is an accurate fallback, but does not
eliminate the cost of generating every source voxel in unexplored terrain.
Once measurements identify that bottleneck, add an optional coarse-summary
contract to registered generators. It must describe coverage and error and
compose in the same ordering as ordinary generation. Apply saved edits after
generation and ensure coarse terrain converges to actual nearby terrain.

The builtin generator now samples terrain and vegetation directly at the
requested scale through an internal coarse-summary contract. Contributors
without a public contract retain full-generation fallback or explicit
unavailable results when resource limits prevent it.
Document the Rust and Luau authoring surfaces if this contract becomes public.
Do not bypass extensions merely to make builtin benchmarks faster.

Screen-space error selection, further geometry compression, and distant
transparency can follow measured need. They are not first-release requirements.

## Verification and completion criteria

Run the repository checks for implementation changes:

```sh
cargo test
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo run --release -- perf 300 6
cargo run --release -- perf 300 6 bounced
```

Add LOD-specific preview and benchmark invocations when their entry points exist.
Use unique temporary save directories for networking tests and benchmarks.
Inspect image outputs after rendering changes; compilation is not visual proof.
Run `graphify update .` after modifying code.

The feature is complete when the integrated server/client supports bounded
distant terrain, accurate committed-edit invalidation, contributor-aware
generation, seamless ready-mesh coverage, and cancellation/eviction across
sessions. Record screenshots and measured baseline comparisons. Inventory,
movement, and world authority remain independent of LOD. If authoritative world
formats become incompatible, increment the world folder target rather than
adding converters; disposable summary caches need only a cache format revision.

## Distant Horizons references

These sources informed the design; the module layout, targets, and phases above
are proposals for Bloxgloom rather than claims about Distant Horizons.

- [Project](https://gitlab.com/distant-horizons-team/distant-horizons)
- [Vertical span representation](https://gitlab.com/distant-horizons-team/distant-horizons-core/-/blob/0cf1aadae7f7819904d494f06950dc6f8b560413/core/src/main/java/com/seibel/distanthorizons/core/util/FullDataPointUtil.java)
- [Hierarchical data aggregation](https://gitlab.com/distant-horizons-team/distant-horizons-core/-/blob/0cf1aadae7f7819904d494f06950dc6f8b560413/core/src/main/java/com/seibel/distanthorizons/core/dataObjects/fullData/sources/FullDataSourceV2.java)
- [Selection and parent fallback](https://gitlab.com/distant-horizons-team/distant-horizons-core/-/blob/0cf1aadae7f7819904d494f06950dc6f8b560413/core/src/main/java/com/seibel/distanthorizons/core/render/QuadTree/LodQuadTree.java)
- [Render color preparation](https://distant-horizons-team.gitlab.io/distant-horizons/com/seibel/distanthorizons/api/interfaces/render/IDhApiRenderProxy.html)
