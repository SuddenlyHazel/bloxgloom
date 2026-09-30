# Phase 8: examples, parity and integrated verification

Acceptance date: September 30, 2026. All non-deferred Phase 8 work is complete.
The [implementation plan](IMPLEMENTATION-PLAN.md) defines the agreed scope;
this record covers its final phase and distinguishes host mechanisms, gameplay
policy, Luau bindings and deferred features.

## Authoring and runnable examples

[SCRIPTING.md](../../SCRIPTING.md) is the complete implemented Luau binding
inventory, including capabilities, arguments, callback events, bounds, delivery,
authority and save rules. [Local packages](LUAU-PACKAGES.md) explains the manifest
and worker/session workflow; [IDE setup](IDE.md) supplies editor configuration.
The editor definitions now include storage, creatures, machines, every gameplay
event kind, authenticated admin services and the daylight clock.

| Example | What it exercises |
| --- | --- |
| [Jade garden](../../fixtures/combined-mod/README.md) | One package: texture/block/item, authored UI action, finite stick cost, scheduled growth, downloaded startup text, WGSL material and public admin clock command |
| [Phase 4 showcase](../../fixtures/phase4-showcase/README.md) | Stateful machine, creature, public replica presentation and downloaded assets |
| [Neighborhood](../../fixtures/neighborhood/README.md) | Cross-chunk reads/edits, durable intents and scheduled owner work |
| [Targeted actions](../../fixtures/ui-target-actions/README.md) | Current aim, server reach/observation checks, authorized receipts and denials |
| [Prism](../../fixtures/visual-packages/prism/README.md) | Version-2 materials, parameters, effect composition and replica-driven presentation |

Strict Luau LSP analysis passes for the garden's four server modules and the
showcase's three server modules using `types/bloxgloom.d.luau`. Definitions are
editor-only; runtime ownership, schema and budget checks remain authoritative.
Local iteration uses an already-built binary: restart the selected package set,
use a new save when identity changes, then reconnect to download the new artifact.
Structured `tracing` diagnostics retain package/module identity and use `RUST_LOG`.

## Production parity audit

The audit follows live startup, dispatch and publication, rather than treating
legacy test-only helpers as production policy. Ordinary non-deferred builtins
use public host contracts. Shared authoritative collision, allocation, storage,
dependency capture, admission, WAL and replication remain engine mechanisms.
They validate both native and scripted proposals; exposing raw mutable engine
state is not required to compose supported gameplay.

| Area | Production route and public surface | Result |
| --- | --- | --- |
| Content and generation | [Builtin catalog](../../src/content/builtins.rs), public block/item/texture/tag declarations; [builtin contributor](../../src/world/generation.rs) and Luau generators share generation composition | Public; frozen identities and builtin numeric IDs retained |
| Harvest, support and semantic decisions | [Builtin policy](../../src/gameplay.rs) uses public `gameplay::Context`; [adapter](../../src/server/gameplay.rs) shares block, entity, inventory and drop participants, including anchored invalidation | Public; server reads and lifecycle/refund validation retained |
| Finite inventories and stock actions | [Slot policy](../../src/gameplay/slot_move.rs), [drop-stack policy](../../src/gameplay/drop_stack.rs), registered actions and public exact component transfers | Public; 128 stack cap, receipts and stable profile ownership retained |
| Containers, kiln and hopper | [Startup](../../src/server/startup.rs) enumerates frozen declarations; [builtin machines](../../src/content/machines.rs) use public `Machine`, `Processor` and `DownwardFlow`; Chest supplies the public storage lifecycle | Public; old direct hopper/kiln test planners are not the production route |
| Creatures and generic entities | [Mossbun](../../src/content/creatures/mossbun.rs) implements public mobile `Behavior`; registered gameplay schemas/actions/ticks share ownership and durable planning | Public; host locomotion, collision and allocation retained |
| World drops | Public falling, merge/pickup/expiry policies in [host entity services](../../crates/host-api/src/entity.rs); [builtin pickup](../../src/gameplay.rs) calls exact public collection/routing | Public; eligibility, player credit, age and explicit pickup events stay server-owned |
| Player behavior and appearance | Negotiated public [player](../../crates/host-api/src/player.rs) and [appearance](../../crates/host-api/src/appearance.rs) contracts; shared movement/spawn/collision service | Public; model/palette selection remains negotiated presentation |
| Commands and daylight | [Builtin admin policy](../../src/gameplay/admin.rs) uses the same authenticated give/spawn/time operations as Luau; registered requests and compatibility packets use durable dispatch | Closed in Phase 8; time is a public action service and a WAL participant |
| Persistent scheduled work | [Public owner adapter](../../src/server/startup/public_systems.rs), registered entity ticks and durable intent/wake paths | Public; state, deadlines and supported effects recover together |
| UI and visuals | Shared live egui renderer, verified package resources, public material hooks/parameters, effect graph and replica worker | Public supported presentation; callbacks stay off the window thread |
| Fire propagation/delivery | Native frontier/propagation/delivery; committed burns still use shared removal services | Explicit user-deferred migration; the subsequently fixed cue and paced wood/leaves spread do not imply migration |

Two gaps found during this phase were closed. The day/night admin command now
uses `world_time()` and `admin_set_time()` through the public gameplay plan.
Clock reads fence manual changes; setting the phase commits with block, entity
and inventory effects and the action receipt. Clock-only actions receive accepted
receipts. Callback errors, caught invalid operations and permission denials roll
back the complete action. The last command survives WAL rotation; a matching
periodic checkpoint retains elapsed phase, while a lagging checkpoint resumes
from the committed command. Daylight services are bound in action callbacks;
other callback kinds retain their documented logical tick services.

Luau startup also no longer limits a package to one action: up to 32 actions
freeze and deliver together. Projection and decoding preserve that bound;
duplicates, excess declarations and malformed metadata fail atomically even
when declaration errors are caught. The shared registry retains its existing
256 total actions and eight per exact target limits, including builtins.

The Rust host API is broader than the Luau manifest. Native item-icon callbacks
and general anchored-behavior registration remain native extension services;
Luau uses documented textures and storage/machine lifecycles. This is disclosed
in the binding inventory, rather than claiming every Rust interface is bound.
Imported/custom player models, arbitrary meshes/collision/liquids, sound, raw GPU
or network access, hot reload, save converters and additional languages are not
introduced by this phase. Native fire migration remains explicitly deferred;
marketplace/CDN infrastructure remains outside scope.

The clock checkpoint and compatibility command format changed. Default saves
are **`world-v18` / `world-v18-fixture`**, and wire version is **13**. Use fresh
saves and matching client/server builds; no prerelease world converter is added.

## Integrated behavior and responsiveness

The garden regression runs through the production nonblocking TCP listener and
real client network, startup, authored UI worker, command ledger and replicas,
with an isolated temporary save. Two profiles repeatedly plant jade, move a
finite stack out and back, break/rebuild their targets and send movement intents.
Both targets share a chunk, exercising real stale-observation denials. A denial
does not debit inventory; retry captures a fresh observation without bypassing
the terrain fence. Inventory replication is awaited independently of receipts.

The scheduled owner runs automatically under load, rather than being manually
advanced by the fixture. Reopening the save verifies owner revision progress,
its jade edit, both rebuilt targets, both profiles' exact balances and empty
temporary slots. The same package's authenticated `verdant:noon` command runs
through the client console and public Luau clock callback before the workload.

Focused final run: **320 successful action/edit/transfer samples**, **64 movement
samples**, **61 completed fresh bundle transfers**, **30 cancellations**, and
**37 safe stale-observation denials/retries**, over **11.45 seconds**.

| Client response processing | Median | p95 | Maximum |
| --- | ---: | ---: | ---: |
| Authored action / edit / transfer (action timing includes fresh-observation retries) | 53.06 ms | 86.97 ms | 168.60 ms |
| Movement acknowledgement | 23.69 ms | 37.68 ms | 50.46 ms |

The regression guards against starvation with coarse p95 <500 ms and maximum
<2 s bounds. These measurements include local loopback, UI callback/dispatch and
response processing as applicable; they are not internet or display latency
guarantees. This is a representative two-player workload, not a 128-player soak.
The existing large-package/machine overlap test also passed: 15 complete 1.5 MiB
transfers and seven cancellations, with 16 edit acknowledgements (p95 46.64 ms)
while movement, lighting/meshing and kiln output progressed.

Existing real-listener suites cover failed startup/readiness, healthy retry,
cancelled preparation, cache reuse, reconnect, stale epochs and cross-server
switches. Visual-contract suites cover per-server catalogs/parameters, resource
reset, authoritative replica inputs, attributed failures and restart identity.
The clock regression separately reopens after every failed callback before any
success can mask a partial write, restores lagging clock/inventory checkpoints,
and verifies common-WAL recovery and old-session denial.

## Visual inspection and rendering context

No release game window was present in the available app inventory. The §13
generated-preview fallback was used. Release garden UI previews at
[1280×720](phase8/garden-ui-1280x720.png) and [640×360](phase8/garden-ui-640x360.png)
were inspected: the startup title, button and focus indication are visible and
readable. The compact preview's native help strip clips its long text; the
authored card itself remains within the viewport.

Garden and Prism material/composition images were generated and inspected through
production meshes, voxel lighting and package pipelines. Retained evidence shows
the [garden material](phase8/garden-material.png) and
[Prism composition](phase8/prism-composition.png). The garden has its green v1
albedo, while Prism colors the selected stone and composes its scene effects;
cutout foliage remains visible. These are offscreen images, not a live-window
switch or display-latency recording. The earlier user-reported garden live check
is supplementary evidence.

Release `perf 300 6` / `perf 300 6 bounced`, Apple M1 Pro/Metal at 1280×720,
compared with the Phase 7 final record. Each run has 163 upload-ramp frames and
300 steady frames; setup is excluded. [Normal output](phase8/perf-normal.txt) and
[bounced output](phase8/perf-bounced.txt) retain the exact methodology.

| Metric | Phase 7 normal | Phase 8 normal | Phase 7 bounced | Phase 8 bounced |
| --- | ---: | ---: | ---: | ---: |
| Setup (ms) | 2295.5 | 2329.0 | 2657.1 | 2781.3 |
| Mesh bytes | 17,292,744 | 18,573,688 | 17,500,968 | 18,797,336 |
| Visible triangles | 88,026 | 88,026 | 89,218 | 89,218 |
| Steady CPU p50 / p95 (ms) | 0.301 / 0.581 | 0.313 / 0.541 | 0.326 / 0.588 | 0.317 / 0.428 |
| Steady GPU p50 / p95 (ms) | 0.259 / 0.527 | 0.289 / 0.414 | 0.238 / 0.426 | 0.289 / 0.382 |

Triangles are unchanged. Mesh bytes increased by four bytes per vertex following
the separately landed day/night lighting change (13 vertex floats rather than
12). Phase 8 makes no rendering/meshing algorithm change. CPU submission and GPU
timestamps are separate; these terrain-only numbers exclude authored shader cost,
network gameplay and presentation. Short-run timing variation is reported, not
treated as a measured improvement or regression caused by this phase.

## Reproduce verification

```sh
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test combined_mod -- --nocapture
cargo test clock -- --nocapture
cargo test world_time -- --nocapture
cargo test package_action_set -- --nocapture
cargo run --release -- ui-preview /tmp/phase8-ui fixtures/combined-mod/packages
cargo run --release -- visual-preview /tmp/phase8-garden fixtures/combined-mod/packages verdant:jade
cargo run --release -- visual-preview /tmp/phase8-prism fixtures/visual-packages prism:stone
cargo run --release -- perf 300 6
cargo run --release -- perf 300 6 bounced
```

Final verification passed: **1,083 game tests and 34 host API tests**,
`cargo fmt --all -- --check`, strict all-target/all-feature Clippy and the
seven-module strict Luau analysis described above. Release builds and the
retained UI/material/composition previews completed successfully.
