# Physics consolidation

**Status: Parked — 2026-10-03.** These are proposed performance and maintenance
improvements, not active work or required scripting-gap closure. Resume when
requested. Vehicles remain parked separately.

The aim is to reduce repeated collision code and use Rapier where it improves
total cost, behavior or maintainability. No equivalent-workload comparison has
established that Rapier is faster than the current custom paths.

## Current implementation

- Opt-in moving rigid bodies use `rapier3d-f64` 0.36.0 for oriented cuboid
  contacts, friction, damping and rotation.
- Other moving entities use the custom swept-AABB solver, including exact
  first-contact pauses for durable scripted impact callbacks.
- Player collision uses direct voxel checks shared by server movement and client
  prediction. Creature locomotion has its own voxel collision and support checks.
- Falling drops use a bounded vertical column scan and stop ticking when settled.

The Rapier adapter reconstructs a bounded world for each body on every fixed
step. It recreates captured obstacles, disables sleeping and discards solver
warm-start caches. This keeps rejected simulation from becoming authoritative
and makes reconstruction from committed state reproducible, at a CPU cost.
Bodies respond to captured obstacles; there are no coupled moving-body islands,
reciprocal impulses into actors or joints. See the
[current motion contract](docs/modding/MOVING-ENTITIES.md#opt-in-rigid-bodies).

The existing isolated release probe on the M1 Pro measured 29.173 microseconds
per body with 64 captured colliders, equivalent to 1.867 ms for 64 such body
steps. It excludes terrain capture, persistence and client rendering. It is
neither a comparison with custom physics nor a coupled-world benchmark.

## Candidates when resumed

1. **Share terrain collision data.** Evaluate immutable collision representations
   cached by chunk revision, including Rapier native voxel shapes or compound
   shapes instead of individual block colliders. Measure construction, edit
   invalidation, memory and query costs. Preserve the contacted block identity
   needed by material selection and scripted impacts.
2. **Consolidate moving-entity collision.** This is the strongest candidate for
   replacing overlapping sweep, slide, bounce and contact resolution. Preserve
   legacy first-contact callback semantics before deleting the custom solver;
   the current rigid-body mode does not support those callbacks.
3. **Prototype a shared character collision backend.** Compare Rapier's
   character controller with current player and creature collision. Keep
   movement intentions, navigation support rules, gravity, jump eligibility and
   modifiers in their existing gameplay owners. Use the same backend and
   configuration for server movement and client prediction.
4. **Evaluate reusable Rapier worlds.** Reusing obstacle structures and solver
   state may reduce current setup overhead. Design speculative stepping,
   rejection, terrain revision changes and restart behavior before relying on
   retained state. A cached world must never advance authoritative state before
   durable receipt. Coupled-body stepping would also require a compatible atomic
   publication design.
5. **Retain simple drop falling unless evidence favors migration.** A short
   column scan may remain cheaper than general rigid-body simulation. Richer
   physical drop interactions would be a separate reason to reconsider it.

Rapier already provides [voxel and compound colliders](https://rapier.rs/docs/user_guides/rust/colliders/)
and a [character controller](https://rapier.rs/docs/user_guides/rust/character_controller/)
with sliding, stairs and ground snapping. The controller consumes a desired
movement vector; gravity and movement intentions still belong to the caller.
These features are available options, not implemented behavior in our current
character pipeline.

## Measure before migrating

Build a representative comparison of custom physics and Rapier with reusable
terrain data. Cover one and many actors, active and settled drops, fast
projectiles, sparse and dense terrain, edited support, chunk boundaries and
missing chunks. Include player prediction and correction under network delay.

Measure terrain capture, collider construction/update, collision queries,
solver steps and durable publication separately, plus total CPU p50/p95,
allocations and memory. Include cold setup and steady state. The renderer's
`perf 300 6` benchmark does not measure live physics or network prediction.

Acceptance requires preserved server authority, bounded work, no procedural
fallback terrain, correct edit invalidation, stale-result rejection, durable
impact behavior and restart handling. Exercise the real nonblocking listener
with an isolated save for networking changes, inspect movement in the release
client, and run relevant tests, formatting and strict Clippy.

Adopt a backend when measured cost or a concrete maintenance/behavior benefit
justifies it. Remove superseded collision implementations after their contracts
are covered. Shared terrain queries and contact resolution are the main DRY
opportunity; actor-specific gameplay policies still need distinct owners.
