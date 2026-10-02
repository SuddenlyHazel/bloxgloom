# Weather-driven rain collector

This playable Luau package combines captured weather, an advisory transition
hook, declared block materials, a custom impact model and a persistent collector.
Its two public state bytes are stored fill (0–100) and currently collecting (0/1).
It fills once per second during rain and pauses when dry, full, or covered within
its 64-block catchment clearance. Unknown terrain defers the tick until loaded.
This is a small demonstration counter, not a recipe or water-item producer.

```sh
cargo run --release -- local-packages fixtures/rain-collector/packages /tmp/bloxgloom-rain-collector-test
```

The local package launcher selects your profile as operator. Use the operator console to grant `rain:collector`, then place it outdoors.
Run `weather set storm 0 severe` for a severe storm with zero transition. You should hear the loop
start; **Empty rain collector** becomes usable as it fills. Rain fills it in
roughly 34 seconds in a severe storm, then its loop stops. Emptying it plays the
completion clip and collection resumes. Run `weather set clear 0`: collection
and the loop pause without losing stored fill. Place a roof above the mouth to
pause collection; remove it to resume. Break the collector to retire its companion
entity and loop. Reconnect during collection to reconstruct the current loop
without replaying past one-shots. Restart preserves fill and weather.

`rain:glass` and `rain:wood` make comparison surfaces; their reused demonstration
texture is deliberately identical. `rain:habitat` declares canopy habitat for
local daytime cicadas. The collector declares a bounded custom resonant metal
impact: it changes each actual spatial droplet's synthesis, not a global material
mix. Unannotated blocks retain native material/habitat classification.

Listen to the custom profile independently through the production mixer:

```sh
cargo run --release -- audio-material-preview custom 8 /tmp/rain-collector-impact.wav 7
cargo run --release -- audio-file /tmp/rain-collector-impact.wav 8
```

The server weather hook logs target transitions and cannot write gameplay state.
The durable collector tick reads weather directly. The client reconstructs audio
from committed public entity state and receives readonly installed weather through
its opted-in replica observations. Terrain clearance and filling rate are explicit
fixture policy; they are not an engine-wide hydrology or climate simulation.
