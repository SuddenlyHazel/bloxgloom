# Prism authored visual example

This format-2 package applies a two-texture material to world stone and its own
Prism Stone/Token texture. The vertex hook adds a bounded 0.04-block shimmer;
the surface hook tints the art cyan and adds a little emission. A half-resolution
warm grade feeds a two-input mix before the game's bloom/display mapping.
Downloaded startup sets the material and effect parameters. The bounded public
entity replica window changes the mix strength through a presentation callback.

Run with a fresh isolated save in separate terminals:

```sh
cargo run --release -- server-packages fixtures/visual-packages 127.0.0.1:4000 ./world-prism-try1
cargo run --release -- client 127.0.0.1:4000
```

Existing world stone should appear cyan with subtle motion and a warm scene mix.
Use the usual admin grant UI to try `prism:stone` and `prism:token`; place the
stone or drop the token to inspect their shared material. Inventories remain
finite and server owned. Neither this animation nor its displacement changes
collision, item ownership, or world state.

Generate offline material and scene-composition images:

```sh
cargo run --release -- visual-preview /tmp/prism-previews fixtures/visual-packages prism:stone
```

Inspect `material.png` and `composition.png`. See the
[authored visual contract](../../../docs/modding/AUTHORED-VISUALS.md) for shader
helpers, typed Luau updates, graph ordering and resource limits. Custom models,
actor-cuboid material shaders, UI icon material shaders and live reload are not
exposed by this fixture.
