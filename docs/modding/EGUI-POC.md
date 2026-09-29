# egui UI proof of concept

This branch adds an experimental egui overlay to the existing wgpu client. Press
**F7** in play, inventory, or a container to open it; press **F7** or **Escape**
to close it. The overlay reads the current inventory and container view. Clicking
a source slot and then a destination sends the game's existing inventory move
or container transfer request to the server. Right-clicking a container transfer
moves up to one item. The server still decides whether each request applies.

The overlay exercises egui text input, a drop-down slot filter, scroll areas,
buttons, keyboard focus, pointer input, clipboard integration, and rendering
into the existing wgpu surface. It does not expose egui to Luau or replace the
current UI; it is an experiment for choosing the Phase 6 UI foundation.

To try it with the built-in kiln, run:

```sh
cargo run -- local /tmp/bloxgloom-egui-poc-world
```

Open a kiln and press **F7**. To try the authored press, use a fresh save:

```sh
cargo run -- local-packages fixtures/phase4-showcase/packages /tmp/bloxgloom-egui-showcase-world
```

The branch pins Rust 1.95 because egui 0.36 requires it. `rustup` installs the
toolchain when needed. For headless screenshots of the same egui document at
desktop and compact sizes, run `cargo run -- egui-preview /tmp/egui-previews`.

![egui container at 1280 by 720](egui-poc/egui-container-1280x720.png)

![egui container at 640 by 360](egui-poc/egui-container-640x360.png)
