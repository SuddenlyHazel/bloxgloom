# Player movement improvements

**Status: Parked — 2026-10-03.** These are proposed follow-ups, not active work.
Resume when requested. Start with airborne state, then jump responsiveness,
then movement audio. Creature behavior is a later, separate task.

The goal is to make walking, sprinting, jumping, and landing feel coherent while
preserving server movement authority and the authored GLB animation pipeline.

## Current behavior

Walking gravity, grounded jumps, collision, crouching, and the admin flight
toggle are implemented. Double-tapping W within 300 ms and holding the second
press enables sprinting at 1.5 times normal speed. Sprint state is replicated,
including for late joins, and selects the GLB's baked run animation.

The player model provides idle, walk, run, crouch, and tool animations. Dedicated
jump and fall clips would be a useful model update. Walking and running already
blend in presentation, but players still need explicit replicated airborne
state. See [current controls](README.md) and the
[character kit](assets/models/player/README.md).

## Replicate airborne state

Make grounded, rising, and falling states follow authoritative player physics.
Use them to stop the walk and run cycles in the air and resume locomotion after
landing. Blend into authored jump and fall clips when those assets are available.
Choose an interim pose if the clips are not ready when this work resumes.

Keep animation and camera feedback presentation-only. Handle late joins,
teleports, flight changes, edited support blocks, ceilings, and missing terrain
without inventing a landing or retaining an old airborne state.

Acceptance: local and remote players agree on airborne transitions; running into
a wall does not keep feet cycling; a fall after removing support lands correctly;
flight and teleport resets clear obsolete animation state.

## Improve jump responsiveness

Add local jump prediction and reconcile it with server movement. Introduce a
short jump buffer for presses just before landing and a brief grace period for
presses just after leaving an edge. Initial tuning candidates are a 150 ms buffer
and a 100 ms grace period; these values are proposals, not agreed defaults.

The server must own eligibility and consume each jump once. Key repeats,
replayed input, or prediction corrections must not grant an extra airborne jump.
Clear pending jump assistance across flight changes, teleports, menus, and
disconnects. Prediction must use streamed authoritative terrain; unavailable
chunks must not become assumed air or support.

Acceptance: jumps respond promptly under realistic network delay, corrections
remain bounded, buffered jumps work on landing, and edge grace does not permit
repeated jumps or bypass ceilings and missing terrain.

## Add material footsteps and landing sounds

Connect footsteps to actual grounded distance and gait so walking and sprinting
sound different without producing steps when blocked or airborne. Select sound
profiles from the contacted surface, starting with grass, stone, wood, and water.
Water contact may need a separate splash rule rather than the solid-floor rule.

Use authoritative landing transitions to drive landing feedback, with intensity
based on impact. Define how local prediction and remote playback avoid duplicate
sounds during reconciliation. Expose useful volume and cadence controls through
the existing audio tuning UI.

Acceptance: surface changes are audible, sprint cadence follows movement, idle
and blocked players stay silent, and one landing produces one event even after
a correction.

## Expand creature behavior later

Build on existing creature wandering and the native GLB pipeline. Possible next
behaviors are looking at nearby players and reacting to interactions through
Luau. Choose the intended reactions and animation assets before starting this
separate task.

## Verify when work resumes

Use focused regression tests for jump eligibility, replication, resets, and
audio event duplication. Exercise the real nonblocking listener with native
clients and an isolated temporary save. Inspect walk, sprint, jump, fall, and
landing in the release client; use rendered previews if the window is unavailable.

Run the repository's test, formatting, strict Clippy, and release build checks.
For rendering changes, compare scene setup, mesh size, CPU time, and GPU time
with `cargo run --release -- perf 300 6`. That benchmark excludes live movement
and network latency, which need their own checks.
