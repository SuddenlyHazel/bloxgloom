# Authored entity UI fixture

The package registers one stationary marker creature and one entity action.
Its document button sends two binary argument bytes (`7, 9`) through the
session-owned Luau presentation worker. The client supplies the currently
aimed replica identity and revision; the server checks the live entity,
arguments, selected stick, and inventory capacity before committing one stick
to one seed.

Run from the repository root with an isolated save:

```sh
cargo run -- server-packages fixtures/ui-entity-actions/packages 127.0.0.1:4000 /path/to/new-ui-entity-save
cargo run -- client 127.0.0.1:4000
```

Use the admin console to spawn `uitarget:marker`, select a stick, aim at the
marker, and open the package document with F6. The listener test creates the
ground and inventory, spawns the marker, clicks through the downloaded UI,
checks the request bytes and receipt, denies altered bytes, and rejoins after
WAL recovery to confirm both the saved inventory and entity identity.
