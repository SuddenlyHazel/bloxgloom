# Registered anchored behavior

`Registrar::anchored_block_entity` registers a general own-state anchored block
entity. It is independent of the host-owned storage and machine inventory codecs;
it does not give extension payloads authority to create inventory stacks.
`extensions/lifecycle-fixture/src/anchored.rs` is the separately compiled example.

## Public contract

`AnchoredBlockEntity` declares namespaced block/item/entity identities, one anchor
state, a fixed footprint, placement cost and maximum removal refund, codec and
projection limits, polling interval, observed relative terrain cells, and a
`Behavior`. Startup validates references, bounds and exclusive block ownership.
The host allocates IDs. Declarative metadata and the callback schema fingerprint
participate in content compatibility; manifest remapping rebinds the behavior by
entity key before recovery. Change the callback fingerprint when semantics change.

Callbacks are deterministic, side-effect-free, trusted native Rust:

- `initialize(anchor)` supplies custom decoded initial state.
- `encode` / `decode` persist it; decoding must reproduce canonical bytes.
- `public` supplies bounded opaque client projection bytes.
- `interact(state, request)` replaces only this entity's private state.
- `react(context)` sees the complete declared terrain-cell set and returns Keep,
  Update, or Remove. This is a level-triggered neighbor/support check, not an
  exactly-once event stream. Do not advance timers by counting wake callbacks.
- `refund(state, cause, maximum)` can reduce the declared refund on player break,
  reactive removal, or system invalidation. It cannot increase the budget.

The selected component-free hotbar stack pays 1–128 units atomically with the
entire placement. Refunds are 0–cost units of that same item. This conserves finite
items even when a callback returns dishonest refund quantities. Multi-item or
component-dependent recipes belong to inventory/process APIs, not opaque state.
Breaking any footprint cell removes the whole footprint and refunds exactly once.

The generic EntityInteract payload from `anchored::interaction_request` carries
tag 4, entity ID, expected revision, and 1–239 request bytes. Server dispatch checks
reach, interest, current footprint ownership, identity, revision and the registered
handler. `definition.interaction` describes the default use request for action/UI
discovery; action presentation remains the general-action dispatcher’s concern.
The server does not treat a visible action as authorization.

## Authority, scheduling and recovery

Placement/removal dispatch uses the shared workstation transaction assembler.
Own-state use uses the existing durable entity-interaction path. Reactions run on
the entity worker executor: the coordinator captures terrain, workers return
plans, and commit checks captured revisions and retains all terrain/entity read
dependencies through admission and receipt. No callback owns mutable server state.

The first poll and subsequent due polls use durable deadlines. A due reaction
schedules from the current tick, without replaying missed callbacks after restart.
Harmless early terrain wakes retain the existing deadline. Wakes only accelerate
checks: dropped hints, unavailable inputs and queue pressure do not erase the
ordinary due lane. Existing rotating bounded due/retry admission separates wakes
from ordinary jobs; worker completion order does not order authoritative commits.
The cursor is the existing coordinator cursor, reset on restart; deadlines and
entity identity survive restart. Unloading a chunk is never destruction.

Player edits to owned cells use registered lifecycle hooks. Generic edits without
an owning lifecycle fail closed. Entity reactions can clear only their complete
registered footprint. Fire’s existing source/frontier behavior is unchanged, but
its admission now expands footprint destruction before writing: burned cells,
cross-chunk footprint cleanup, despawns, exact refunds/contents, and fire
frontier/mailboxes/cursor share one WAL record. This covers passive containers and
machines as well as custom anchored entities. Accepted failures remain quarantined
until replay; unaccepted conflicts retain the original fire frontier for retry.
This is lifecycle integration, not the deferred fire-system API migration.

The shared footprint/refund assembly is also used by non-fire owner-system
`WorldEdit` proposals. One touched cell expands the complete registered footprint
before neighbor decisions; despawn, contents, refunds and owner progress enter the
same admission/receipt. Fire still has its separate bounded AIR-only invalidation
assembler. Committing producer progress separately from removal remains unsafe.

## Bounds and limits

- Footprint: 1–64 unique cells; every axis offset is -16..=16 and the anchor is
  explicit. Cells belong to the declared block type.
- Observations: at most 64 unique relative cells, also -16..=16. Worker capture is
  the fixed one-chunk halo (27 chunks) plus the footprint. Unavailable terrain is
  requested/deferred, never replaced with generated fallback or implicit air.
- Private bytes: at most 64 KiB. Public bytes: at most 4 KiB. Native callbacks are
  trusted to bound their own computation/allocation; a sandbox/instruction budget
  is still the separate runtime-loader task.
- Fire invalidation: at most 32 burned cells/unique entities, 2048 footprint
  cells and 1760 host refund/content stacks before preparation. Complete merge
  neighborhoods are preflighted against the existing bounded dependency capture;
  exact drop dependencies survive batch combination, admission and apply.
- The existing 1 MiB journal record, entity transaction, resident-chunk and local
  entity-page limits remain. Oversized fire cleanup is retained without partial
  burn/refund; retry alone cannot make a permanently oversized removal fit. Smaller
  local state/content is needed. This slice does not add multi-record destruction.
- Footprints are fixed, own-state interactions do not spend items, and opaque
  behavior is not composed into machine/storage private codecs. Scheduled machine
  inventory work continues through its separate conserving process contract.

No existing save/wire layouts changed. The new EntityInteract tag uses the existing
bounded opaque payload. Adding the fixture changes that development catalog’s
manifest, which is rejected rather than converted in incompatible fixture saves.

## Verification

Focused production tests cover cross-chunk placement, insufficient cost with no
debit, private initialization and public projection, duplicate/stale use, captured
neighbor invalidation, worker reactions, restart and durable support removal.
Another test burns two flammable cross-chunk footprints in one source record,
checks pre-receipt invisibility, exact refunds, restart and duplicate prevention.
The real nonblocking listener test uses production client replica assembly to
place/use/restart/break the independent fixture, including requests through its
non-anchor cell. No rendering algorithm or visual presentation changed.
