# Phase 4 Luau showcase

`packages/demo` is a local package for the supported Phase 4 authoring path. It
defines a five-part mobile sproutling, a two-cell fueled stone press with an
inventory screen, explicit idle/lit block states, and a client replica callback
that colors the sproutling from its public grounded flag and emits a short-lived
blue spark. It also emits a larger, longer-lived spark at the press's installed anchor. The mobile
callback handles at most eight creatures per batch within the 16-command
output limit.

For a live check, spawn `demo:sproutling` through F4 and right-click its body
within reach. Each pat updates a small private counter and should show
`Interaction applied`; the sproutling does not wander or play a pat animation.
Give yourself `demo:press`, `bloxgloom:stone`, and `bloxgloom:stick` through F4.
In the press screen, click stone in the lower player inventory and then INPUT;
click stick below and then FUEL. Click OUTPUT and then a player slot to take
gravel. Transfers use the current server slots even if the press ticked after
the client rendered them; left-click moves up to a stack and right-click one.

To inspect the package in the local game with your normal admin profile, use
an isolated save directory:

```sh
cargo run --release -- local-packages fixtures/phase4-showcase/packages /tmp/bloxgloom-phase4-showcase-save
```

Package script changes alter the save's content identity. Use a new save path
when trying a changed showcase package; keep the old directory for comparison.

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
finite inventory item. It transfers finite stone and fuel through the screen,
waits for two gravel from the authored process, takes the output into the player
inventory, and places four more finite blocks while the creature and press run.
It restarts the server from the same save and checks the stable catalog/entity
identities, both press cells, withdrawn output, inventory balance, four saved
edits, and an anchor-attached spark. The
test logs four edit response times as a small mixed-load sample. The second
creature in the tinted preview uses the
callback's grounded-state RGB value. The preview applies that value directly
to the production avatar renderer; it does not show a live callback transition.
The downloaded client also aims at and pats the creature through its registered
interaction twice. Restart checks the resulting private `happy:2` state, while the
client receives only public pose bytes.
