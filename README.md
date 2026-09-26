# Bloxgloom

Bloxgloom is a Rust multiplayer voxel game. The dedicated server owns a procedural, editable world; the desktop client renders streamed chunks with `wgpu` and uses `winit` for input.

The client/server event flow and tick loop are documented in [docs/runtime-architecture.md](docs/runtime-architecture.md). The active foundation contract and its remaining acceptance gates are in [docs/growth-foundation-plan.md](docs/growth-foundation-plan.md); it is not a claim that the whole foundation is complete. The earlier simulation design and implementation record remain in [docs/server-simulation.md](docs/server-simulation.md).

The world generates on demand as players travel, with no fixed horizontal boundary. Temperature, moisture, and uplift create plains, forests, deserts, tundra, and rocky highlands with distinct landforms and surface layers. A deterministic wave-function-collapse pass makes constrained ground-cover patches that match across independently generated regions. Biome-aware flowers, ferns, grass, and broadleaf trees add vegetation that stays consistent across chunk borders; plants can be broken and collected. Caves remain below the surface, but the new-world spawn has a solid floor beneath it; the world has an immutable solid bottom at Y = −64. Built-in blocks use pixel-art assets in `assets/textures/`. Opaque blocks use greedy chunk meshes; foliage uses a separate cutout mesh. The sky has world-anchored clouds and shares a fixed sun direction with terrain lighting, so the sun moves across the view when you turn.

Voxel skylight travels down open columns and diffuses into caves; placeable glowstone emits warm local light. This is the default lighting mode. In Settings, `LIGHTING: BOUNCED` enables a more expensive single diffuse RGB bounce from block surfaces, including color bleed. It is a voxel approximation, not path tracing or multi-bounce GI. Lighting is derived from nearby chunk snapshots on meshing workers and refreshed after edits or quality changes, including across chunk seams. Mesh corners average nearby light for soft transitions, and unlit cave fog stays dark. An unstreamed neighboring chunk uses its procedural baseline until the server snapshot arrives.

The current default save directory is `world-v7/`. Incompatible older worlds are rejected explicitly; this pre-release project does not provide world-upgrade tooling. Development checks and benchmarks use isolated temporary directories and do not delete repo-local saves.

Blocks, legal block states, items, entity types, and texture layers have namespaced definitions in a startup content catalog. New worlds record their numeric ID mapping in `content.map`; a world refuses to load when an existing ID is reassigned or required content is missing, and multiplayer rejects clients with a different catalog. Save and wire content IDs are widened to 32 bits. This is groundwork for future mod loading, not a mod-file format or scripting API yet.

### HDR presentation

The world renders into a linear `RGBA16Float` scene buffer, followed by quarter-resolution soft-threshold bloom and neutral, fixed-exposure tone mapping to the display. Glowstone and the sun retain HDR highlights; the HUD and selection outline are drawn afterward. Exposure does not adapt when entering caves. This is an internal HDR pipeline with SDR output, so no HDR monitor is required.

Open **Escape → Settings → Graphics** to adjust exposure and bloom strength live. **Post Effects: Off** bypasses tone mapping, exposure, and bloom; **Bloom: Off** disables just bloom. Toggles preserve the adjustment values for re-enabling, and all changes save automatically. Mouse controls and Tab/arrow-key navigation both work.

The corresponding config keys are `post_processing=true`, `bloom_enabled=true`, `exposure=1.0` (range `0.25`–`4.0`), and `bloom_strength=0.12` (range `0`–`1`). Zero bloom strength also skips bloom passes. On macOS the file is `~/Library/Application Support/Bloxgloom/config`; Linux uses `$XDG_CONFIG_HOME/bloxgloom/config` or `~/.config/bloxgloom/config`, and Windows uses `%APPDATA%/Bloxgloom/config`. Restart after manual file edits. Existing configs use the defaults for missing keys. Image previews and the headless benchmark use the same post-processing pipeline at its default settings.

## Run locally

With a recent Rust toolchain, run a local game with one command:

```sh
cargo run
```

This starts a local server and client in the same process and saves edits in `world-v7/`. Cube-face textures live in `assets/textures/blocks/`, leaf and plant cutouts in `assets/textures/foliage/`, and non-block item art in `assets/textures/items/`.

For a dedicated multiplayer server, start the server in one terminal:

```sh
cargo run -- server 127.0.0.1:4000 world-v7
```

Start one or more clients in other terminals:

```sh
cargo run -- client 127.0.0.1:4000
```

The server defaults to `127.0.0.1:4000` and saves edits in `world-v7/`. To connect from another computer on your LAN, bind the server to a reachable address (for example `0.0.0.0:4000`) and pass that computer's address to the client. Admission defaults to 128 clients and can be configured up to 256; 128-client loopback TCP baselines have passed, but the combined gameplay acceptance workload remains unverified.

Click the window to capture the mouse. Use WASD to fly horizontally, Space and Shift to ascend and descend. The crosshair marks the targeted block: left click harvests it, and right click places a block from the selected hotbar stack against it. Flowers drop themselves; tall grass can drop seeds, and leaves can drop leaves, sticks, and saplings. Seeds, sticks, and saplings are inventory items, not placeable blocks. Walk near a drop to pick it up. Use 1–9 or the mouse wheel to select a hotbar slot. Press Q to drop one selected item, or Shift+Q to drop its full stack.

E opens the 36-slot inventory (27 backpack slots and nine hotbar slots). Select a source slot, then left-click a destination to move its whole stack; right-click the destination to move half. Matching stacks merge up to 128 blocks; moving a full stack onto a different block swaps them. Escape opens the pause menu, where you can resume, change settings, or exit. F3 toggles the debug HUD. The local game also has an admin menu on F4 or the pause menu: click a catalog item to grant a stack of 128, or type `give namespace:item [count]` and press Enter. `help` lists available commands. These grants are authorized and persisted by the local server; dedicated multiplayer servers do not grant admin access by default.

**Meet the Mossbun:** stand on an open patch of ground, open the local admin menu with **F4**, type `spawn mossbun`, and press **Enter**. Close the menu to watch your mint-colored, rosy-cheeked little companion wander and pause. Each command spawns one nearby on clear supported ground (at most 16 Mossbuns in the destination chunk; crowded entity pages also reject spawning). They persist across saves/restarts, wander even without connected players, avoid cliffs and solid blocks, and settle if their floor is removed. They do not jump, climb steps, fight, consume items, or spawn naturally. `spawn bloxgloom:mossbun` is equivalent.

Mossbuns now choose nearby destinations and use bounded worker-side A* to walk around obstacles on level ground. Shared creature locomotion handles body clearance, ledges, accelerating gravity, and exact landing; the persisted idle decision timer is separate from support rechecks. Client actor interpolation smooths authoritative updates, with distance-driven paws, smooth turning, idle breathing, airborne stretch, and landing squash. See [creature movement](docs/CREATURE-MOVEMENT.md) for the reusable layers and current navigation capabilities.

Blocks are now finite: the server owns inventory, drops, pickup, and placement. Breaking a block pops its drop upward; resting drops hover and spin, then fly toward the player when picked up. A full inventory leaves drops in the world. Inventory and world drops persist in the server save directory; drops expire after ten minutes. Each OS user has a persistent local profile ID for their inventory and saved position; simultaneous connections with that same profile are rejected. Exiting the local game saves the last authoritative position, and restarting restores it unless that position has become obstructed. Movement remains server-authoritative with block collision; gravity and other survival systems are not implemented yet. The inventory/drop protocol is versioned; older clients must be rebuilt.

The server targets a 50 Hz fixed-step simulation even with no clients connected. A coordinator owns authoritative state; movement, fire, registered entity tick policies (including drops and kiln ticks), and owner handlers run on workers. Publication workers prepare interest, snapshots and committed updates. Disk writes run on workers, but simulation and publication barriers can wait for results or durable receipts. See the execution diagrams below for the distinction between implemented parallelism and pending work.

## Current execution architecture

These diagrams describe the current Rust execution paths. **Solid arrows are implemented paths. Dashed arrows and nodes labelled `PENDING` are planned work, not active execution.** Entity tick policies and registered owner handlers use separate worker dispatchers but share durable admission and ordered receipt handling. The reviewed implementation record and remaining work are in [EXECUTION-FOUNDATION-PLAN.md](EXECUTION-FOUNDATION-PLAN.md).

### Server threads and workers

```mermaid
flowchart TB
    Clients["Connected clients"] <--> Reactor["Socket reactor: one nonblocking I/O thread"]
    Reactor <--> Codec["Protocol workers: 4 decode + 4 encode"]
    Reactor <--> Join["Join inventory loading: 4 workers"]
    Reactor <-->|"bounded input and outbound queues"| Coordinator["Simulation coordinator: authoritative state, 50 Hz target"]
    Coordinator -->|"immutable terrain snapshots"| Movement["Player movement: parallel worker jobs"]
    Movement -->|"results at barrier"| Coordinator
    Coordinator --> Fire["Fire: private source, encode and apply worker pools"]
    Fire -->|"prepared results"| Coordinator
    Coordinator -->|"bounded immutable captures"| EntityWorkers["Entity tick workers: drop physics, kiln ticks, registered policies"]
    EntityWorkers -->|"ordered plans at barrier"| Coordinator
    Coordinator --> Owners["Registered owner handlers: parallel worker jobs"]
    Owners --> Logs["Owner-write construction: scoped worker threads"]
    Logs -->|"validated results"| Coordinator
    Coordinator <-->|"chunk requests and results"| Loader["Chunk loading and terrain generation: 2 workers"]
    Coordinator -->|"transactions"| WAL["Journal worker: append and fsync"]
    WAL -->|"durable receipts"| Coordinator
    Coordinator --> Checkpoints["Checkpoint write workers + fenced entity mirror serialization"]
    Coordinator -->|"immutable committed effects and shared page captures"| PublishWorkers["Publication workers: interest, drop visibility, snapshots and delta projection"]
    PublishWorkers -->|"prepared frames at barrier"| Coordinator
    Coordinator -->|"validated outbound frames"| Reactor
    Coordinator --> Serial["Serial: authorization, actions, captures, validation, apply, pins and queue admission"]

    Fire -.-> SharedFire["PENDING / DEFERRED: fire cutover to generic owner infrastructure"]
    classDef pending fill:#fff4cc,stroke:#946200,stroke-dasharray:5 5,color:#222;
    class SharedFire pending;
```

- **Player movement and fire computation are parallel today.** Fire retains its own scheduler, mailboxes and checkpoint machinery; its generic-path migration is deferred.
- **Entity tick policies and registered owner handlers run on workers**, with immutable inputs and ordered result collection. The coordinator constructs and validates their transactions; entity interactions remain coordinator-planned.
- **Publication preparation runs on workers:** nearest-interest traversal, drop visibility, snapshot checksums, and committed block/entity/player projection. Matching frames share prepared content and encoded bytes through the existing codec workers; sharing is batch-local.
- Input authorization, block edits, inventory actions, pickups, trusted transfer construction, effect routing and authoritative apply remain coordinator work. The coordinator also captures bounded inputs, manages subscriptions/pins, validates publication results and admits outbound frames.
- **Worker dispatch does not make the tick loop nonblocking.** Simulation and publication use explicit barriers; socket I/O continues independently.

### Tick and durable commit paths

```mermaid
flowchart TB
    Input["Input authorization"] --> Durable["Durable actions"]
    Durable --> Simulation["Simulation"]
    Simulation --> Interaction["Interaction commit"]
    Interaction --> Publish["Publish"]
    Publish --> Next["Next fixed-step tick"]

    Durable --> EntityPlan["Entity tick workers: immutable inputs to plans"]
    EntityPlan --> Motion["Coordinator: validate and batch eligible motion, up to 256 candidates"]
    Motion --> Admission["Shared admission: exact read/write dependencies, bounded pending queue"]
    Actions["Coordinator-planned actions and interactions"] --> Admission
    OwnerJobs["Owner workers: replacements, effects and next deadlines"] --> Admission
    Admission -->|"assign entity publication order on acceptance"| Journal["Main server.wal: atomic transactions, append and fsync"]
    Journal --> Receipt["One ordered receipt queue: later receipts cannot overtake"]
    Receipt --> Apply["Coordinator: confirmed apply, release reservations, emit effects"]
    Apply --> Publish
    Receipt --> Barriers["Explicit barriers: motion tick boundary and last owner admission in each phase"]
    Barriers --> Next
    Apply --> Wake["Wake hints: eligible no earlier than next tick"]
    Wake --> Schedule["Bounded admission: active/due owners, due entities, hints and suspended rechecks"]
    Next --> Schedule
    Schedule --> EntityPlan
    Schedule --> OwnerJobs
```

The five tick phases run in order. The commit branches above show shared mechanisms, not additional threads or a claim that every system runs in every phase.

- Entity actions and owner waves share admission, a **256 pending-commit bound**, and ordered receipt/apply handling. An owner phase barrier completes through its last accepted admission, including earlier work from other producers. Ordinary non-motion actions and fire can remain pending across ticks.
- A tick that stages a motion batch drains staged receipts before completing its durable phase. This removes ordinary receipt-poll timing from admitted drop steps, at the cost of waiting for the journal. Terrain availability, admission limits and rotation can still defer work.
- Built-in durable actions remain coordinator-ordered. Atomic pickup and cross-entity transfers put all affected state into one transaction; a receipt gates visibility.
- **Conflict identity is separate from publication order.** Independent entity updates can coexist in flight, including within one owner chunk. Entity records, terrain, membership pages, anchored cells and queried absence retain their real dependencies. Shared readers coexist; writers remain fenced until those readers apply. Shared page changes and ID allocation can still serialize.
- Owner handlers choose `Active` or a future `AtTick` deadline. Bounded indexed selection uses circular fairness; state, deadlines, owner wake flags and rotation cursors persist through the same journal. Exact transient ready-set order is not preserved across restart.
- Suspended tickable entities remain in a derived recheck index rebuilt on recovery. Bounded circular rechecks discover changed support even if every entity wake hint is lost. Supported sleeping drops reaffirm without motion or WAL writes. Recheck latency depends on population and admission opportunities.
- Effect consumers schedule work rather than directly modifying world or inventory state. Entity hints are coalesced and capped at 256; durable owner flags retain absent-destination work. Neither determines item ownership.

### Publication and checkpoint boundaries

```mermaid
flowchart TB
    Apply["Confirmed coordinator apply"] --> Effects["Ordered immutable committed effects"]
    Effects --> Projection["Bounded publication workers: group, filter and prepare client frames"]
    Pages["Immutable subscription captures, mobile pages and resident chunk handles"] --> Projection
    Projection --> Validate["Coordinator barrier: validate session/revisions, update subscriptions, enqueue"]
    Validate --> Codec["Existing codec workers: encode matching shared messages once"]
    Codec --> Socket["Reactor: shared byte buffers, independent per-client budgets"]

    Apply --> Mirror["FIFO entity checkpoint mirror worker"]
    Rotation["Rotation boundary: close admission and drain accepted receipts"] --> Fence["Constant-size mirror fence request"]
    Fence --> Mirror
    Mirror --> BGEN["Stream fenced entity checkpoint, fsync and atomic rename"]
    Apply --> Dirty["Bounded dirty values: select at most 16 keys per dispatch"]
    Dirty --> Files["Checkpoint workers: per-key file writes"]
    BGEN --> Coverage["Verify checkpoint coverage and drain dirty files"]
    Files --> Coverage
    Coverage --> Base["Journal worker: stream ordered base, durably switch manifest"]
    Base --> Resume["Release fence and reopen admission"]
```

- Publication uses batches of at most **16 worker jobs**, with one committed effect fully projected before the next. Stale outputs cannot advance subscriptions. Oversized transaction projection triggers complete resnapshots; impossible snapshots or failed per-client delivery disconnect the affected session rather than truncate updates or stop other clients.
- Snapshot and effect projection still wait at synchronous barriers. Bounded selected public-view capture, authoritative bookkeeping and queue admission remain on the coordinator; there is no cross-tick publication cache.
- Entity checkpoints stream from the existing worker-owned mirror; journal bases stream from an ordered latest-values map. There is no whole-generation output buffer or coordinator entity-store clone. Serialization turns process at most **16 schema-bounded entries**, stopping after reaching 64 KiB; one entry may exceed that target. Individual output/checksum pieces are at most 64 KiB.
- **Total checkpoint work is still proportional to saved state.** Dedicated workers finish each command through bounded turns; these are not interleaved executor jobs. Rotation can hold admission closed across multiple ticks, and filesystem operations have no fixed latency guarantee. Startup recovery remains population-sized; ordinary per-key snapshots are whole schema-bounded values. Fire's private complete-map checkpoint remains outside this streaming guarantee.

### Client execution and remaining extension work

```mermaid
flowchart TB
    Server["Server"] <--> Net["Client network reader / writer threads"]
    Net <-->|"bounded message queues"| Window["Window thread: input, replicas, UI, render submission"]
    Window --> Mesh["Meshing workers: lighting and chunk mesh generation"]
    Mesh -->|"revision-tagged results"| Window
    Window --> Config["Config-save worker"]
    Window --> Adapters["Entity presentation / interaction registry: public views only"]
    Window --> GPU["GPU rendering: current texture-array materials"]

    GPU -.-> Paging["PENDING: logical texture IDs mapped to GPU pages / layers"]
    Adapters -.-> Extension["PENDING: independent startup-extension crate"]
    Extension -.-> Registration["PENDING: complete custom effect-kind and persistent-domain registration"]
    classDef pending fill:#fff4cc,stroke:#946200,stroke-dasharray:5 5,color:#222;
    class Paging,Extension,Registration pending;
```

Client presentation never decides item ownership. Server snapshots and explicit pickup events control authoritative state; drop hover, spin and pickup flight are visual effects. Worker mesh/light results carry revisions so stale results can be discarded.

The next foundation task is the independent extension crate, completing any missing startup registration hooks through a real consumer. Texture paging remains a separate rendering task. Known limits include local rejection/backoff for permanently oversized neighbour-dependent views and atomic owner-wave deferral delaying other owners in that wave. Drop and kiln policies skip unused neighbour capture, so ordinary dense drops no longer hit that neighbour-view limit. Public mod loading and scripting remain future work.

Code entry points: [`src/server/runtime.rs`](src/server/runtime.rs), [`src/server/runtime/systems.rs`](src/server/runtime/systems.rs), [`src/server/durable/admission.rs`](src/server/durable/admission.rs), [`src/server/durable/receipt.rs`](src/server/durable/receipt.rs), [`src/server/durable/entity_dispatch.rs`](src/server/durable/entity_dispatch.rs), [`src/server/durable/publication/dispatch.rs`](src/server/durable/publication/dispatch.rs), [`src/server/streaming.rs`](src/server/streaming.rs), [`src/server/entity_checkpoint/worker.rs`](src/server/entity_checkpoint/worker.rs), and [`src/client/workers.rs`](src/client/workers.rs).

## Development and previews

The client logs FPS, frame-time percentiles, visible chunks, triangles, and upload backlog every five seconds. Run `cargo test` for the world, protocol, server, UI, and meshing checks. The interface implementation and validation record are in [PLAN.md](PLAN.md).

For visual debugging without a desktop display, run `cargo run -- preview preview.png` or `cargo run -- preview desert.png -928 -1024` to center the render near specified world coordinates. This renders terrain through the same GPU shader and mesh pipeline and writes a PNG that can be inspected directly.

Run `cargo run -- ui-preview ui-previews` to render the Playing, Inventory, Admin, Pause, and Settings screens at 1280×720, 640×360, and 640×360 with 2× requested UI scale, plus sun-facing and sun-away views, without opening a game window.

Run `cargo run -- lighting-preview lighting-previews` to compare a sealed cave, a lamp under default lighting, and the same lamp under bounced lighting through the production mesh and GPU shader pipeline.

Run `cargo run -- vegetation-preview vegetation-preview.png` to inspect trees and plant cutouts through the production GPU path without opening a window.

Run `cargo run -- drop-preview drops.png` to render a few textured world drops through the production GPU pipeline without opening a game window.

Run `cargo run -- mossbun-preview mossbuns.png` to inspect two Mossbuns and a player scale reference through the production actor meshes and shader, without opening a window.

Run `cargo run --release -- mossbun-motion-preview mossbun-motion-previews` for six frames of walking, stopping, falling, and landing through the production client animator and actor shader.

Run `cargo run -- drop-animation-preview drop-frames` to inspect the pop, hover, and pickup states as three headless GPU renders.

Run `cargo run --release -- perf 300 6` to measure headless 1280×720 chunk-upload, world-render, target-outline, and HUD work at the maximum supported view radius. It reports CPU submit-side and GPU render-pass frame-time percentiles, adapter, and scene size. It does not measure window presentation or live gameplay FPS.

Run `cargo run --release -- server-perf 300` to measure separate 16-player clustered and spread authoritative server workloads at 50 Hz. It uses isolated temporary saves and reports tick, backlog, worker, chunk-load, WAL, and replication metrics. This headless benchmark does not measure TCP socket writes or client graphics.

Run `cargo run --release -- server-perf fire-cpu --workers 1 --iterations 3000` and repeat with `--workers 4` for matched fire-compute measurements. Run `cargo run --release -- server-perf tcp --clients 128 --ticks 15000 --scene clustered` (or `spread`) for paced production-listener TCP measurements. These are separate workloads; a network baseline alone does not establish the combined foundation gate.

Append `bounced` to benchmark the optional lighting mode, for example `cargo run --release -- perf 300 6 bounced`. Scene setup includes light-field construction and meshing; its time is reported separately from steady frame samples.
