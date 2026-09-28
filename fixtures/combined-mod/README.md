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

The renderer still supports only one package albedo target and one-state cube
geometry; this example does not demonstrate imported models, typed shader
parameters, general client replica callbacks, or live reload. The bundled
Roboto Mono subset and its license come from `fixtures/ui-target-actions/`;
the jade PNG and WGSL example come from `fixtures/material-packages/jade/`.
