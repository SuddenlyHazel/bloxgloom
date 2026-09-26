# Kiln: the shared anchored-entity path

The Kiln is a two-block workstation with three server-owned slots: fuel, input,
and output. The initial recipe converts gravel to stone in four scheduled pulses.
Its public screen shows slots, remaining burn time, and recipe-relative progress.

## Shared path

1. Startup registers its catalog identity, private codec, interaction policy,
   and tick planner through `EntityTypeRegistryBuilder`.
2. Placement atomically consumes one item, creates the anchored entity and both
   footprint blocks. Either half resolves to that same entity, including seams.
3. Screen clicks use `EntityInteract`, not a second inventory protocol. Version-2
   requests include entity identity and revision so an outdated open screen cannot
   modify a replacement Kiln or overwrite a concurrent change. All 36 inventory
   slots are supported; legacy keyboard requests still work.
4. The coordinator validates reach and the target, plans the inventory/entity
   transaction, reserves dependencies, and submits it to the shared journal.
   Nothing becomes visible before the durable receipt. Retries are idempotent.
5. Cooking runs in the existing entity worker dispatch against immutable input.
   Its plan changes private state and lit block states in the same transaction.
   Output capacity, fuel use, and stack conservation stay server-owned.
6. Normal committed entity snapshots/deltas carry a bounded 24-byte public
   workstation summary. Nearby players see slot item IDs/counts and cooking
   status; arbitrary item components remain private. The UI never predicts item
   ownership. Its geometry cache includes workstation state so ticks repaint.
7. Recovery restores slots, fuel, progress, and due work. The shared journal
   clock ensures cooking resumes after restart. Breaking either half atomically
   removes the complete footprint and turns the stored stacks into world drops.

The client closes the screen when the target disappears, changes identity,
leaves reach, or the connection closes. Rejected actions use existing status
receipts. Local source selection is cosmetic and never removes an item.

## Content and checks

Kiln entity schema 2 expands public presentation; the new default world folder
is `world-v8`. The masonry and dark/lit vents are original generated assets in
`assets/textures/blocks/`, reproducible with `source/kiln.py`.

Tests exercise backpack transfers and stale-target rejection, public payload
bounds/privacy, both-half UI resolution, compact layout hit testing, and the
durable place/load/cook/restart/collect/retry/break flow across a vertical seam.
The existing full-output and fuel-consumption tests remain in place.
The full suite passed **627 tests**, with clean formatting and strict all-target,
all-feature Clippy.

The inspected offscreen previews use the production UI and material renderer;
they are not a live gameplay recording. A single `perf 300 6` comparison against
`7f9fdf2` kept mesh size at 17,292,744 bytes and visible triangles at 88,026.
Scene setup was 1768.0 → 1723.9 ms, steady CPU median 0.331 → 0.321 ms, and
steady GPU median 0.280 → 0.293 ms. This terrain scene does not measure active
Kiln simulation or multiplayer inventory contention.
