# Creature movement and presentation

Mossbun is the first consumer of three reusable layers. Registration, immutable
worker capture, terrain revision fences, WAL receipts, and entity publication
remain the authority boundary.

## Server locomotion

`src/server/entities/locomotion.rs` owns an actor body's clearance/support checks,
safe ground edges, and fixed-step gravity. Bodies configure width, height, and
walking speed. All intersecting voxels are checked, including interior voxels
for bodies larger than a block. Ground locomotion requires full support;
physics allows partial support. Downward sweeps subdivide displacement and snap
to voxel tops, including at terminal velocity. Missing terrain rejects the plan.

The first locomotion mode is a level-ground walker: it cannot jump, climb,
swim, fly, or collide with other entities. Add those as explicit capabilities
with matching navigation edges, rather than allowing the planner to promise
movement its follower cannot execute.

Mossbun physics integrates 40 ms of logical time per movement step. The normal
due lane admits a one-tick deadline at InteractionCommit and executes it on the
following tick. Worker/admission pressure stretches wall time; it never causes
an unbounded catch-up sweep. Behavior chooses a destination independently.
During an AI pause, support rechecks remain due within ten ticks (plus admission
and execution latency), with terrain-edit hints providing faster interruption.
Repeated early hints cannot add airborne physics steps.

Startup resumes the shared clock from the greatest synced journal transaction
tick, rather than fire activity. Journal compaction retains this watermark as
internal base metadata, without introducing a shared gameplay conflict key.
This keeps persisted movement and AI deadlines on the same timeline after restart.

## Navigation

`src/server/entities/navigation.rs` implements deterministic cardinal A* with an
eight-cell Manhattan radius and at most 128 discovered/expanded nodes. The heap
is also bounded by the finite edge count. The graph uses the same body/edge
checks as locomotion, so body clearance, cliff avoidance, and intermediate
collision rules agree. Results distinguish arrival, a next waypoint,
unreachability, budget exhaustion, and missing terrain.

Mossbun persists its destination and current waypoint. It validates that edge
before moving and searches again at waypoint boundaries or after obstruction.
Searches run inside the existing immutable entity worker job; captured terrain
revisions fence the entire result before admission. No unfenced route cache or
coordinator-side path search is introduced. Goals that fail back off before the
next deterministic choice. This is local navigation, not a world-scale route
service or crowd-avoidance system.

Mossbun schema 2 persists velocity, grounded state, AI deadline, destination,
waypoint, and deterministic choice state in 41 bytes. Its two-byte public view
contains facing and movement/grounded flags. The default save folder is
`world-v7`; old entity schemas are not converted.

## Client presentation

`src/client/actors.rs` retains up to eight arrival-time samples for each of the
nearest 512 rendered actors. Rendering runs 80 ms behind sample arrival and
interpolates positions without extrapolating through unknown terrain. Long idle
gaps are anchored to a short segment when movement resumes. Large position
discontinuities (>4 blocks), model changes, removal, and visibility re-entry reset
history. The current wire protocol has no explicit teleport marker: smaller
discontinuities use ordinary interpolation. Packet stalls freeze presentation.

Continuous yaw and a per-actor animation clock run at frame rate. Mossbun's
distance-driven gait fades when rendered movement stops, even if the last
replicated intent still says walking. Ground contact is delayed with position so
landing squash does not play before landing is visible. Breathing, ear sway,
stride, airborne stretch, and squash are purely cosmetic; they never move the
server collision body or decide gameplay events.

The actor shader accepts a model pose (yaw, stride, bob, squash). Additional
procedural models can consume that presentation pipeline. Imported skeletal
assets and authored animation clips would require a new model renderer; no
asset/rig pipeline is implied by this implementation.

## Verification

Adjacent tests cover acceleration/landing, walls/cliffs/seams, larger bodies,
obstacle detours and replanning, bounded search, idle support checks, early wake
deduplication, schema/restart persistence, interpolation, stopping, discontinuity
resets, and delayed landing animation. `mossbun-motion-preview` generates six
offscreen frames using the production animator and shader. It is a deterministic
presentation fixture, not a recording of a live network session.

The full suite passed 621 tests, with clean formatting and strict all-target,
all-feature Clippy. Walking/falling/landing/resting preview frames were inspected.
A single before/after `perf 300 6` comparison against `ecb7479` on Apple M1 Pro
kept mesh size at 17,292,744 bytes and final visible triangles at 88,026. Scene
setup was 1757.5 → 1737.7 ms; steady CPU median 0.312 → 0.314 ms; steady GPU
median 0.292 → 0.285 ms. This terrain benchmark excludes actor rendering and
live gameplay, so those timings are not an NPC performance claim.
