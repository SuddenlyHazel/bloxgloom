# Packaged and scripted audio

Sound clips are immutable package assets, decoded on preparation workers before
publication. Declare them in a format-2 `package.txt`:

```text
asset sound motor assets/sounds/motor.wav
```

A package named `factory` registers that clip as `factory:motor`. File paths never
cross the wire or reach gameplay callbacks. The existing exact bundle hash,
streaming verification and dependency checks cover audio bytes. Audio uses asset
tag 10 in the existing canonical bundle grammars; wire version 24 and client
runtime contract 11 require matching clients before downloading a bundle.
No save schema or numeric content IDs change.

WAV accepts mono/stereo integer PCM at 8/16/24/32 bits or 32-bit float, 8–192 kHz.
Invalid, empty, truncated, nonfinite and out-of-range samples reject the whole
installation. Bounds are the existing 2 MiB per package asset, 1,323,000 frames per
clip, 256 clips per installation including four builtins, and a 64 MiB process-wide
decoded audio budget, including scratch allocation. Decoded clips retain their
source sample rate. Stereo positional clips downmix to mono for spatialization.

## Server gameplay calls

Gameplay action, block decision, pickup decision and generic entity/moving-object
decision callbacks accept `host.sound(table)`:

```lua
host.sound{kind='play', voice='motor-1', clip='factory:motor',
    entity=event.entity, looping=true, gain=0.5, pitch=0.8}
host.sound{kind='play', voice='finish', clip='factory:finish', position={x,y,z}}
host.sound{kind='update', voice='motor-1', gain=0.7, pitch=1.2}
host.sound{kind='stop', voice='motor-1'}
```

A sound is staged with the transaction. Failed/caught-invalid calls, abandoned
attempts, stale plans and rejected actions publish nothing. Sound publication
occurs after WAL acknowledgement and world installation. Action receipt retries
return the prior result without replaying audio. Native break/place, pickup and
interaction cues originate here too, rather than from optimistic client input.
Builtin keys are `bloxgloom:break`, `bloxgloom:place`, `bloxgloom:pickup` and
`bloxgloom:interact`. Bulk terrain effects produce at most one stock edit cue per
action; fire waves do not masquerade as player block breaks.

`voice` is a nonempty local identifier up to 64 bytes; it is scoped to the handler's
package. Play requires a registered namespaced `clip` and either `position` or an
exact entity handle. Entity position is captured by the server and participates
in its ordinary dependency validation. Loops require an entity. Gain defaults to
1 and accepts 0–4; pitch defaults to 1 and accepts 0.25–4. Coordinates must be
finite and within ±16,000,000. At most 16 script sound operations may be staged in
one transaction, within the normal host operation budget. Unknown clips and
invalid arguments latch rollback even inside `pcall`.

Sound operations are transient presentation, not durable voices or save data.
A restart does not replay old one-shots. A newly joining client does not recover
a server-started loop automatically. Declare ongoing loops from current public
replicas when they must survive joins or streaming changes, as the fixture does.
Dedicated creature, process-machine, owner-system and player-lifecycle callback
signatures remain their existing contracts; this API is on gameplay Context.

## Client presentation commands

UI and replica callbacks can return the same fields with `op='sound'`:

```lua
return {{op='sound', kind='play', voice='motor-1', clip='factory:motor',
    entity=entity.id, looping=true, gain=0.5, pitch=1}}
```

These use the normal 16-command callback limit. Entity-linked play commands must
name a handle offered to that callback. Generic script entities now enter their
package's existing bounded public mobile window alongside creatures and moving
objects; private state stays private. Positional one-shots are available to UI
callbacks too. Use `kind='update'` for gain/pitch and `kind='stop'` for explicit
cleanup. Defaults on update are 1, so specify both controls to retain either.
Only free positional voices use an updated position; an attached voice continues
following its entity's authoritative location.

Client voices are scoped to the entry package and kept separate from server
voices. A client command cannot stop another package or a server-owned voice.
Unknown clips/invalid sound batches produce no native audio from that batch.
Repeated looping play for an active name is idempotent and preserves its cursor;
use update to change controls, or stop then play to change clip/attachment.
Reusing a one-shot name replaces its previous voice with a short fade.

The client tracks at most 32 voices and a server batch high-water mark. Reliable
publication is receipt-ordered; repeated or earlier batches do not retrigger sounds. Entity disappearance
from installed replicas stops attached voices; session retirement clears voices,
clip references and deduplication state through the native reset epoch. Attached
positions update at most 20 Hz. Stops retry when the 64-command native queue is
full. Presentation overload may drop new sounds; it does not alter authoritative
state or action receipts. Playback controls and stops glide over about 20 ms.

There is horizontal panning and inverse-distance attenuation, with the existing
limiter and Effects/Master volume controls. Occlusion, reverb, buses, compression,
streamed music, device selection and hot-plug recovery remain follow-up work.
Weather continues to use its native procedural path. Luau now supplies captured
weather reads, advisory transition hooks and authorized admin controls, while block
`acoustics` metadata selects native/custom impact profiles and insect habitats.
See [weather and acoustic authoring](../../SCRIPTING.md#weather-hooks-and-block-acoustics).

See [the playable audio timer](../../fixtures/audio-machine/README.md),
[editor definitions](../../types/bloxgloom.d.luau) and
[native audio foundation](FOUNDATION.md).

## Verification — October 1, 2026

The workspace all-feature suite passed 1,473 tests (1,427 game + 46 host API);
six existing opt-in tests were skipped. Strict all-target/all-feature Clippy and
format checking passed. Eight focused sound tests passed again after test cleanup.
The native device path accepted the fixture motor WAV and completed its two-second
probe without device errors. This verifies device playback, not subjective mixing
quality. No renderer or mesh implementation changed, so a graphics benchmark was
not part of this audio extension. The code graph was refreshed with AST extraction.

```sh
cargo test --workspace --all-features -- --test-threads=2
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --all-features sound -- --test-threads=2
cargo run --all-features -- audio-file fixtures/audio-machine/packages/audio/assets/sounds/motor.wav 2
```

Real nonblocking-listener tests cover verified package delivery, committed sound,
receipt replay suppression, caught-invalid rollback, and the timer machine's full
start/completion cycle. Client/mixer tests cover live controls, entity following,
voice limits, cleanup, stop retries under queue pressure and whole-session batch
deduplication. Decoder/protocol tests reject malformed/truncated data and validate
wire lengths and exact entity IDs.
