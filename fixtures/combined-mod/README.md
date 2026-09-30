# Jade garden — combined mod example

This single format-2 package registers its own PNG-backed placeable cube,
server-authorized stone-to-jade action (costing one stick), persistent scheduled
chunk-owner growth, a downloaded client startup module and authored UI, and a
WGSL albedo shader on the cube's texture. Run from the repository root with a
**new** save directory, in separate terminals:

```sh
cargo run --release -- server-packages fixtures/combined-mod/packages 127.0.0.1:4000 /path/to/new-jade-garden-save
cargo run --release -- client 127.0.0.1:4000
```

For one-process local iteration with your normal admin profile, run:

```sh
RUST_LOG=warn,bloxgloom=info,bloxgloom::script=debug cargo run --release -- local-packages fixtures/combined-mod/packages /path/to/new-jade-garden-save
```

Use F4 `give bloxgloom:stick 16` when you need sticks. A dedicated server does
not grant admin access by default. After changing scripts or assets, stop the
server, select a fresh save directory when content identity changes, and
restart/reconnect. The client downloads and verifies the new artifact; no
Rust recompilation is needed when running an already-built game binary.
`RUST_LOG` controls structured stderr diagnostics; package/module errors retain
source identity. Server callbacks run in fresh VMs, so persistent state belongs
in the transaction or owner bytes rather than module globals.

With script DEBUG enabled, downloaded client startup logs `Garden presentation prepared`
after checking a binary buffer, and the planting callback logs `Plant proposed`.
Both include seeded random samples. These describe execution attempts; the
action receipt establishes whether the edit and cost committed. Retries may
repeat the planting log. See [runtime tools](../../docs/modding/RUNTIME-TOOLS.md)
for library support, seed inputs and diagnostic budgets. This example needs
matching client/server binaries supporting client host contract 3.

F4 `verdant:noon` calls the public Luau clock API to set daylight to noon.
It requires the same authenticated admin profile as `time set noon`; a mod
cannot grant itself admin access. Its server-only callback reads the captured
phase, stages the change and observes its own staged result. Clock commands
share the gameplay WAL, rollback and restart path with world/inventory effects.

Select a stick, aim at visible stone within reach, then press **F6** and click
**Plant in aimed stone** (or focus it with Tab and press Enter). The UI passes
only the action key; the client samples its current streamed-world ray hit and
the server authorizes the target, inventory cost and WAL edit. A denied action
does not consume the stick. The system independently turns one stone cell at
`(3, 80, 0)` into jade on its first eligible tick; generated terrain there may
not be stone, so that example change is only guaranteed in the deterministic
test setup. The package does not grant items or alter world authority on the
client. The UI heading is set by downloaded client startup code.

Run `cargo test combined_mod_downloads_acts_grows_and_recovers` for the
isolated real-listener test. It stages the scheduled owner through the normal
receipt-gated WAL path, connects a client without local package installation,
activates the authored button, then reopens the save to check both world edits,
the finite inventory cost and owner state. It verifies the material's selected
catalog texture layer, but it is **not** a release-window visual inspection.
To render just this package's authored UI (including the downloaded startup
heading) without opening a game window, run:

```sh
cargo run -- ui-preview /path/to/output-dir fixtures/combined-mod/packages
```

The preview does not show jade world geometry or validate live gameplay visuals.

`cargo test combined_mod_mixed_load -- --nocapture` exercises two real clients
performing 320 authored actions, edits and finite transfers during fresh bundle
downloads and cancellations. Movement acknowledgements and automatic growth
continue, and reopening the save checks both profiles' balances and owner
progress. Stale chunk observations are denied and retried from fresh client
observations. Timings measure client response processing, not display latency.

This example uses the compatible version-1 albedo material and one-state cube
registration. The engine also supports explicit legal block states, multiple
version-2 material targets, typed parameters, composed effect graphs and public
replica callbacks; see [Prism](../visual-packages/prism/README.md) and the
[Phase 4 showcase](../phase4-showcase/README.md). Imported models and live
reload remain deferred. The bundled Roboto Mono subset and its license come
from `fixtures/ui-target-actions/`; the jade PNG and WGSL example come from
`fixtures/material-packages/jade/`.
