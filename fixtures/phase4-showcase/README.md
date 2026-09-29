# Phase 4 Luau showcase

`packages/demo` is a local package for the supported Phase 4 authoring path. It
defines a five-part mobile sproutling, a two-cell fueled stone press with an
inventory screen, explicit idle/lit block states, and a client replica callback
that colors the sproutling from its public grounded flag.

Use the package directory as the local package root when starting a server.
These headless previews show the authored assets and screen:

```sh
cargo run -- creature-preview demo:sproutling /tmp/sproutling.png fixtures/phase4-showcase/packages
cargo run -- inventory-preview demo:press_machine /tmp/press-screen fixtures/phase4-showcase/packages
cargo run -- block-preview 'demo:press[lit=on]' /tmp/press-lit.png fixtures/phase4-showcase/packages
```

The real-listener integration test `phase4_showcase_creature_machine_and_replica_survive_real_join_and_restart`
downloads the package into a client, spawns the creature, verifies the worker's
state-derived tint, restarts the server from the same save, and checks the stable
catalog and entity identities. The previews do not show live tint transitions.
