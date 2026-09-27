# Public dynamic-entity surface

The public `bloxgloom-host-api::entity` module exposes the current ground-creature
capabilities. Mossbun consumes the same declarations, behavior hooks, movement
services, and presentation contract as the independent Copperling fixture.

## Registration and identity

`Registrar::mobile_entity(MobileEntity { ... })` declares a namespaced type with:

- Explicit private schema version/fingerprint and private/public byte bounds.
- A body, initial tick interval, terrain read radius, nearby-entity subscription,
  and terrain-change wake policy.
- An immutable `Behavior` implementation for initial state, canonical encode/decode,
  public projection, tick planning, interaction state updates, and public pose.
- A bounded cuboid model, part animation roles, procedural animation parameters,
  and an optional opaque default right-click request.

The host assigns numeric IDs and binds declarations into the startup-frozen
catalog. Manifest remapping preserves the implementations under the negotiated
IDs. Body, scheduling, read bounds, model, animation, and interaction declarations
are included in compatibility fingerprints. Authors must change their explicit
schema fingerprint when changing codec/behavior semantics.

Registration validates initial codec roundtrips and projection bounds before
world storage opens. Extension installation remains atomic. Content IDs for
existing built-ins retain their identities.

## Behavior and movement

`Context` supplies entity ID, logical tick, durable next deadline, position,
immutable private payload, bounded public neighbours identified by namespaced
key, and the host's read-only `World` services. `random` provides deterministic
counter-based choices; hooks must not use wall-clock time or hidden mutable state.

The shared services expose solidity, body clearance/support, safe walking edges,
bounded local cardinal A*, and one fixed collision/gravity step. They use only
captured authoritative terrain. Missing/out-of-range reads reject the plan,
including out-of-range errors a callback tries to ignore. The movement service
uses the registered body and the captured entity position; a callback cannot
chain several steps or return an unchecked teleport.

`Plan` declares replacement state, a new deadline, optional host-produced motion,
and optional lifecycle effects. State and motion still pass through the existing
entity coordinator, revision fences, worker barriers, atomic WAL, and confirmed
publication. Crossing a chunk boundary uses the existing fenced owner transfer.

Lifecycle effects can spawn up to four registered mobile types and remove the
planning entity. Each spawn carries a namespaced type, initial payload, and nearby
position. The host checks supported clear terrain and schema bounds, then combines
allocation with the parent's update/removal into one transaction. A stale terrain
snapshot invalidates the whole operation. Removal cannot name unrelated entities.

## Interaction and presentation

The client intersects its aim ray with registered bodies, bounded by the nearest
voxel hit and ordinary reach. It builds one generic request containing entity ID,
revision, target cell, and the registered request bytes. The server independently
checks interest, reach, current identity/revision/location, and terrain occlusion
before invoking the type's interaction hook. Replayed actions use the existing
durable receipt ledger. The current interaction hook changes the creature's own
private state; item exchange and arbitrary event/verb menus are later surfaces.

The generic projection path validates registered public bytes and obtains yaw and
grounded state. Models are selected by negotiated entity identity, never by a
closed creature-kind list. The renderer builds bounded registered cuboid meshes
with per-part colors and body/left-foot/right-foot roles. Models share the bounded
instance buffer and use 32-bit mesh indices.

Client-only interpolation and procedural gait support registered stride rate and
amplitude, idle breathing rate/bob, walking bob, airborne stretch, and landing
squash. Foot roles and the existing body/ear-sway deformation are shared rendering
capabilities. Public state supplies authoritative position/grounded state; rendered
motion cannot decide simulation, collision, or interaction ownership.

Mossbun's behavior/state codec now lives in `src/content/creatures/mossbun.rs` and
imports only the public API and standard library. Its old server module is a
test-only compatibility helper. Generic host adapters live in
`src/server/entities/mobile/`; no production Mossbun dispatch remains in server
behavior, spawn requests, client projection, or the renderer.

## External proof: Copperling

`extensions/lifecycle-fixture/src/creature.rs` depends only on the public API.
Copperling is a smaller copper-colored creature with a short crest, a clockwise
square patrol, a quicker gait, and a right-click pause/resume toggle. Pausing
behavior does not disable gravity. Its patrol origin, phase, velocity, and pause
state persist privately; the bounded public projection exposes presentation state.

```sh
cargo run --release --features lifecycle-fixture
```

Use F4 on clear ground:

```text
spawn fixture:copperling
spawn bloxgloom:mossbun
```

Close the menu and right-click Copperling to pause or resume it. `spawn mossbun`
remains a shorthand. The same feature also installs `fixture:tall_store`.

Default saves are **`world-v12`** or **`world-v12-fixture`** for the development
fixture build. The catalog/presentation and generic spawn receipt/wire contracts
changed, so earlier worlds are not converted.

```sh
cargo run --release --features lifecycle-fixture -- creature-preview fixture:copperling copperling.png
cargo run --release -- creature-preview bloxgloom:mossbun mossbun.png
```

## Bounds and remaining surfaces

- At most 128 registered mobile models, each with 1–64 cuboids; 512 visible actor
  instances remain the rendering cap.
- Private/public state caps are at most 64 KiB/4 KiB per entity. Nearby capture
  remains complete-or-fail within the existing count/byte bounds.
- Read radius is 0–1 chunks. Bodies are 0.05–1.0 blocks in half-width,
  0.1–3.0 in height, and 0–4 blocks/second in walking speed. Spawn effects stay
  within eight blocks per axis and the captured terrain.
- Navigation remains bounded level-ground cardinal A*. Flying, jumping, climbing,
  skeletal animation, richer materials, and natural spawning are not existing
  capabilities newly introduced by this slice.
- `Payload` is an immutable shared type-erased value for this Rust host binding;
  durable/wire state is always explicitly encoded. These contracts do not select
  a future mod runtime or establish a native ABI. In-process hooks must obey the
  deterministic, side-effect-free contract; this is not sandbox isolation.
- Full capability parity still requires the remaining block/anchored lifecycle,
  recipe/filter/automation, player, item, world-system, and asset surfaces in the
  root plan.

## Verification

- **655 workspace/all-feature tests passed**, formatting passed, and strict
  all-target/all-feature Clippy passed.
- The real nonblocking listener test installs the external package, authorizes
  generic spawning, observes worker-driven movement through production client
  replica assembly, uses the actual client ray/click request path, verifies
  duplicate-action behavior, and restarts to recover the same ID and pause state.
- Spawn/self-removal tests stage a single atomic WAL transaction, fence stale
  terrain, reject a blocked child spawn without removing its parent, and recover
  parent removal/child creation together after reopening the world.
- Adapter tests reject unchecked movement and ignored out-of-range reads. Existing
  Mossbun movement, codec, worker/restart, remapping, interpolation, and production
  listener tests continue to pass through the migrated path.
- Release previews of Copperling and Mossbun were inspected. No live-window
  interaction recording is claimed.

One required `perf 300 6` terrain check, compared with the inventory-slice run:
scene setup **1697.5 → 1712.1 ms**; steady CPU median **0.319 → 0.318 ms**;
steady GPU median **0.286 → 0.282 ms**. Mesh bytes remain **17,292,744**, with
**88,026** visible triangles. This benchmark excludes presentation and live
creature populations; it does not measure creature scalability.
