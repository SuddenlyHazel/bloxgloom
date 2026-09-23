# Bloxgloom

Bloxgloom is a Rust multiplayer voxel game. The dedicated server owns a procedural, editable world; the desktop client renders streamed chunks with `wgpu` and uses `winit` for input.

Terrain uses coherent multi-scale height fields, rocky uplands, and caves. Saved edits are versioned against the terrain generator so a changed baseline cannot silently alter an existing world.

## Run locally

With a recent Rust toolchain, start a server in one terminal:

```sh
cargo run -- server 127.0.0.1:4000 world
```

Start one or more clients in other terminals:

```sh
cargo run -- client 127.0.0.1:4000
```

The server defaults to `127.0.0.1:4000` and saves edits in `world/`. To connect from another computer on your LAN, bind the server to a reachable address (for example `0.0.0.0:4000`) and pass that computer's address to the client. The first release targets up to 16 concurrent players.

Click the window to capture the mouse. Use WASD to fly horizontally, Space and Shift to ascend and descend, left click to remove a block, right click to place dirt, and Escape to release the mouse. Movement is server-authoritative with block collision; gravity and survival systems are not implemented yet.

The client logs FPS, frame-time percentiles, visible chunks, triangles, and upload backlog every five seconds. Run `cargo test` for the world, protocol, server, and meshing checks. The architecture and delivery plan is in [PLAN.md](PLAN.md).

For visual debugging without a desktop display, run `cargo run -- preview preview.png`. This renders representative terrain through the same GPU shader and mesh pipeline and writes a PNG that can be inspected directly.
