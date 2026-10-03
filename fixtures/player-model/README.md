# Packaged player model

Run `cargo run --release -- local-packages fixtures/player-model/packages /tmp/bloxgloom-player-model-v26` with a fresh world, then join. New players use the embedded-texture fixture rig. The tiny model deliberately reuses its `bounce`/`nod` clips to make switching visible.

Open Character to choose Builtin character or `rig:player`, edit named eyes/mouth/hat and body/iris colors, and preview the baked state mappings. Apply persists the selected rig and look. F4 command `rig:nod` triggers a one-shot baked clip on the server; nearby players see it too. Reconnect/restart keeps looks while clearing transient clip playback.

The owner's first-person color pass hides the eye choices. Third-person, other players and world shadows keep the complete rig. All public model identities and control data are frozen package declarations, with compatibility checked before joining or opening an existing world.
