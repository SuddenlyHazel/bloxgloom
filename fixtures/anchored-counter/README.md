# Persistent anchored counter

This package declares a two-block-tall counter with a private binary state and
a smaller public projection. Placing it costs three `counter:block` items;
removing either footprint cell or losing support clears both cells and refunds
two items. The host owns footprint reservation, inventory debit, persistence,
and refund delivery.

Run the local game with your normal admin profile and an isolated save:

```sh
cargo run --release -- local-packages fixtures/anchored-counter/packages /tmp/bloxgloom-counter-save
```

Use F4 to give yourself at least six `counter:block` items. Place the post on a
solid floor with two empty cells above it. Placement consumes three items, even
though the footprint contains two blocks. Right-click either cell to open its
registered action, then activate it to increment the counter exactly once.

Place a solid block immediately east of the lower cell to set the public
neighbor signal; remove it to clear the signal. Break the floor directly below
the lower cell to remove the whole post. Alternatively, break either post cell.
Removal refunds two items as world drops, rather than one item per cell.

The reaction callback observes current terrain on periodic work and advisory
terrain wakes. It never counts invocations as elapsed time. The persisted
polling deadline ensures support checks do not depend on wake delivery.

Stop and restart against the same save to retain the post's identity, count,
neighbor signal, and private original placement height. Only the count and
signal are sent to clients. This fixture does not add a counter-value UI; its
public projection is available to the engine's public replica services.

For a separate server:

```sh
cargo run --release -- server-packages fixtures/anchored-counter/packages 127.0.0.1:7878 /tmp/bloxgloom-counter-server-save
cargo run --release -- client 127.0.0.1:7878
```

Inspect the block through the production lighting, meshing, and GPU path:

```sh
cargo run -- block-preview counter:block /tmp/bloxgloom-counter.png fixtures/anchored-counter/packages
```

Use a new save directory after changing package code; this prerelease does not
convert persisted package schemas. The texture reuses the repository's small
Jade fixture asset; it is registered as package-owned counter art.
