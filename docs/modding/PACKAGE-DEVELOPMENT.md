# Package development: manual reload and download caching

Start the native local development game:

```sh
cargo run --release -- local-packages my-packages ./world-package-dev
```

Save your edits, open F4, and enter **`reload packages`**. Finish all related file
edits before issuing it: discovery freezes one set of bytes, but filesystem
editing is not an atomic multi-file transaction. Nothing watches files or reloads
automatically. The command requires the local server's admin profile and an
explicit package development root. Normal builtin startup has no reload root.
`server-packages` currently has no configured admin profile, so this command is
available through `local-packages` rather than its dedicated-server CLI.

The server keeps simulating during discovery and validation on a worker. It
compiles all listed modules, reruns server startup registration, compares the
frozen runtime contract, preflights native client startup and prepares the new
handshake frames. Errors appear in the native status message and keep the
current code/resources active. Only one reload can be pending at a time.

| Edit | Live reload |
| --- | --- |
| Existing server behavior modules and imported helpers | Yes, if startup declarations and persistent contracts remain equal |
| Existing client/shared modules, UI, sounds and compatible presentation resources | Yes, if resource preparation and client startup succeed |
| Package identity/version, module/asset manifest or dependencies | Restart required |
| Registered blocks/items, IDs, physics, palettes, state schema, owner plan/seeds, creature defaults | Restart required |
| Catalog-bound PNG textures and GLB models or their controls | Restart required |
| Terrain generator package or transitive dependency server/shared source | Fresh save required; generation retains startup sources |

Successful reload switches one immutable revision at a coordinator boundary
after accepted WAL work has applied. Each callback and all its imports retain
one revision throughout execution. Retained readonly server realms reset at
their next callback; authoritative attempts remain isolated. Old revisions stay
alive only while referenced, subject to a shared 256 MiB / 64 snapshot budget
for discovered file bytes, in addition to existing bundle/VM/resource limits.

Connected clients automatically retire the old session and rejoin the same
address using the ordinary background package/GPU preparation path. A join
already transferring an older artifact keeps that handshake intact and receives
another refresh after admission. The server world stays open. Finite inventories
remain keyed by stable profile, and durable entities, owner values, deadlines
and terrain stay authoritative. Reconnect creates a new session/action epoch;
client UI/module locals, session-only state and movement mode reset normally.
An in-flight readonly callback can finish on the previous immutable revision.

Compatible behavior edits also survive process restarts: saved identities depend
on explicit registration contracts and relevant package/dependency metadata,
without hashing ordinary source. Client-only source edits do not change terrain
identity. Generator server/shared sources remain protected even at an unchanged
revision, including every module in their transitive package closure. Explicit
schema, layout, revision or content changes can still require a fresh save;
restart alone does not make them compatible. See [save compatibility](SAVE-COMPATIBILITY.md)
for the complete boundaries and error evidence. No save conversion is provided.

## Persistent client package cache

Verified canonical bundle bytes are stored under native cache storage:

| Platform | Directory |
| --- | --- |
| macOS | `~/Library/Caches/Bloxgloom/package-bundles-v1/` |
| Windows | `%LOCALAPPDATA%/Bloxgloom/Cache/package-bundles-v1/` |
| Linux and other Unix | `$XDG_CACHE_HOME/bloxgloom/package-bundles-v1/`, falling back to `~/.cache/bloxgloom/package-bundles-v1/` |

The disk cache is capped at **512 MiB / 64 entries**, pruning oldest entries on
write. Files include the negotiated runtime version and SHA-256 bundle identity.
A cache hit still decodes and verifies the bytes against the exact server offer;
it does not bypass runtime compatibility, catalog agreement or client startup.
The join display reports cached package reuse and skips the bundle download.

Writers use a shared file lock and atomic temporary-file replacement. Interrupted
temporary writes are cleaned on a later write. Corrupt/truncated entries are
discarded and downloaded again. Unavailable storage or write contention logs a
cache warning and continues joining. The cache contains neither world saves nor
private server modules and can be removed while the game is stopped.

Regression coverage includes cold disk reuse after clearing the process cache,
corruption and capacity recovery, real nonblocking TCP reload authorization,
rejected source/contract changes, preserved inventory, changed durable behavior,
stale handshake refresh, retained import reset and callback revision consistency.

Verification on October 3, 2026: `cargo test --workspace` passed 1,685 game tests
and 52 host API tests, with 10 intentionally ignored game tests. All six reload
regressions passed again after the final revision-lifetime change. Formatting,
strict all-target/all-feature Clippy and the release build passed. The release
command-menu UI preview was rendered and inspected; live window reload behavior
is covered by the joining-shell test and the production TCP integration test.
