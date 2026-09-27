# Hopper inventory automation

The Hopper is a one-block, downward-facing workstation with three slots (128
items per slot). It first tries to push one item into the inventory directly
below. If no push is possible, it tries to pull one item from directly above.
It visits slots in order and performs at most one transfer per 20-tick pulse.
Blocked or empty machines keep a bounded periodic recheck; freeing space
resumes flow. There is no loose-drop pickup or configurable facing in this first
version.

## Shared foundation

- `EntityTransferPolicy` is the inventory capability. Registered policies expose
  public offers and acceptance rules, plus trusted component-preserving
  withdrawal/deposit planners. The Hopper contains no Kiln-specific dispatch.
- `EntityView` attaches frozen policy handles to its bounded public neighbor
  projections. It never exposes neighboring private payloads to worker planners.
- Transfer intent now supports push as well as pull. The initiating machine's
  schedule advances; the peer's schedule is preserved. The coordinator resolves
  both private payloads, runs the registered hooks, and prepares one atomic
  batch with both revision preconditions. A stale endpoint invalidates the
  whole transfer before publication.
- The Kiln's port accepts fuel or registered recipe inputs and offers finished
  output only. The Hopper's port exposes all three slots. New inventory types
  can register the same capability.
- Placement and breaking use the shared anchored-workstation action path.
  Placement consumes one item; breaking refunds the Hopper and its contents in
  one durable transaction. The complete Hopper/Kiln chain survives restart.
- The private codec now uses independent BGCT container snapshots, with exactly
  three slots and no player-inventory revision or padding. Components are
  preserved privately. Public inventory summaries contain item IDs/counts,
  not component bytes. Trusted hooks always make the final fit decision.
- The existing workstation UI supports all 36 player inventory slots, exact
  one-item transfers, and entity ID/revision checks. Hopper slots are all
  bidirectional. The generic entity stream carries contents; no new network
  inventory ownership path was added.

New content uses stable block/item ID 300, state ID 600, entity ID 5, and entity
schema 2. Hopper initially used `world-v9`; registered-inventory content now makes the default `world-v11`.
No old-save conversion is provided.

## Verification

The full suite passed **635 tests**, with clean formatting and strict
all-target/all-feature Clippy.

Regressions cover a durable feed/cook/collect chain across negative coordinates
and a vertical chunk seam, restart at a late simulation tick, break refunds,
blocked destinations, concurrent destination changes rejecting whole pushes,
peer schedule preservation, private component round trips, and overflow refusal.
A real nonblocking-listener test feeds a Kiln through a Hopper while the player
moves and places blocks, applying the messages through the production client
replica assembler to catch resync loops.

The production offscreen renderer was used to inspect the chain and the Hopper
screen at 1280×720 and 640×360. The original metal/down-arrow textures are
reproducible with `assets/textures/blocks/source/hopper.py`.

One terrain benchmark comparison against `0b77dd7`, using `perf 300 6`:

| Mode | Setup ms, before → after | Steady CPU median ms | Steady GPU median ms | Mesh bytes |
| --- | --- | --- | --- | --- |
| Voxel | 1667.6 → 1742.5 | 0.316 → 0.311 | 0.234 → 0.286 | 17,292,744 unchanged |
| Bounced | 2002.2 → 2092.5 | 0.310 → 0.314 | 0.231 → 0.288 | 17,500,968 unchanged |

Visible triangles were unchanged at 88,026 / 89,218 respectively. These are
single-run observations; the terrain benchmark excludes active automation and
live presentation. It is not a Hopper throughput claim.
