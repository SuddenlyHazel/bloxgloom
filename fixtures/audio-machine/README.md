# Audio timer machine

A small Luau machine demonstrates a packaged motor loop, committed interaction
and completion clips, public-state reconstruction for joining clients, and
entity lifetime cleanup. It processes a ten-second timer, not inventory recipes.
The block owns a colocated generic script entity with one public state byte.
Breaking it removes that entity in the same transaction. Completion leaves it
idle; restart uses the registered block action. No items are created by the timer.

```sh
cargo run --release -- server-packages fixtures/audio-machine/packages 127.0.0.1:4000 /tmp/bloxgloom-audio-machine-test
cargo run --release -- client 127.0.0.1:4000
```

Use an isolated save. In the operator admin console, grant `audio:machine`, then
select and place it. Placement plays the start clip; the motor runs for about ten
seconds and the completion clip plays once. Aim at the idle machine, open the
registered-actions view and select **Start audio timer** to run it again.

Walk around it to hear panning and distance attenuation. Break it while running:
the loop fades out. Disconnect/reconnect while running: the current public state
reconstructs the loop without replaying earlier start/completion sounds. Two
machines use separate exact entity-based voice names. Effects and Master volume
settings control these clips; Ambient controls procedural weather separately.

`server/machine.luau` stages one-shots with state updates through `host.sound`.
`client/sounds.luau` declares the motor from current committed public replicas;
repeated loop assertions keep the original playback cursor. Explicit `update`
commands can change gain and pitch, and `stop` fades the named voice out.
