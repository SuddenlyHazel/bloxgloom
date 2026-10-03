# Scripted health, damage and respawn

Run from the repository root with a fresh isolated save:

```sh
cargo run --release -- local-packages fixtures/player-health/packages /tmp/bloxgloom-health-demo-v26
```

Press F4 to open the native command menu. Run `demo:health`, `demo:hurt 25`,
`demo:heal 10` and `demo:kill`. The native health bar updates immediately after
the server's durable receipt. Killing yourself opens a death screen; click
**Respawn** to return to a supported, unobstructed origin spawn at full health.
Inventory is kept. Death/respawn notices are visible in chat history (T).

Quit while dead and restart with the same save: you remain dead. Repeating a
request never repeats death effects. Ordinary movement and inventory/world
actions are unavailable while dead. The native respawn request remains usable.

`damage.luau` is a pure server damage policy; returning zero cancels damage.
`hooks.luau` demonstrates transaction-bound death/respawn callbacks. Both reload
with F4 `reload packages` when their declared contracts remain unchanged.
No fall/contact/environment damage is automatically enabled by this fixture.
See [the health contract](../../docs/modding/PLAYER-HEALTH.md).
