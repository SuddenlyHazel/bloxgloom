# Targeted authored UI fixture

Run from the repository root with an isolated save:

```sh
cargo run -- server-packages fixtures/ui-target-actions/packages 127.0.0.1:4000 /path/to/new-ui-target-save
cargo run -- client 127.0.0.1:4000
```

Select a stick, aim at a visible stone block within reach, then press **F6**.
Click the button (or Tab then Enter). One stick pays for turning the currently
aimed stone into glowstone. The UI displays the server receipt or denial; it
does not supply coordinates, entity identity, inventory revision or permissions.
Aim is sampled when the completed callback is dispatched, not captured when
the document opens. No stick, wrong target, blocked sight or a changed target
cannot apply the transaction. Close the panel with F6 to aim elsewhere.

The automated `authored_block_action_current_aim_denials_and_reconnect_recovery`
test prepares deterministic terrain and finite inventory, downloads this package
through the real nonblocking listener, runs the actual client callback/aim/send
path, and verifies denial, retry, fresh reconnect and durable recovery.

This slice supports registered **Gameplay block actions** only in addition to
the existing item/empty actions. It does not add entity targets or arguments.
Block requests retain the existing server block-type check, not a new terrain
revision fence: same-type replacement is not distinguished. The bundled font
and its license are copied unchanged from the existing `uidemo` fixture.
