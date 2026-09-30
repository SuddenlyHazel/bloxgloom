# General anchored block entities

`register_anchored` binds the existing native own-state device lifecycle to Luau.
Storage and machine declarations remain available for their specialized inventory
and recipe behavior. The runnable [counter package](../../fixtures/anchored-counter/README.md)
exercises placement, state, public projection, interaction, reactions and refunds.

## Declaration

Declare `bloxgloom:content/v1` and `bloxgloom:anchored_entities/v1`, register the
block first, and pass an own-package entity identity and server/shared module:

```luau
h.register_anchored {
    entity = "counter:post", block = "counter:block", module = "counter:behavior",
    schema_version = 1, max_state_bytes = 9, max_public_bytes = 5, interval = 20,
    placement_cost = 3, removal_refund = 2,
    footprint = {
        {offset = {0,0,0}, state = "counter:block"},
        {offset = {0,1,0}, state = "counter:block"},
    },
    observe = {{0,-1,0}, {1,0,0}}, interaction = "increment",
}
```

Entity/block/module keys belong to the package. Eight anchored declarations per
package are allowed. A block has one storage, machine or general anchored owner;
conflicting declarations reject startup in either order, even through `pcall`.
Definitions install before the catalog freezes and before the world opens.

`placement_item` defaults to the block item; it must place exactly `anchor_state`,
which defaults to the registered block's placement state. Every footprint state
must be a legal state of that block. A footprint defaults to the anchor alone,
contains 1..64 distinct cells, and includes offset `{0,0,0}` in `anchor_state`.
Offsets are integer triples within ±16. `observe` defaults to an empty sequence
and permits up to 64 distinct offsets within ±16. Missing terrain is deferred,
never fabricated as air. The host checks coordinate overflow and occupancy.

`schema_version` is 1..65535, `max_state_bytes` 1..65536,
`max_public_bytes` 0..4096, and `interval` 1..4294967295 logical ticks.
Private state is variable-length binary bounded by the declaration; it may be
empty. `placement_cost` defaults to one and accepts 1..128 component-free units
from the selected hotbar stack. `removal_refund` defaults to the cost and accepts
0..cost. The callback may reduce this maximum, never increase it.

`interaction` defaults to binary `"activate"` and is at most 239 bytes. An empty
string disables the default USE action. Otherwise that action consumes one of
the existing eight discovered actions per target. Catalog and save identity
include all declaration metadata and the frozen source/dependency fingerprint.
Behavior edits follow current frozen-package save compatibility rules; no
schema converter or hot reload is provided by this binding.

## Callback

The declared module returns one pure function accepting `BloxAnchoredEvent`.
Its event, coordinate sequences and captured cell records are readonly.
It receives no mutable world, inventory, player or filesystem service. Standard
Luau libraries, deterministic attempt-local randomness and structured logs use
the same bounded runtime as other callbacks.

| Kind | Captured input | Required result |
| --- | --- | --- |
| `Initialize` | Anchor coordinates | Private binary string |
| `Validate` | Private `state` bytes | true/nil accepts; false or another type rejects |
| `Public` | Private `state` bytes | Explicit public binary string |
| `Interact` | Private state and opaque binary `request` | Replacement private binary string |
| `React` | State, anchor, exact logical `tick`, ordered `cells` with `offset`, legal state key and `solid` | nil keeps; `{state=bytes}` replaces; `{remove=true}` removes |
| `Refund` | State, `cause` (`Broken`, `Reaction`, `WorldEdit`) and `maximum` | Integer 0..maximum, or nil for maximum |

No arbitrary table serialization occurs. The private codec is exact binary
identity; optional `Validate` can enforce a package's own byte schema on proposed
and recovered values. Native round-trip validation and public projection can
invoke callbacks more than once. Each invocation gets a fresh bounded VM and
all deterministic random seed inputs come from its captured event. Logs describe
attempts, not receipts; callbacks must not perform external side effects.

A malformed/oversized reply, rejected byte schema, exception or runtime quota
failure rejects the candidate without publishing partial state, placement costs,
footprint edits or refunds. Refund decisions are retryable planners, not notices.

## Scheduling, authority and publication

Initialization and refund decisions use the existing bounded coordinator planning
path. Reactions and entity interactions use the native worker/planning boundaries.
The server validates reach, interest, exact entity identity/revision, captured
terrain and every footprint cell before admitting changes to the WAL.

Reactions are level-triggered observations of current terrain. They run on durable
polling deadlines and advisory terrain wakes; an early wake preserves a future
deadline. A due poll schedules from current logical time. Do not count reaction
invocations as elapsed time or as a guaranteed history of every neighbor edit.
Explicit polling makes support/neighbor rules survive restart even if a wake is
lost. Unloading chunks or disconnecting a player is not device removal.

Placement debits the configured cost, installs the whole footprint and initializes
the entity in one transaction. Interactions mutate only its private state. A
manual break through any footprint cell, a `React` removal, or an authoritative
world/system invalidation clears the whole footprint, removes the entity and
emits its bounded refund together. Failed or stale plans do not partly apply.
Receipt replay does not repeat state changes, placement costs or refunds.

Public projections flow through the existing entity replica/presentation APIs;
private bytes stay server-side. Default USE targeting resolves secondary footprint
cells to the same entity and carries its exact identity/revision. Client bundles
contain closed inert V44 declaration metadata and matching catalog/action
identities, without automatically bundling server-only callback source. Explicitly
shared modules retain their declared visibility; clients never execute the anchored
behavior through this metadata. The existing
`presentation_anchors` window can read owned public bytes for authored UI or
visuals; this binding does not create a general scene renderer.

## Example and checks

The counter stores a private count, neighbor signal and original anchor height.
Only count/signal are public. Interaction increments the count; reactions project
an observed neighbor and remove the device when support disappears. It costs
three block items and refunds at most two, preserving the configured economy.

The real nonblocking-listener tests in
[`script_startup/anchored.rs`](../../src/server/net/tests/script_startup/anchored.rs)
use isolated saves, production secondary-cell interaction, replay, private/public
projection, neighbor edits, support removal, restart and held-plus-dropped item
accounting. Adjacent failure tests cover capability/registration rejection and
callback rollback. Client codec tests verify bounds, malformed artifacts and
catalog identity. A block preview uses the production lighting/meshing pipeline;
it verifies appearance while the loopback tests verify runtime behavior.
