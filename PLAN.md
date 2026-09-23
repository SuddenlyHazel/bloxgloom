# Bloxgloom architecture plan

Status: **implementation baseline**. The decisions below are initial targets; measured results may justify changes.

## Goal and constraints

Build a Rust, Minecraft-like voxel game with multiplayer from the start, a large procedural world, persistent block edits, and a 60 FPS target on a modest desktop GPU. `wgpu` is the renderer, `winit` handles the desktop window and input, and `glam` supplies math types.

The repository currently contains only a Cargo starter and these dependencies. No game systems or renderer exist yet.

The 60 FPS target gives each frame about 16.7 ms. Frame-time percentiles, memory use, chunk throughput, and network traffic should be measured throughout development. Chunk dimensions, view distance, compression, and GPU batching are hypotheses until benchmarked on target hardware.

## Architecture options

| | A: Server sends chunk snapshots and edits | B: Client generates base terrain and receives edits |
| --- | --- | --- |
| World data flow | Server generates or loads chunks, sends snapshots on entry, then versioned deltas | Server sends seed and generator version; client generates chunks and applies server edits |
| Authority | Server owns simulation, collision, and edits | Server still owns simulation and edits, but clients reproduce terrain |
| Strength | One canonical block state and straightforward resynchronization | Less terrain data sent for unexplored, unedited areas |
| Cost | More server work and exploration bandwidth | Generator compatibility, extra client CPU work, harder mismatch recovery |
| Main failure mode | Slow chunk delivery under load | Clients disagree about terrain after a generator change or bug |

**Recommendation: A.** It keeps the multiplayer consistency model simple while preserving deterministic server generation. Measure bandwidth and server throughput before considering B as an optimization. Both options still need interest management, bounded work queues, and chunk versioning.

## Proposed system boundaries

- **Shared world model:** integer world coordinates, chunk keys, block identifiers, chunk versions, and edit operations. Keep data usable without graphics or a window. Establish rules for negative coordinates and edits at chunk boundaries.
- **Server:** authoritative player simulation, collision, terrain generation, edit validation, persistence, and per-player chunk interest. It sends chunk snapshots and ordered edit deltas. A reconnect or version gap is repaired with a fresh snapshot.
- **Client world cache:** stores authoritative chunk data separately from predicted local movement. Applying a server delta advances the expected chunk version; stale or missing data requests resynchronization.
- **Chunk work pipeline:** generation/loading, lighting if needed, meshing, and upload are distinct stages. Workers operate on stable chunk snapshots; a result carries its source version and is dropped if obsolete. Work is prioritized near the player and bounded so rapid movement cannot grow queues indefinitely.
- **Renderer:** one mesh per chunk or chunk section, rather than one draw per block. Begin with hidden-face removal and greedy meshing for opaque blocks, a separate transparent path, depth testing, and frustum culling. Budget GPU uploads per frame. Optional GPU features may improve batching later; the baseline must run without them.
- **Storage:** generated terrain and player edits have separate ownership. Use a versioned format with a recovery strategy for interrupted writes. Define when the server considers an edit durable before promising that an acknowledgment means it is saved.

The server is the only authority for shared world changes. The client may predict its own movement for responsiveness, then reconcile with server state. The render thread must never wait for terrain generation, disk I/O, networking, or full-chunk meshing.

## Performance and correctness rules

- Benchmark candidate chunk dimensions and block representations using realistic terrain, caves, and edits before fixing them. Compare memory per loaded chunk, meshing time, edit remesh cost, draw count, and network payload size.
- Keep chunk data compact and separate CPU world storage from GPU mesh storage. Avoid allocating or drawing per block in the hot path.
- Make queue depth, chunk priorities, memory budgets, and upload budgets explicit. Handle cancellation and stale results when players move or blocks change.
- Test the invariants most likely to fail: coordinate conversion, edits across chunk boundaries, version gaps, persistence recovery, and stale mesh rejection. Add load and frame-time checks where they reveal real regressions.
- Record at least frame-time percentiles, mesh and upload timings, queued jobs, loaded chunks, bytes per client, and server tick time. Tune against the agreed player count, view distance, and target machine.

## Delivery sequence

1. **Contract and benchmark foundation.** Define the world coordinate and chunk contracts, snapshot/delta protocol, and edit ordering. Build representative data benchmarks for chunk layouts and meshing. Decide chunk dimensions from those results.
2. **Authoritative world loop.** Build a headless server and client world cache with procedural chunks, versioned edits, interest management, and resynchronization. Verify two clients observe the same edits, including at chunk boundaries.
3. **Rendering pipeline.** Connect the client cache to a `winit`/`wgpu` renderer. Implement chunk meshing, culling, bounded uploads, and camera movement. Measure frame-time spikes during streaming and editing.
4. **Persistence and movement.** Persist edits with documented durability semantics; add server collision, client movement prediction, and reconciliation. Test reconnect and interrupted-save recovery.
5. **Scale tuning.** Profile exploration and editing with the agreed player count and view distance on target hardware. Tune storage, mesh layout, queue budgets, and transport from measurements. Consider client terrain generation only if chunk bandwidth is a demonstrated bottleneck.

The first playable milestone is two clients exploring and editing the same persistent terrain while chunk streaming and frame-time metrics are visible. It is not complete merely because blocks appear on screen.

## Initial product decisions

- **Deployment:** a dedicated native server for LAN/local play. The protocol should remain usable over TCP, but internet hosting, authentication, and hostile-client hardening are outside the first release.
- **Capacity:** design for 16 concurrent players. Begin with a three-chunk horizontal view radius and make it configurable within server-enforced limits. Measure a representative crowded scene as well as isolated exploration.
- **Performance target:** 60 FPS at 1280×720 on an Apple M1-class integrated GPU, with frame-time percentiles recorded during streaming and editing. This is a measurement target, not a claim of current performance.
- **Simulation scope:** procedural terrain, persistent block edits, player movement, and collision. Defer fluids, combat, complex entities, and dynamic lighting until the core world pipeline is measured and stable.
- **Durability:** an accepted edit acknowledgment means the edit reached durable storage. A failed write rejects the edit rather than showing a change that may disappear on restart.

Revisit view distance, chunk dimensions, capacity, and meshing strategy using measured results. Changes to world or protocol formats require explicit versioning and migration or rejection behavior.
