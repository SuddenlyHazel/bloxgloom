# Player lifecycle package

The server audit callback logs committed inventory revisions through the readonly
observer lane. It is advisory; first-join rewards use durable profile decisions.

Run from the repository root with a fresh temporary save:

```sh
cargo run -- server-packages fixtures/player-lifecycle/packages 127.0.0.1:7878 /tmp/bloxgloom-player-lifecycle
```

Connect a client, verify three sticks, disconnect and reconnect with the same
profile, then restart the server against the same save. The reward remains three
sticks. The private kit flag and reward share the main WAL record; temporary
participation state starts fresh on every connection.

Press F6 to open the authored profile panel. It reads `level:1` from the server's
explicit public projection through SessionReady/PlayerStateChanged callbacks.
Client disconnect callbacks run off the window thread, log the retired session,
and cannot apply stale replies to a replacement connection. Private kit/session
bytes are not delivered.

Render the initial panel with:

```sh
cargo run -- ui-preview /tmp/bloxgloom-player-panel fixtures/player-lifecycle/packages
```

See [the active API reference](../../docs/modding/PLAYER-LIFECYCLE.md).

Open the command console and enter `/welcome:inspect <name>`. Tab completes a
unique name prefix to an exact session token. The server logs the selected player;
an ambiguous, departed or stale target is rejected before the callback runs.
The command is permitted for ordinary admitted players; it grants no admin role.

The Admin command `/welcome:manage <player> 2` (builder) or `3` (ban) updates
package-owned policy through ordinary gameplay transactions. Builder is a role
for this package's rules; it does not grant native operator privileges. Ban saves
private policy and kicks the exact selected session after receipt; the admission
callback rejects subsequent joins for that profile, including after restart.
`/welcome:inspect` logs the package policy alongside the selected session.
These are policies on claimed profiles, with the engine's existing identity trust.

`/welcome:manage <player> 1` applies the registered server uniform. These modes
share one command so the example respects the target-action discovery bound.
