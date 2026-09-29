# Phase 4 Luau showcase

`packages/demo` is a local package for the supported Phase 4 authoring path. It
defines a five-part mobile sproutling, a two-cell fueled stone press with an
inventory screen, explicit idle/lit block states, and a client replica callback
that colors the sproutling from its public grounded flag and emits a short-lived
blue spark. The callback handles at most eight entities per batch within the
16-command output limit.

Use the package directory as the local package root when starting a server.
These headless previews show the authored assets and screen:

```sh
cargo run -- creature-preview demo:sproutling /tmp/sproutling.png fixtures/phase4-showcase/packages
cargo run -- creature-preview demo:sproutling /tmp/sproutling-tinted.png fixtures/phase4-showcase/packages 0.7,1,0.7
cargo run -- inventory-preview demo:press_machine /tmp/press-screen fixtures/phase4-showcase/packages
cargo run -- block-preview 'demo:press[lit=on]' /tmp/press-lit.png fixtures/phase4-showcase/packages
cargo run -- fire-preview /tmp/fire-and-spark.png
```

The real-listener integration test `phase4_showcase_creature_machine_and_replica_survive_real_join_and_restart`
downloads the package into a client, spawns the creature, verifies the worker's
state-derived tint and attached spark, and places the two-cell press from one
finite inventory item. It restarts the server from the same save and checks the
stable catalog/entity identities, both press cells, and consumed item. The
second creature in the tinted preview uses the callback's grounded-state RGB
value. The preview applies that value directly to the production avatar renderer;
it does not show a live callback transition.
