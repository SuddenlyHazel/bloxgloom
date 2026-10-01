# Client audio foundation

Bloxgloom now has native client audio output, a bounded mixer, prepared WAV clips
and procedural rain, wind and thunder. The audio worker owns device discovery,
synthesis, mixing and resampling. The device callback consumes prepared stereo
frames from a lock-free ring, converts the device sample format and counts errors.
It does not run Luau, decode files, allocate, log or acquire application locks.
Dedicated servers and ordinary headless previews do not open an audio device.

## Try it

Start the game normally. Open **Settings → Audio** to adjust **Master**,
**Ambient** and **Effects** volumes. Settings persist through the existing config
writer. **Local preview** cycles Off, Rain, Storm and Wind. **Test sound** plays
a short synthesized click; in Storm preview it also triggers thunder. Preview selection is local, starts Off, and resets on
session retirement; it does not announce rain or a storm to other players.

Render reproducible stereo PCM without an audio device:

```sh
cargo run --release -- audio-preview rain 10 /tmp/bloxgloom-rain.wav 1
cargo run --release -- audio-preview storm 20 /tmp/bloxgloom-storm.wav 1
cargo run --release -- audio-preview wind 10 /tmp/bloxgloom-wind.wav 1
```

Exercise the actual device path, or play a prepared file:

```sh
cargo run --release -- audio-play storm 10
cargo run --release -- audio-file /tmp/bloxgloom-rain.wav 10
```

Preview duration is 0.1–120 seconds. Offline output is 44.1 kHz stereo signed
16-bit PCM. The optional offline seed defaults to 1. Device playback uses the
client mixer's seed; it is not a promise of byte-identical offline/device output.
Storm probes explicitly trigger one strike so a short test can hear thunder.
The settings storm preview begins with a strike, then generates strikes probabilistically.

A missing or failed device logs a warning and leaves gameplay usable. The CLI
device probe returns an error instead of reporting success for silent fallback.
The backend uses the default output device, with no microphone access. Device
recovery currently requires restarting the client; selection/hot-plug UI remains
follow-up work.

## Mixer and native playback contract

The mixer uses a 44.1 kHz sample clock. WAV clips retain their input rate and use
linear interpolation during playback. The worker adapts the stereo mixer to the
default device rate; mono output downmixes, stereo maps directly, and additional
channels are silent. Linear resampling is the initial quality profile, without
an anti-aliasing reconstruction filter or a high-quality downsampling guarantee.

Native commands start/stop named clip voices, update the listener or trigger
thunder. Clip voices can loop and use a world position. Positional clips downmix
to mono, use equal-power horizontal panning and inverse-distance attenuation,
and respond to the listener's position/yaw. Volume and positional gains glide
over 20 ms. Clip starts have a short attack, and explicit stops fade over 20 ms. Nonpositional stereo clips retain their channels. Blocks are currently
one metre for the native distance interpretation. There is no terrain occlusion,
room geometry or Doppler calculation.

The final stereo-linked lookahead limiter has 128 mixer frames of latency,
a 0.98 output ceiling and 100 ms gain recovery. Master volume applies to all
sources; Ambient applies to the weather bed, and Effects applies to clips and
thunder. Changing weather fades the old bed before replacing its synthesizers.
A session reset drops queued commands and buffered frames through epoch fencing,
removes voices and clears procedural/reverb/limiter state.

| Resource | Bound |
| --- | --- |
| Pending native commands | 64; overflow rejects new commands |
| Control updates | One coalesced latest volume/preset state |
| Clip voices | 32 |
| Decoded user clip storage and decode scratch | 64 MiB across the process |
| WAV file size | 16 MiB |
| Clip length | 1,323,000 frames; 30 seconds at 44.1 kHz |
| WAV input | Mono/stereo, 8–192 kHz, PCM 8/16/24/32-bit or float32 |
| Device output | Supported native PCM format, 1–32 channels, 8–384 kHz |
| Prepared device ring | 4,096 stereo frames; normal fill target roughly 1,024 |
| Worker mix block | 256 frames |
| Rain impacts | 128 active voices, four modes per impact |
| Configured base rain rate | Up to 2,000/s; default 900/s; gusts modulate arrivals |
| Thunder | Two strikes, 256 segments each, six echo reflectors |
| Sound-only storm cells | Four |

Native command enqueue success means queue admission. The worker can subsequently
reject a command for invalid parameters, duplicate voice IDs or full voice pools;
counters distinguish queue/mixer rejection and device underruns/errors. This is
presentation, without authoritative gameplay receipts. Controls and session reset
do not compete for the bounded event queue. Device teardown waits at most 100 ms;
a stalled OS driver can finish on its detached worker rather than freezing close.

## NoiseMachine adaptation

The Rust procedural modules derive from
[kvmet/NoiseMachine](https://github.com/kvmet/NoiseMachine), pinned to commit
`e709f125b8f8e85780c466ddf6c850ae90026748`. Its MIT license and copyright are
retained in [NoiseMachine-LICENSE](../../third-party/NoiseMachine-LICENSE).
No C library is compiled or linked into the game.

Ported models include:

- Tagged deterministic random streams, damped resonant modes and biquad filters.
- Material-specific rain impacts for water, dirt, leaves, concrete, glass, metal,
  plastic, asphalt and asphalt roofs, including water-bubble resonance.
- Marshall–Palmer drop-size distributions, falling/wind-driven surface impacts,
  advected gust sheets and a 15-band bed representing unplayed distant rain.
- Per-ear rain attenuation, fractional delay, analytic spherical-head shelf and
  rear filtering, plus a six-line shared feedback-delay reverb.
- Speed/bearing-dependent wind coloration and rumble.
- Tortuous-channel thunder with N-wave pulses, distance/air filtering, seeded
  echo reflectors, stereo panning and quarter-rate reverb.

The storm driver adapts the upstream **default** squall shape: passing rain cells,
a trailing rain band, gust fronts and lightning scattered around each cell. Strikes
outside the 15 km audible range are skipped, rather than moved nearer. It uses a 60× preview clock and
omits temperature/cooling controls because insect synthesis is not in this slice.
Rain surfaces are nine fixed configurable slots; zero coverage disables a slot.
Presets use the upstream default material mix. This does not yet sample actual
voxel materials, outdoor exposure or game weather.

Thunder follows the upstream convention of starting at its first audible arrival;
it preserves channel-relative propagation timing, but does not wait the entire
strike distance divided by sound speed before playback. Echo reflectors are seeded
synthetic scenery, without voxel-world acoustic tracing. Rain's head model is
analytic, without measured HRTFs, elevation or live head tracking.

Not ported in this foundation: white/pink noise layers, hum generators, crickets,
cicadas, the source desktop GUI/Arduino host and every configurable storm control.
The port is a documented weather-synthesis subset, rather than full API parity.

## Verification and next work

Tests cover output conversion and silence, rate adaptation, reset/queue races,
finite parameter validation, clip decoding, panning, looping/stopping, overload
limiting and render-buffer partition independence. Procedural tests exercise
physical bubble modes, rain admission/reclamation, surface distribution and wind
impacts, deterministic spectral beds, thunder distance/retirement and pool bounds.
A wind regression compares samples generated by the pinned upstream C implementation.

The foundation supplies native output and synthesis. Luau declarations/bindings,
package sound delivery, commit-driven sound events and deduplication, entity-attached
sound lifetimes, voxel material sampling, device recovery and richer
acoustics remain future work. [Game weather](../weather/FOUNDATION.md) now supplies
continuous rainfall/wind, shelter exposure and synchronized thunder; the local
preview driver remains separate from authoritative weather. Audio playback remains
client presentation; game weather adds its own server snapshot and checkpoint
without changing chunk, inventory or entity save formats.

## Acceptance measurements

Measured on an Apple M1 Pro with 16 GiB RAM on October 1, 2026, using the release
build and offline seed 1. Each run includes stereo synthesis, final limiting,
PCM encoding and WAV writes; mixer construction/calibration and Cargo compilation
are outside the timed render. These are single-run profiles, not worst-case bounds.

| Preview | Audio duration | Render elapsed time | Peak | RMS | Rejected rain impacts |
| --- | --- | --- | --- | --- | --- |
| Rain | 10 s | 236.1 ms | 0.0411 | 0.00736 | 0 |
| Wind | 10 s | 27.2 ms | 0.0354 | 0.00721 | 0 |
| Storm with a manual strike | 20 s | 818.9 ms | 0.3076 | 0.01115 | 0 |

The native device probe played five seconds of storm synthesis at **48 kHz stereo**
with **zero underrun frames, device errors or rejected commands**. Prepared WAV
playback through the same output path also succeeded. This verifies delivery and
sample-rate adaptation; listening quality still benefits from testing with your
speakers/headphones. Offline samples are available by running the commands above.

The workspace acceptance run passed 1,367 application tests and 45 host-API tests,
with five opt-in tests ignored. After lightning and compact-layout corrections,
all 25 focused audio tests passed. Formatting and strict all-target/all-feature
Clippy passed. Production egui screenshots were inspected at 1280×720 and 640×360;
the compact page keeps both Test sound and Back visible. The code graph was refreshed.

The release renderer benchmark (`perf 300 6`) retained 320,236 vertices,
480,354 indices and 18,573,688 mesh bytes. Scene setup took 2,328.1 ms; steady
CPU p50/p95 were 0.312/0.468 ms and GPU p50/p95 were 0.293/0.407 ms. The most
recent pre-audio run recorded 2,354.7 ms setup, CPU 0.304/0.425 ms and GPU
0.241/0.338 ms with the same geometry. These separate runs are a historical
comparison, not a controlled attribution of timing changes. Headless renderer
benchmarks do not open the audio device; synthesis cost is measured above.
