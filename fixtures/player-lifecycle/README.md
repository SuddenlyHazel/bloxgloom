# Player lifecycle package

Run from the repository root with a fresh temporary save:

```sh
cargo run -- server-packages fixtures/player-lifecycle/packages 127.0.0.1:7878 /tmp/bloxgloom-player-lifecycle
```

Connect a client, verify three sticks, disconnect and reconnect with the same
profile, then restart the server against the same save. The reward remains three
sticks. The private kit flag and reward share the main WAL record; temporary
participation state starts fresh on every connection.

See [the active API reference](../../docs/modding/PLAYER-LIFECYCLE.md).

Open the command console and enter `/welcome:inspect <name>`. Tab completes a
unique name prefix to an exact session token. The server logs the selected player;
an ambiguous, departed or stale target is rejected before the callback runs.
The command is permitted for ordinary admitted players; it grants no admin role.
