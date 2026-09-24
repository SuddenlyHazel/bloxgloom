# Bloxgloom

Bloxgloom is a Rust multiplayer voxel game. The dedicated server owns a procedural, editable world; the desktop client renders streamed chunks with `wgpu` and uses `winit` for input.

Terrain uses coherent multi-scale height fields, rocky uplands, and caves. Saved edits are versioned against the terrain generator so a changed baseline cannot silently alter an existing world.

## Run locally

With a recent Rust toolchain, run a local game with one command:

```sh
cargo run
```

This starts a local server and client in the same process and saves edits in `world/`.

For a dedicated multiplayer server, start the server in one terminal:

```sh
cargo run -- server 127.0.0.1:4000 world
```

Start one or more clients in other terminals:

```sh
cargo run -- client 127.0.0.1:4000
```

The server defaults to `127.0.0.1:4000` and saves edits in `world/`. To connect from another computer on your LAN, bind the server to a reachable address (for example `0.0.0.0:4000`) and pass that computer's address to the client. The first release targets up to 16 concurrent players.

Click the window to capture the mouse. Use WASD to fly horizontally, Space and Shift to ascend and descend. The crosshair marks the targeted block: left click removes it, and right click places the selected hotbar block against it. Use 1–9 or the mouse wheel to select a hotbar slot. E opens the creative block inventory; choose a slot and then a block to assign it. Escape opens the pause menu, where you can resume, change settings, or exit. F3 toggles the debug HUD.

The current game is creative building with unlimited grass, dirt, and stone. Movement is server-authoritative with block collision; gravity, survival systems, and item quantities are not implemented yet. Local settings and hotbar choices are saved per user.

The client logs FPS, frame-time percentiles, visible chunks, triangles, and upload backlog every five seconds. Run `cargo test` for the world, protocol, server, UI, and meshing checks. The interface implementation and validation record are in [PLAN.md](PLAN.md).

For visual debugging without a desktop display, run `cargo run -- preview preview.png`. This renders representative terrain through the same GPU shader and mesh pipeline and writes a PNG that can be inspected directly.

Run `cargo run -- ui-preview ui-previews` to render the Playing, Inventory, Pause, and Settings screens at 1280×720, 640×360, and 640×360 with 2× requested UI scale, without opening a game window.

Run `cargo run --release -- perf 300 6` to measure headless 1280×720 chunk-upload, world-render, target-outline, and HUD work at the maximum supported view radius. It reports CPU submit-side and GPU render-pass frame-time percentiles, adapter, and scene size. It does not measure window presentation or live gameplay FPS.
