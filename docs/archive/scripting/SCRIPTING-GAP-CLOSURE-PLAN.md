# Scripting Gap Closure Plan

> **Archived September 30, 2026.** Historical snapshot, not an active plan or
> current API reference. Status, limits and instructions below reflect the time
> of writing. Start at [current modding docs](../../../docs/modding/README.md),
> [Phase 8 acceptance](../../../docs/modding/PHASE-8-ACCEPTANCE.md) and
> [current Luau gaps](../../../LUAU-SCRIPTING-GAPS.md).

Status: Proposed implementation roadmap. This document plans the work requested
in the scripting gap review; it does not mark features implemented or authorize
starting implementation. The current surface is described in
[SCRIPTING.md](../../../SCRIPTING.md). The existing
[modding implementation plan](../modding/IMPLEMENTATION-PLAN.md) remains the
progress record for work already underway.

The goal is to let mod developers build substantial gameplay and presentation
without recompiling Bloxgloom. Remove arbitrary authoring ceilings, expose
composable engine operations, and provide useful development tools. Retain
server ownership and transaction correctness while making those mechanisms
available through a capable public API.

## Design requirements

- Give built-in gameplay and scripts the same public primitives whenever they
  perform the same operation. Specialized storage, machine and creature APIs
  should remain convenient compositions of those primitives.
- Keep authoritative player, inventory, drop, entity and world changes on the
  server. Expose operations with explicit outcomes and receipts.
- Preserve stable profile ownership, the 128-item stack cap, exact component
  bytes and conservation during transfers. Item creation must be an explicit
  operation or registered transformation.
- Keep catalog registration frozen during a live world/session. Use versioned
  identities for new content, assets and host contracts; never recycle built-in
  numeric IDs.
- Keep I/O, compilation, simulation and expensive preparation off the window
  thread. Preserve revision checks for worker results and edited chunk seams.
- Make execution and resource budgets configurable and measurable. Bound total
  work rather than forcing developers into tiny packages or toy callbacks.
- Keep scripts deterministic where world generation or replay requires it.
  Supply time and randomness through explicit host contracts.
- Follow the prerelease save policy in [AGENTS.md](../../../AGENTS.md): increment the
  default world-folder version for incompatible changes. Do not build world or
  entity-schema converters in this work.

## Coverage and implementation order

The milestones below close all 15 gaps from the review. Ordering is a proposed
dependency sequence, not a delivery estimate. New engine features such as audio,
imported models and liquids require their engine mechanisms as well as bindings;
they cannot be completed by adding Luau methods alone.

| Milestone | Gaps closed | Depends on |
| --- | --- | --- |
| 1 Authoring foundations | Multiple registrations, configurable budgets, structured state and initial diagnostics | Existing host contracts |
| 2 Gameplay and native parity | Gameplay control, lifecycle hooks and missing native bindings | 1 |
| 3 World jobs and observations | World-scale jobs and richer client observations | 1 and relevant services from 2 |
| 4 Dynamic UI | Runtime widgets and missing controls | 1 and structured observations from 3 |
| 5 Audio | Sounds, spatial playback and music | Asset delivery and event contracts from 2 |
| 6 Models and world geometry | Imported models, animation, richer shapes, transparency and liquids | 2 and relevant world jobs from 3 |
| 7 Shader expressiveness | Bounded loops/arrays and additional rendering contracts | 1; geometry-specific additions follow 6 |
| 8 Development workflow | Logging, debugging, profiling and reload | 1, then each implemented surface |
| 9 External integration | Optional package storage and HTTP | 1 and asynchronous service infrastructure |
| 10 Integrated acceptance | Realistic packages using the completed capabilities together | 1 through 9 |

Imported models were deferred in the earlier plan. Milestone 6 records their
proposed expansion here without changing that earlier plan's approved status.
Reload also requires an explicit compatibility design; it must not silently
invalidate existing schema or catalog protections.

## Milestone 1 Authoring foundations

Replace singleton action, generator and owner-system storage with collections
keyed by namespaced identity. Validate each registration independently and keep
deterministic ordering, duplicate-owner rejection, negotiated metadata and save
identity. Do not require splitting one mod into several packages to add actions.

Introduce a server-configured budget policy covering VM memory, execution work,
callbacks per tick, outstanding jobs, captured data and delivered assets.
Packages may declare resource needs; the server resolves them against its policy
and reports the effective limits. Separate gameplay, startup, generation and
presentation policies. Keep finite defaults and backpressure; do not prescribe
new numeric ceilings until representative workloads have been measured.

Add structured state helpers over explicit persisted bytes: supported scalar,
string, sequence and record values; deterministic encoding; schema version and
validation errors. Preserve binary-state access for authors who need it. Provide
documented schema compatibility rules without prerelease converters.

Add package/module/callback-attributed logging, timing and budget diagnostics
early so every later milestone can use them. Logs need bounded buffering and
rate accounting rather than native stdout access from simulation workers.

Acceptance:

- One fixture package registers multiple actions, generators and systems and
  survives join/restart with deterministic identities and ordering.
- A representative workload runs under an adjusted server policy; exhaustion
  reports the resource, effective limit and responsible callback.
- Structured state round-trips through persistence, preserving binary strings
  and exact host identities where serialization is supported. Malformed state
  fails before applying effects; unsupported handle serialization is explicit.
- Runtime bindings, editor types and author documentation describe the same
  public methods and limits.

## Milestone 2 Gameplay control and native parity

Add authoritative player services for teleportation, movement modifiers,
attributes, health/damage/death and respawn as those mechanics are introduced.
Expose stable actor/profile handles with explicit access rules. Apply position
changes through collision/spawn admission and replicate the result so prediction
and camera state reconcile. Cosmetic model selection belongs to the appearance
contract; collision changes belong to the negotiated body contract.

Generalize entity services to cover supported motion, lifecycle, interactions
and components. Bind the native anchored-behavior and item-icon contracts to
Luau, including their verified client metadata. Keep storage/machines useful
without making them the only way to author an anchored object.

Add versioned events for player join/leave, inventory changes, interaction,
damage/death and crafting completion. Distinguish decision callbacks that can
propose transaction effects from notifications of already committed effects.
Define event ordering, retry semantics and durable delivery where required.
Prevent recursive event cascades from escaping scheduling/accounting limits.

Inventory, world, entity and scheduled effects must compose in one transaction
where their participants support it. Declare unsupported combinations explicitly
instead of letting a mod apply half an operation.

Acceptance:

- A fixture performs an authorized teleport, applies an attribute/movement
  modifier, handles player lifecycle events and reconciles a loopback client.
- An authored anchored object uses a Lua behavior and registered inventory icon
  without an engine-only key dispatch branch.
- An inventory-triggered behavior and entity lifecycle operation commit with
  their world effects; injected failures leave no partial gameplay outcome.
- Decision callbacks and committed notifications have demonstrated ordering;
  retries do not duplicate notifications or durable effects.
- Unsupported actor access and stale identity/revision requests are rejected
  with attributable errors.

## Milestone 3 World jobs and structured observations

Add asynchronous region/chunk access, paged queries, bulk edits and persistent
jobs that can span larger areas. Requests specify region, required data and
consistency. The host loads authoritative chunks with bounded memory, deadlines,
cancellation and fair scheduling; scripts never block a worker waiting for I/O.

Keep bounded snapshots for individual plans. Offer explicit conditional writes
and progress receipts for larger jobs. Define whether a batch commits atomically
or as resumable pages; do not promise world-wide atomic edits. Persist enough
job progress to restart without skipping or duplicating committed work.

Replace text-only inventory/block/action observations with versioned structured
records. Let client scripts select public entity subscriptions by region/type
with configurable pagination and interest limits. Expose membership changes,
revisions, overflow and resynchronization explicitly. Add package-scoped messages
for gameplay interactions that need data beyond current action arguments, using
typed payloads and authoritative handlers.

Acceptance:

- A durable world job crosses several chunks, reports progress, cancels cleanly
  and resumes after restart with exactly the recorded committed pages.
- Concurrent edits invalidate stale pages without overwriting newer world state.
- A client UI consumes structured inventory/action observations and subscribes
  to more than the current fixed replica window when server policy permits.
- Overload coalesces or resynchronizes according to the declared contract, with
  visible diagnostics and no unbounded queue or frame-thread work.
- Public subscriptions and messages expose only authorized data; private entity
  state remains server-owned.

## Milestone 4 Dynamic UI

Extend the existing authored UI renderer with stable node identities and atomic
tree updates: create, remove, replace, reorder and set layout/properties. Keep
the declarative document format as the initial tree and an easy authoring path.
Add sliders, checkboxes, dropdowns, tables/lists, multiline input and tooltips.
Provide focus, keyboard navigation, scrolling and virtualization for large lists.

Add an animation/timing API for presentation values and transitions. Preserve
document ownership and session retirement. Define state/focus behavior when a
node disappears, stale callbacks refer to old nodes or a virtualized row changes.
Game actions remain requests to authoritative handlers, with structured receipts.

Acceptance:

- One fixture builds and changes a widget tree, edits multiline text, uses every
  new control and shows a virtualized list from structured observations.
- Keyboard, clipboard, IME, focus and scrolling remain usable during updates.
- A rejected tree patch applies no partial changes; callbacks from a retired
  session cannot mutate the replacement UI.
- Inspect the live window or generated previews at several sizes. Measure large
  list/update behavior and confirm callback work stays off the window thread.

## Milestone 5 Audio

Build a package audio asset pipeline, decoding/streaming workers and mixer, then
bind playback to scripts. Support local and world-positioned sounds, attachment
to entities, looping, music, volume groups, fades, stop/query handles and player
audio settings. Select formats based on licensing, portability and runtime cost.

Separate local presentation sounds from server-originated gameplay cues.
Replicate committed cue events with identities so retries do not replay them.
Bound simultaneous voices, decoded memory and streaming buffers through policy.
Keep loading and decoding off the window thread.

Acceptance:

- A downloaded package plays a spatial entity sound and local UI sound, streams
  music and changes volume/fades without frame stalls.
- Reconnect, entity departure and session switch retire owned playback correctly.
- Duplicated gameplay deliveries do not duplicate cue playback; unsupported or
  malformed assets fail with package/resource attribution.

## Milestone 6 Models geometry and physics

Deliver this milestone in separate increments, each exposing its mechanism to
mods as soon as it exists:

1. Define a versioned model asset format/import pipeline with meshes, textures,
   materials, skeletons and animation clips. Support creature models first,
   followed by cosmetic player models with explicit body/collision contracts.
2. Add authored block meshes and collision/selection shapes. Compile validated
   shape data into catalog lookups used consistently by movement, ray targeting,
   placement, lighting and meshing. Preserve existing cube/foliage shorthands.
3. Add transparent material rendering with documented ordering, depth, lighting
   and selection behavior. Transparency is separate from liquid simulation.
4. Define liquid content/state and simulation primitives: sources, flow rules,
   displacement, interactions and persistence. Schedule authoritative flow jobs
   through the existing durable world mechanisms, then expose the rules to Luau.

Do not make arbitrary scripts execute per render vertex or collision test.
Compile reusable geometry/physics declarations and run behavior on simulation
workers. Maintain seam invalidation and both lighting modes, including dark
sealed caves. Measure model/shape/liquid scene costs separately.

Acceptance:

- A package downloads a skinned animated creature model and selects a custom
  player appearance; server collision and client prediction match their contracts.
- A partial-shape block has consistent collision, selection, placement and
  relighting across chunk seams.
- Transparent surfaces are visually inspected under overlapping geometry,
  fog and voxel/bounced lighting.
- Authored liquid rules flow across chunks, react to edits and survive restart;
  missing chunks defer simulation rather than using client fallback terrain.
- New declarations participate in save/catalog negotiation; malformed assets
  reject preparation with actionable diagnostics.

## Milestone 7 Shader and rendering expressiveness

Permit fixed-size arrays and bounded loops with validated maximum work. Add
declared texture/sampler inputs and useful frame data where a material/effect
requires them. Introduce versioned additional pass contracts for demonstrated
needs, such as depth-aware effects, with explicit input/output attachment
ownership, lifetime and dependency rules.

Keep renderer resource allocation and scheduling in the host. A script should
declare useful rendering work without reaching into device-specific internals.
Measure supported shader workloads and expose preparation/work budget errors.
Do not equate a CPU static-work estimate with a guaranteed GPU execution time.

Acceptance:

- A fixture uses an array and bounded loop and composes a new pass with existing
  materials/effects while respecting graph ownership and resize behavior.
- Excessive expanded work or invalid attachment dependencies reject preparation
  before the package becomes live.
- Inspect exported previews and compare setup time, mesh size, CPU frame time
  and GPU frame time individually for affected scenes.

## Milestone 8 Development workflow and reload

Expand foundation diagnostics into a developer console with searchable logs,
callback traces, source locations, timing/allocation summaries and job/event
inspection. Provide breakpoints and stepping in a development mode that defines
what simulation pauses; remote debugger access must be an explicit server option.
Deliver editor types and runnable examples alongside each API increment.

Implement reload in two stages. First reload presentation scripts/assets into
an isolated candidate and replace them only after successful preparation.
Then reload schema-compatible server behavior at a defined simulation boundary:
finish or retire old plans, retain explicit persisted state, switch source
generation and fence stale results. A callback must never mix old/new sources.

Resolve the current whole-installation source fingerprint deliberately.
Separate executable generation identity from declared persistent schema identity
where compatibility can be verified; retain rejection when it cannot. Content
registration or schema changes require a clean world/session restart and fresh
save when incompatible. Do not mutate the installed catalog or add converters
to make reload appear successful.

Acceptance:

- A mod author locates a failing callback, inspects its source and measures a
  budget problem without modifying engine source.
- Presentation reload preserves defined local state and rejects a bad candidate
  while keeping the prior session resources valid.
- A server behavior reload retains compatible explicit state and rejects stale
  plans; schema/content changes request the documented restart path.
- Debugging and reload have deterministic retirement/cancellation behavior and
  cannot leave old workers writing to replacement sessions.

## Milestone 9 Optional external integration

Add asynchronous package-scoped storage and HTTP services with explicit server
grants. Storage uses a separate namespaced location with quotas and atomic writes,
not direct access to world saves, profiles or arbitrary filesystem paths.
HTTP uses declared endpoints and server-controlled credentials, with timeouts,
response size limits, cancellation and attributed failures. Keep secrets out of
downloaded client/shared modules and assets.

Return results as scheduled inputs to scripts. External responses that influence
durable gameplay must become recorded inputs or explicit transactions; retries
must not repeat external side effects inadvertently. Support idempotency where
the remote service offers it and document failure/duplicate semantics otherwise.
Generation callbacks remain pure and cannot fetch network data.

Acceptance:

- A server package fetches granted external data and reads/writes its own storage
  without blocking simulation or touching another package's files.
- Denied endpoints, timeouts, malformed responses, quota exhaustion and server
  shutdown produce observable completion/cancellation outcomes.
- A reconnecting client cannot obtain server credentials, and a gameplay retry
  does not silently repeat an external request with side effects.

## Verification and completion

Implement each milestone as small reviewable slices with focused modules and
useful adjacent tests. Commit completed slices with messages describing their
behavior. Update [SCRIPTING.md](../../../SCRIPTING.md), types, examples and the existing
implementation progress record when the corresponding behavior actually ships.
Document proposals here without relabeling unfinished features as supported.

For code changes, run `cargo test`, `cargo fmt --all -- --check`, and
`cargo clippy --all-targets --all-features -- -D warnings`. Networking/lifecycle
changes must exercise the production nonblocking listener with loopback clients
and unique temporary saves, including join, reconnect, failure and restart.
Rendering/meshing changes require visual inspection and
`cargo run --release -- perf 300 6`, adding `bounced` when relevant. Compare scene
setup, mesh size, CPU time and GPU time separately; this benchmark excludes
live gameplay and presentation, which need their own measurements.

Use several focused fixtures plus one substantial integration package covering
multiple actions/systems, structured state, world jobs, player/entity behavior,
dynamic UI, audio, models and shaders. Demonstrate ordinary play while its work
runs, not just successful startup. Record responsiveness and resource usage
under realistic populations and sustained edits, with the effective budget
policy and reproducible scene/setup details.

Completion requires implemented runtime bindings, negotiated client metadata,
save identity where applicable, editor types, author documentation, an example,
and relevant correctness/visual/performance evidence for every gap. Any remaining
gap must remain explicitly open in the progress record. No release dates or
performance claims are implied by this roadmap.
