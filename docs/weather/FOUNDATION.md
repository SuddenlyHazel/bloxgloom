# Game weather foundation

The server now owns clear, rainy and stormy weather. Clients interpolate its
weather snapshots and present clouds, rain, lightning and procedural sound.
Weather changes presentation; it does not damage players, grow crops, extinguish
fires or change blocks in this first pass. Luau weather bindings are follow-up work.

## Try it

Start a local game and use the existing admin console:

```text
weather set rain
weather set storm 0
weather set storm 0 severe
weather set storm 10 mild
weather set clear 10
```

The optional number is the transition duration in whole seconds, from 0 to 60;
the default is 10. An optional third argument selects storm severity: `mild`,
`normal` (the default), or `severe`. Severity profiles are server-owned and blend
continuously, including when switching between storms. They are saved and sent to
joining clients. Natural storms use normal severity.

| Storm severity | Rainfall | Wind | Clear zone / storm fog obscures 99% |
| --- | --- | --- | --- |
| Mild | 21 mm/hour | 10 m/s | 6 / about 153 metres |
| Normal | 30 mm/hour | 18 m/s | 6 / about 80 metres |
| Severe | 54 mm/hour | 30 m/s | 6 / about 49 metres |

All three have continuous overcast and the same lightning opportunity timing.
Storm fog uses radial distance from the view camera and exponential scattering,
with a six-metre clear zone. Distant surfaces converge to the same sky color,
including shaded faces; their own skylight level no longer keeps silhouettes
visible through the storm. Listener shelter fades storm density on sealed interior surfaces; sky-lit
outdoor geometry remains foggy through openings. Clear-weather distance haze remains. The ranges above
refer to storm scattering, on top of existing background haze. Wind affects
rain slant and procedural wind audio; it does not push players or bend vegetation.
Thunder now combines the strike's original transient with a broad 35–85 Hz body,
sharing its directional position, air filtering, echoes and output limiter.

Only the server's authorized admin can change weather. Overrides
last five minutes before natural weather resumes. Use `time set midnight` to see
nighttime lightning, then `time set noon` to return to daylight.

Leave **Settings → Audio → Local preview** at **Off** to hear game weather.
Rain, Storm and Wind previews temporarily replace the world ambience; switching
back to Off restores it. Master, Ambient and Effects settings still apply.

For a manual acceptance pass:

1. Set rain and watch the clouds, rain density and sound change gradually.
2. Enter a building or cave. Rain should stay above the roof; outdoor rain can
   still be seen through openings. Ambient sound becomes softer and muffled.
3. Set storm with a zero-second transition. After the next strike, the flash
   should precede thunder by its travel time. Sealed caves should stay dark.
4. Join with a second client. Both should see the same weather and transition;
   players in the same lightning region share strike position and timing.
5. Restart the server during a transition. Weather should resume, with no replay
   of old lightning or retained sounds from a previous client session.

Production offscreen comparisons are also available:

```sh
cargo run --release -- weather-preview /tmp/bloxgloom-weather-previews
```

This writes clear, rain, storm, lightning, sheltered and sealed-cave lightning PNGs.

## Authority, timing and persistence

New worlds begin clear. The first natural choice is after three minutes; subsequent
choices last three to six minutes. Seeded choices have clear/rain/storm weights
of 50/30/20 percent and may keep the current state. Natural transitions blend over
30 seconds. Cloud cover is normalized; rainfall is relative to a normal storm (0–1.8),
and wind speed is metres/second.

A monotonic weather clock is separate from the day/night clock. Setting the time
of day does not move a storm backwards. Weather pauses while the server is stopped.
The server sends a full weather sample at join, after an admin change and roughly
once per second. Dropped advisory samples are repaired by the next sample.

Admin changes use the existing registered action, permission and durable receipt
path. The weather anchor enters the server WAL; normal clock progress is saved by
a bounded background worker every five seconds and at graceful shutdown in
`world.weather`. Startup recovery repairs committed overrides whose checkpoint
was not written, including after WAL rotation. An abrupt crash can lose ordinary
clock progress since the last checkpoint (normally about five seconds); durable
admin overrides still recover. Live application of an override
preserves clock progress during the durable write wait.

The wire version is 22; client and server must run the same build.
Existing world data remains compatible: weather adds a checkpoint file rather
than changing chunk, inventory or entity formats.

## Presentation and bounds

The server seed and weather clock define one lightning opportunity per 15-second
slot once a storm has fully blended in. Each fixed 512×512-metre region has a
deterministic world-space strike at height 120. Timing and slot identity are shared;
positions differ by region. Crossing a region boundary does not replay the same
slot. Strikes are cosmetic, with no block/entity collision or damage.

Clients suppress strikes already past when joining, deduplicate live strikes and
discard stale events after a stall. Up to eight pending thunder arrivals are
retained. Thunder starts after distance/343 metres per second, uses relative
horizontal direction, and respects the audio engine's two-strike pool. Sheltered
thunder is quieter; it is not a voxel acoustic simulation.

Rain uses 512 streaks at normal storm intensity and at most 1,024 depth-tested streak triangles in a 16×16-metre area around
the view camera. A 16×16 cover grid samples resident authoritative chunks every
200 ms, scanning 16 metres below and up to 64 above the camera. Its ceiling is
clamped to the streamed vertical range around the player’s feet, so crossing an
eye-height chunk boundary cannot mistake an out-of-range chunk for a roof. Solid blocks, including
transparent solid roofs, clip rain; unknown columns suppress it until streamed.
Streaks also check adjacent roof columns when wind slants them. This is bounded
nearby shelter sampling, not a global height map: roofs beyond the sampled horizon
and overhangs beyond the streamed area are outside this first pass.

The player's eye determines audio shelter independently of third-person camera
position. Audio checks only cells overhead; missing ground below the listener
does not count as shelter. Exposure fades over half a second, and weather audio updates at most
20 times per second. The audio worker smooths physical rainfall and wind inputs;
it generates no independent world lightning. Existing local previews retain their
own sound-only storm simulation. See [Audio foundation](../audio/FOUNDATION.md).

Clouds darken the sky and outdoor skylight, with slow continuous advection.
Lightning changes sky illumination without creating light inside sealed caves.
There is no particle collision simulation, wetness, snow, biome climate, regional
rainfall, gameplay weather effect or script weather API yet.

## Acceptance evidence

The final all-feature workspace run passed 1,391 application tests and 46 host-API
tests (1,437 total), with five opt-in tests ignored. The 22 focused weather tests
also passed. Coverage includes real nonblocking listener admission, shared updates
and admin denial, persistent transitions, rotated-WAL recovery, elapsed-clock
continuity during durable writes, deterministic scheduling and regional strikes,
wire validation, shelter/roof clipping, stale-strike suppression, audio source
fades and session resets. Existing block-placement and player-lifecycle examples
remain covered by the full suite.

Formatting and strict all-target/all-feature Clippy passed. Final production
previews were rendered and inspected, including sheltered scenes using the roof
grid. Rain remains outside the ceiling, and sealed-cave lightning stays dark.

On the Apple M1 Pro, the release `perf 300 6` baseline retained 320,236 vertices,
480,354 indices and 18,573,688 mesh/upload bytes. Scene setup was 2,336.6 ms.
Steady CPU p50/p95/p99 were 0.310/0.440/0.567 ms; GPU values were
0.251/0.388/0.408 ms. The preceding audio-foundation run had 2,328.1 ms setup,
CPU p50/p95 0.312/0.468 ms and GPU 0.293/0.407 ms with identical geometry.
These separate runs are historical comparisons, not statistically controlled
performance changes. This headless clear-weather benchmark excludes presentation,
live audio, client shelter sampling and active rain; bounded rain geometry and
weather sound behavior are verified separately above.

## Storm presentation follow-up

Cloud contrast fades out at the horizon, preventing the planar cloud projection
from stretching into vertical bands against the sky below unloaded terrain.
Storm severity also increases terrain and character fog density without changing
voxel lighting, chunk authority or stream distance. The rain buffer is sized
separately from the fire buffer to hold the increased severe-storm geometry.

These follow-up changes require a new visual and listening acceptance pass;
the foundation screenshots and benchmark above describe the earlier build.

Follow-up verification: 28 weather-focused tests passed with the real-listener
case excluded after the sandbox denied socket binding; 28 audio-focused tests
and 46 host-API tests passed. The weather and audio selections overlap on shared
mixer tests. Formatting, strict all-target/all-feature Clippy and GPU-independent
WGSL validation passed. A 15-second seeded storm WAV rendered in 8.2 seconds in
the debug build, with peak 0.357, RMS 0.039 and no rejected audio events or dropped
rain voices. GPU previews and graphics performance could not run because this
sandbox exposes no GPU adapter. Multiplayer severity synchronization still needs
the real-listener test rerun outside the socket restriction.

### Depth-fog acceptance

Use `weather set storm 0 severe`. Nearby blocks within six metres should retain
clear detail and color, while terrain and trees at 40–50 metres should lose almost
all contrast against the sky. Turn the camera: fog distance should stay anchored
to the geometry, including toward the edges of the view. Compare mild and normal
severity, then look outdoors from under a roof and check a sealed cave interior.

The shared terrain/actor fog shader has an opt-in GPU regression for near detail,
depth progression and convergence of distant lit and shaded faces:

```sh
cargo test --all-features gpu_storm_fog -- --ignored
```

Depth-fog verification passed 29 weather-focused CPU tests and five fog-focused
CPU/shader tests (these selections overlap). The optional GPU regression was
ignored. Strict Clippy, formatting and the release build passed. Production
weather previews and `perf 300 6` were attempted, but both stopped because Metal
exposed no GPU adapter in this sandbox; visual acceptance and performance
measurement remain pending outside that restriction.
