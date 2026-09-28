# Jade voxel material example

This format-2 package registers its own `jade:tile` PNG texture and a
`jade:token` sprite item, then applies `assets/shaders/jade.wgsl` to that
texture layer selected in `assets/materials/tint.json`.
It uses the renderer's existing geometry, lighting, emission, fog and alpha
cutout; the shader only changes the sampled albedo. To try it with a **fresh**
save directory, run from the repository root in separate terminals:

```sh
cargo run --release -- server-packages fixtures/material-packages 127.0.0.1:4000 ./world-jade-try1
cargo run --release -- client 127.0.0.1:4000
```

The Jade Token sprite should shade green; world stone and other textures retain
their normal appearance. The verified client bundle prepares the WGSL off the window
thread and resolves the namespaced texture against the exact session catalog
before acknowledging content readiness. Shader/pipeline failure aborts client
setup rather than silently substituting the default material.

Current limit: one material target per bundle. This fixture only registers an
item sprite; a package can also call `host.register_block` with its registered
PNG to create a one-state opaque placeable cube using the same texture. This
package does not provide arbitrary mesh shaders or live shader reload.
