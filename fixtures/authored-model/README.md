# Native GLB preview fixture

This tiny generated test model exercises native Rust GLB loading, named baked
clips, selectable eye geometry, mouth/hat visibility, and material color modes.
It is a pipeline fixture, not the replacement player art.

```sh
cargo run -- model-preview fixtures/authored-model/model.glb /tmp/model-classic.png fixtures/authored-model/controls.json fixtures/authored-model/classic.json
cargo run -- model-preview fixtures/authored-model/model.glb /tmp/model-sleepy.png fixtures/authored-model/controls.json fixtures/authored-model/sleepy.json
cargo run -- model-preview fixtures/authored-model/model.glb /tmp/model-bounce.png fixtures/authored-model/controls.json fixtures/authored-model/bounce.json
```

Regenerate the 9.7 KiB GLB with `python3 fixtures/authored-model/generate.py`.
Python creates this test fixture only; game loading and preview rendering use
Rust and wgpu directly. See [the model authoring guide](../../docs/modding/AUTHORED-MODELS.md).
