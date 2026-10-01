# Agent guidance

- Avoid god modules: keep each Rust module focused on one area of responsibility, and split growing files into well-scoped submodules before unrelated systems accumulate.
- Keep the process lightweight. Use the amount of planning, documentation, and review that helps the task; avoid ceremony for its own sake.
- Treat performance and correctness as compatible goals. Choose clear designs that meet both, and measure performance when a tradeoff matters.
- Write useful tests that protect behavior or catch realistic regressions. Do not add tests merely to increase the test count.
- Keep changes focused and explain meaningful tradeoffs plainly.
- Commit often; write good commit messages.

## Architecture and invariants

- The server owns world state, edits, and movement. The client streams snapshots/deltas and must not treat procedural fallback chunks as authoritative.
- The server also owns finite inventories and world drops. Preserve the 128-block stack cap, do not let placement or slot moves create items, and keep player inventories keyed by stable profile ID. Inventory files and `drops.bin` in local world directories are save data.
- Register blocks, items, and textures in the startup `content::Catalog` before it is installed. Builtin numeric IDs are save/wire identities; never recycle one for a different namespaced key. New worlds persist `content.map` as save data, and client/server catalogs must match at handshake. The catalog is frozen during play so workers can use read-only, constant-time lookups.
- Drop pop, hover/spin, and pickup flight are presentation-only. The server sends item age and explicit pickup events; do not make client animation timing decide inventory or world-drop ownership.
- Keep network I/O, config writes, lighting, and meshing off the window thread. Client light/mesh jobs carry revisions so stale results can be discarded; edits must refresh affected chunk seams.
- Voxel lighting is the default. Bounced lighting is an optional, single-bounce approximation; preserve dark sealed caves and correct relighting after edits when changing either mode.
- Keep opaque cube-face art in `assets/textures/blocks/`, alpha-cutout leaves/plants in `assets/textures/foliage/`, and non-block pickups in `assets/textures/items/`. Add new assets to the matching folder so the material library stays navigable.
- Put new code in the focused modules: `src/client/` for workers/events, `src/render/` for mesh/material/pipeline/sky/visibility, `src/ui/` for layout/drawing, and `src/preview/perf.rs` for the headless benchmark. Keep tests in adjacent `src/<module>/tests.rs` files.
- Folders like `world-v3/` and `world-v4/` are local test save data, these can be treated like disposable build artifacts. Use a unique temporary save directory for tests and benchmarks.
- This prerelease has no player worlds to upgrade. Do not build world-format or entity-schema converters now: if changes cause incompatibilities increment the world folder version target (eg. v6, etc). 

## Verify graphics and performance

- For server networking or lifecycle changes, exercise the real nonblocking-listener path with a loopback client and an isolated temporary save. A blocking test listener and `server-perf` do not validate game startup; inspect the release client window when available.
- If you cannot observe the game window, use `cargo run -- preview ...`, `ui-preview ...`, or `lighting-preview ...` and inspect the generated images. Do not claim a visual change is good based only on compilation or tests.
- Use `cargo test`, `cargo fmt --all -- --check`, and `cargo clippy --all-targets --all-features -- -D warnings` for relevant changes. For rendering or meshing changes, benchmark `cargo run --release -- perf 300 6` and, when relevant, append `bounced`; compare scene setup, mesh size, CPU frame time, and GPU frame time separately. This benchmark excludes presentation and live gameplay.

## graphify

This project has a knowledge graph at graphify-out/ with god nodes, community structure, and cross-file relationships.

When the user types `/graphify`, use the installed graphify skill or instructions before doing anything else.

Rules:
- For codebase questions, first run `graphify query "<question>"` when graphify-out/graph.json exists. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts. These return a scoped subgraph, usually much smaller than GRAPH_REPORT.md or raw grep output.
- Dirty graphify-out/ files are expected after hooks or incremental updates; dirty graph files are not a reason to skip graphify. Only skip graphify if the task is about stale or incorrect graph output, or the user explicitly says not to use it.
- If graphify-out/wiki/index.md exists, use it for broad navigation instead of raw source browsing.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review or when query/path/explain do not surface enough context.
- After modifying code, run `graphify update .` to keep the graph current (AST-only, no API cost).
