# Client audio foundation

Bloxgloom now has native client audio output, a bounded mixer, prepared WAV clips
and procedural rain, wind, thunder, crickets and dog-day cicadas. The audio worker owns device discovery,
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
| Insect individuals | Four crickets and four dog-day cicadas, separate from clip/rain pools |
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
does not drive the live insect layers or model world temperature/cooling.
Local previews retain the upstream default material mix; live world rain uses
the voxel surface integration below. Outdoor shelter and synchronized weather
are supplied by the client presentation path.

Thunder follows the upstream convention of starting at its first audible arrival;
it preserves channel-relative propagation timing, but does not wait the entire
strike distance divided by sound speed before playback. Echo reflectors are seeded
synthetic scenery, without voxel-world acoustic tracing. Rain's head model is
analytic, without measured HRTFs, elevation or live head tracking.

Not ported: standalone white/pink noise layers, hum generators, the other nine
cicada species, the source desktop GUI/Arduino host and every configurable storm
control. Cricket and dog-day cicada integration is described below.
The port is a documented weather-synthesis subset, rather than full API parity.

## Verification and next work

Tests cover output conversion and silence, rate adaptation, reset/queue races,
finite parameter validation, clip decoding, panning, looping/stopping, overload
limiting and render-buffer partition independence. Procedural tests exercise
physical bubble modes, rain admission/reclamation, surface distribution and wind
impacts, deterministic spectral beds, thunder distance/retirement and pool bounds.
A wind regression compares samples generated by the pinned upstream C implementation.

The foundation supplies native output and synthesis. The [scripting extension](SCRIPTING.md)
now adds package sound delivery, commit-driven events/deduplication and entity-linked
sound lifetimes. Device recovery and richer geometry-aware
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

## Packaged scripting extension

[Scripted audio](SCRIPTING.md) now delivers namespaced WAV assets and connects
transactional gameplay sound, UI/replica commands and entity-linked loops to this
mixer. It adds live gain/pitch updates, committed stock gameplay cues, duplicate
delivery suppression and session/entity cleanup. Procedural weather retains its
existing native path.


## World-space rain surfaces

Live rain samples a 16×16 patch around the player's eye every 200 ms. Only resident
server chunks are read. Each column contributes its first known solid top face;
missing cells above a surface suppress that column, and ground underneath canopy
or a roof is not sampled as exposed rain. Known open neighbours admit the top
block's two windward side faces for driving rain. The current game wind direction
is fixed toward +X/+Z. The immutable scene contains at most 768 one-square-metre
faces and crosses the native audio queue by shared ownership. Identical scenes
are skipped, queue rejection retains the latest scene for retry, and session
retirement clears it. There is no disk I/O or synthesis on the window thread.

Builtin grass, dirt, moss, sand, gravel and snow use the upstream dirt profile;
solid cutout foliage uses leaf; stone and other hard solids use concrete. All
wood axis states use a new damped wood profile. Modded solid cutout blocks inherit
leaf and other modded solids currently inherit concrete. Explicit package acoustic
material declarations are a follow-up. Water, glass, metal, plastic and asphalt
profiles are retained for comparisons, but they do not invent corresponding
materials in a world without those block types.

On the audio worker, sampled face area and windward incidence drive the bounded
arrival rate. Face selection favours nearby surfaces by inverse squared distance;
this is a perceptual importance approximation, not an exact simulation of every
physical impact. New drops jitter over the selected face and use its actual 3D
distance and listener-relative horizontal bearing. Material click/resonant/bubble
models, drop-size distributions, wind-driven impact speeds, per-ear attenuation,
fractional delay, spherical-head shadow and rear filtering all stay active. The
15-band diffuse rain bed learns from these actual impact spectra. Its distant
energy uses the existing annulus approximation rather than voxel acoustic tracing.
Unknown/empty scenes generate no fake material rain; wind remains independent.

The listener is the player's eye, including in third-person mode. Leaf shelter
blocks visible drops without imposing building-style audio muffling. Real roofs
retain softened outdoor ambience. Reverb and thunder reflectors still use synthetic
geometry, and the head model has no elevation-dependent HRTF.

Use the same seed and duration for material comparisons:

```sh
cargo run --release -- audio-material-preview wood 8 /tmp/rain-wood.wav 1
cargo run --release -- audio-material-preview leaf 8 /tmp/rain-leaf.wav 1
cargo run --release -- audio-material-preview stone 8 /tmp/rain-stone.wav 1
cargo run --release -- audio-material-preview water 8 /tmp/rain-water.wav 1
cargo run --release -- audio-material-preview split 8 /tmp/rain-spatial.wav 1
```

These use the game mixer, 30 mm/hour rain, no wind, and a 1.6-metre-high listener
above a tiled patch. `split` places metal on +Z and wood on −Z, then turns the
listener through one full revolution. All ten material names are accepted by the
CLI. Local Rain/Storm preview still uses the default mix; leave Local preview Off
when checking live world materials. Try `weather set rain 0`, compare grass and
stone terrain, then stand beside/under a canopy and build a wooden roof. Walk and
turn to hear the sampled surfaces change, and remove the roof to verify updates.

Initial integration checks passed 39 audio-selected and 32 weather-selected tests
(the selections overlap), strict Clippy and formatting. Regression coverage
includes canopy exposure, missing chunks, wood states, spatial turns, windward
walls, queue retry, session cleanup, distinct material spectra and empty-scene
silence. Subjective quality is for the listening pass; these checks verify behavior.


## Spatial insect ambience

The native world ambience now includes four persistent cricket individuals and
four dog-day cicadas adapted from the same pinned upstream revision. Crickets
retain three-to-five-pulse chirps, pitch differences, a falling carrier per pulse,
slightly different/jittered call periods and exponentially distributed singing
and silent bouts. Dog-day cicadas retain their resonant tymbal clicks, jittered
click intervals, 10–18-second held calls, 2–4 Hz throbbing, swells, falling pitch
and click rate during wind-down, random rests and a diffuse stereo chorus. Both
send to the shared rain reverb and use the same ear-delay/head/rear-filter path.
The other nine upstream cicada species remain a separate extension.

Known exposed grass/moss tops supply cricket habitat; leaf canopy tops supply
cicada habitat. Snow, sand, hard roofs and unknown chunks do not invent insects.
A seeded world-coordinate ordering chooses at most four locations per layer and
keeps them stable while the sampled patch is unchanged, independent of vector
ordering. Individuals are presentation sources, without creature entities, damage,
save files or server-side populations. Updating geometry does not reset call
phases or consume the rain/thunder random streams. Sources retire when habitat
leaves the resident patch, and session reset clears all voices, filters and tails.

The server's existing world clock supplies day/night activity: crickets at night,
cicadas in daylight, with twilight blending and a half-second volume convergence.
Rain above 0.5 mm/hour suppresses new calls in both layers; wind above 8 m/s also
suppresses cricket chirps. A presentation temperature profile of 18–27 °C controls
cricket rate and cicada admission; this is explicitly an audio policy, not a new
authoritative climate or biome-temperature system. The 15% sheltered audio floor
and indoor filtering also apply to insects. Distance and bearing update at the
100 Hz audio control clock from the player-eye listener; ongoing calls retain
filter state while spatial coefficients are retargeted, so turning during a long
cicada call changes its ear delay, attenuation, head shadow and rear coloration.

Listen with Local preview Off. Use `weather set clear 0`, then `time set midnight`
on grass/moss for crickets. Use `time set noon` beside trees for cicadas. Switching
to rain should fade the insects, and a sealed cave without nearby habitat should
have no fabricated chorus. Ambient/Master volume controls govern both layers.

Reproducible isolated previews use the live mixer and rotate through one revolution:

```sh
cargo run --release -- audio-insect-preview crickets 12 /tmp/crickets.wav 1
cargo run --release -- audio-insect-preview cicadas 12 /tmp/cicadas.wav 1
```

The scenes use grass-like ground at night and canopy habitat during daylight,
respectively, with no rain/wind. They demonstrate synthesis/spatial behavior;
actual world locations come from resident voxel snapshots. Tests cover habitat
admission/retirement, day/night/rain suppression, deterministic independent
synthesis, source stability and movement/turning during active calls.


### World-audio integration acceptance — October 1, 2026

The final audio selection passed 41 tests and the weather selection passed 32
(the selections overlap). Strict all-target/all-feature Clippy and formatting
passed. The release build succeeded. Seven offline probes rendered through the
production mixer with zero rejected impacts/commands and no clipping.

| Probe | Duration | Render elapsed | Peak | RMS |
| --- | --- | --- | --- | --- |
| Wood rain | 8 s | 241 ms | 0.0531 | 0.00948 |
| Leaf rain | 8 s | 152 ms | 0.0238 | 0.00459 |
| Stone rain | 8 s | 138 ms | 0.0202 | 0.00351 |
| Water rain | 8 s | 364 ms | 0.0877 | 0.01307 |
| Rotating wood/metal rain | 8 s | 304 ms | 0.0600 | 0.00975 |
| Rotating crickets | 12 s | 73 ms | 0.0573 | 0.00324 |
| Rotating dog-day cicadas | 12 s | 80 ms | 0.0586 | 0.00516 |

These are single-run Apple M1 Pro measurements, including PCM encoding and file
writes, excluding compilation and mixer construction. They measure synthesis
and delivery behavior, not subjective listening quality or client voxel-sampling
frame time. The WAVs are local disposable artifacts in `target/audio-integration/`;
the CLI commands above reproduce them. No renderer/mesh implementation changed.

The native device path also accepted the generated split-material WAV and
completed its two-second playback probe without reporting device errors. This
checks playback availability/delivery; it is not a subjective listening review.
The code graph was refreshed with AST extraction.
