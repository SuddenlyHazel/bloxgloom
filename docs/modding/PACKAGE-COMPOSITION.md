# Larger Luau packages and independent simulation features

One package can register several separately named owner systems and generation
contributors without a dispatcher or artificial dependency packages. The Lua
registration signatures are unchanged. Repeated calls require distinct keys in
the declaring package's namespace. Registrations are canonicalized before
installation, preserving numeric catalog identity when only declaration order
changes. Saved contracts also remain stable under ordinary behavior source edits;
explicit schema/revision/layout and relevant dependency metadata remain fences.
Scripted generation additionally protects its exact server/shared source closure.
Fresh saves remain the prerelease path for incompatible installations; save
conversion remains excluded. See [package development](PACKAGE-DEVELOPMENT.md)
and [save compatibility](SAVE-COMPATIBILITY.md).

## Independent systems

`register_system` admits eight systems per package. Each declares its own module,
schema/revision, seed owners, state bound, access contract and job budget. Shared
callback modules do not share owner bytes, deadlines, cursors, receipts or intents.
Mutable Lua attempts remain isolated; persisted state belongs in owner bytes.

`after` accepts up to 16 same-package or declared direct-dependency system keys.
The existing native planner rejects missing targets, forbidden dependencies,
self-edges and cycles before opening a world. An edge orders execution phases;
it does not permit reading another system's private state. Cross-system wakes
remain scheduling hints; durable intent payloads stay within their system.
The installation-wide ceiling remains 128 owner systems including builtins, and
native aggregate phase budgets still constrain the combined workload.

## Deterministic terrain contributors

`register_generator(key, revision, module)` admits eight contributors per package
and 256 per installation. Builtin terrain runs first, then lexical namespaced-key
order; later contributors win overlapping writes. Registration order does not
establish precedence. Every contributor samples builtin terrain rather than an
earlier contributor's partial output. Use explicit names such as
`farm:10_wild_crops` and `farm:20_groves` when precedence matters.

All scripted contributors share a 100 ms wall-time execution allowance per
candidate chunk. Each invocation uses its remaining allowance, capped at the existing 50 ms per-call limit, and
existing instruction/output bounds. Failed contributors discard the complete candidate;
no earlier output or procedural fallback is published. Queue delay, builtin
terrain, meshing and delivery are separate from this script allowance.

## Shared admission policy

Authoritative constants live in
[`capacity.rs`](../../src/server/script/capacity.rs) and are consumed by discovery,
startup declaration collection and client bundle verification.

| Resource | Limit |
| --- | --- |
| Packages / direct dependencies per package | 64 / 32 |
| Blocks / total items / textures per package | 256 / 512 / 256 |
| Owner systems / contributors per package | 8 / 8 |
| Modules / assets per package | 256 / 256 |
| Modules / assets across installation | 1,024 / 1,024 |
| Manifest / module source bytes | 64 KiB / 64 KiB |
| General asset file bytes | 2 MiB |
| Discovered file bytes across installation | 32 MiB |
| Encoded client bundle, including metadata | 40 MiB |
| Startup per package | 250 ms, 50,000 periodic interrupt checks, 16 MiB VM |
| Installation initialization | 10 seconds |
| Client source preparation | 10 seconds, 16 MiB compiler VM |
| Script execution per candidate chunk | 100 ms across contributors |
| Native content declaration collection | 4,096 declarations and 64 MiB estimated bytes |
| Registered PNG copies | 64 MiB per package and installation |
| Material texture array | 128 MiB / 1,536 layers, further constrained by adapter |

Block registration also consumes a total-item slot: 256 blocks leave room for
256 standalone items. Installation-wide catalog ceilings and builtin consumption
still apply: 32,768 block types, 131,072 block states, 32,768 items and 8,192
textures across the installation, including builtins. Stack size remains 128; default gameplay calls remain bounded at
50 ms and 8 MiB. Import depth and retained client realm admission are unchanged.

Encoded size is not decoded resource size. PNG dimensions/pixels, font expansion,
shader validation and renderer limits are independent. Package authors should
measure the complete installation rather than multiplying local maxima. A valid
manifest or successful discovery alone does not prove client preparation success.
Server/client contract negotiation rejects unsupported capacity profiles before
a download or content acknowledgement. ContentReady still requires a matching
frozen catalog, verified bundle and successful preparation.

## Example and acceptance

[`fixtures/farming-scale`](../../fixtures/farming-scale/README.md) contains 128
blocks, 192 total items, eight textures across four families, 96 modules, three systems and two
contributors. Irrigation durably wakes growth ahead of its own deadline; seasons
have independent state/deadlines. The downloaded panel requests finite planting
and harvesting transactions. The real-listener acceptance test confirms seed
128→127, produce 127→128 and independent owner recovery after restart.

The separate deterministic pressure generator produces 1,024 modules and 1,024
valid RGBA assets across four packages, roughly 16 MiB of file content. These
are synthetic admission inputs, not gameplay padding. Boundary options permit
maximum-plus-one checks independently from the representative farming mod.

Baseline measurement used the immutable test binary from commit `4e3a46a`,
on this development machine, through the production listener (legacy reporter
uses `floor((n-1) × percentile)` order statistics): 320 authored
operations and 64 movement acknowledgements under repeated fresh downloads and
cancellations. Action median/p95/max were 53.274/93.802/133.457 ms; movement
median/p95/max were 19.208/43.918/52.464 ms. It completed 72 transfers and cancelled
35 attempts; 27 safely fenced stale observations were retried. The paced workload
lasted 11.415 seconds. An isolated repeat of the same immutable old binary
reported action median/p95/max 56.348/103.681/162.749 ms and movement
18.144/38.514/54.815 ms, with 73 fresh transfers and 36 cancellations. This
run-to-run variation is relevant when comparing the final workloads. The
preexisting harness did not report p99. These are client
response-processing timings, not rendering latency or universal performance
promises.

## Memory and decoded-resource admission

Cold download/verification reserves `3 × offered bytes + 2 × 64 KiB` against a
128 MiB process-wide transient ledger before requesting bytes. This charges the
download buffer, canonical retained bytes, decoded payload and protocol scratch
at their maximum overlapping stages. Wrapper verification releases its decoded
intermediate before copying the outer canonical artifact. Admission failures and
cancelled transfers release reservations.

Retained verified bundles separately reserve `2 × encoded bytes + complete
estimated content declaration bytes` (including PNG copies once) against 256 MiB
and at most 32 live artifacts. Cache and session `Arc`s share one reservation, released after the last reference. Startup
texture clones are checked against 64 MiB per package and installation before
publication; repeatedly referring to a large PNG cannot bypass accounting just because source bytes are shared.
The single-entry cache is bounded; reconnects reuse the verified artifact rather
than multiplying its retained payload. Typed residency pressure may evict only an
unused cached artifact and retry the already downloaded bytes once. Active or
retiring sessions keep their bundles pinned. Ordinary transfer corruption
preserves the cache; a memo evicted specifically to relieve residency pressure
stays discarded if subsequent validation fails. This permits replacing large
unused cached artifacts without granting unbounded concurrent residency.

Decoded resources retain their existing bounds: the authored UI atlas is
1,024×1,024 RGBA (4 MiB), aggregate UI image area is 262,144 pixels, and at most
four fonts admit 128 glyphs each with 512 points per glyph. Effect/custom shader
and pass budgets remain independently enforced. World PNG decode is serial,
with a 16 MiB decoded-image ceiling and resampling to 128×128 texture layers.
Each full RGBA mip chain occupies 87,380 bytes per texture; array-layer support
and aggregate GPU allocation are checked separately from encoded PNG size.
The array is capped at 128 MiB/1,536 layers, including builtin layers, and the
actual adapter may permit fewer. Device creation requests supported array limits
explicitly; expanding a package beyond the default 256 device layers no longer
relies on a later validation failure. Admission precedes pixels/mip allocation.

VM policy is unchanged from the lifetime work: a lane's compiled-code cache is
bounded at 128 entries/4 MiB; startup and source validation reserve 16 MiB each,
and ordinary gameplay reserves 8 MiB. Aggregate VM admission is 256 MiB/64 VMs.
These are policy/reservation ceilings, not measured occupied heap values for the
acceptance process.
Native declaration admission estimates structure, strings, states and PNG bytes
before Lua pending collection and client metadata publication, and checks the
combined installation before merging. Its 64 MiB estimate includes overhead;
it can reject repeated large texture aliases before the separate 64 MiB raw-PNG
subbudget is full. Pending package collection and accumulated native declarations
have separate bounded lifetimes; transient coexistence can reach 64 MiB each.

Neither encoded admission nor a source-pixel estimate is a measured GPU peak.

New acceptance reporters use nearest-rank `ceil(n × percentile)-1` indices.
The 12-sample cold/cached join runs have coarse tails: p95 and p99 both select
the maximum sample. They establish observed bounded progress, not a reliable
population tail estimate.

## Verification record

The frozen workspace suite passed 1,285 game tests and 37 host API tests with
two threads in 274.35 seconds. Four benchmarks/explicit pressure probes were
ignored by the ordinary suite; the generated maximum-count pressure probe was
run explicitly as described above. Formatting verification and strict
all-target/all-feature Clippy passed.
The release build passed. Final measurements/previews are recorded below.

### Final package measurements

These debug-binary probes ran sequentially without concurrent builds or graph
updates on the Apple M1 Pro development machine. Join timing ends at `Welcome`
after bundle verification, source preparation and catalog agreement. It excludes
window/GPU resource installation. RSS is the whole test process maximum reported
by macOS `time -l`, not a bundle allocation or steady-state heap measurement.

| Probe | p50 | p95 | p99 | Samples |
| --- | --- | --- | --- | --- |
| Farming cold verified join | 228.469 ms | 267.368 ms | 267.368 ms | 12 |
| Farming cached reconnect | 173.912 ms | 178.542 ms | 178.542 ms | 12 |
| Larger installation cold verified join | 675.811 ms | 741.957 ms | 741.957 ms | 12 |
| Larger installation cached reconnect | 196.143 ms | 213.527 ms | 213.527 ms | 12 |
| Farming authored actions/edits/transfers | 51.326 ms | 87.674 ms | 107.528 ms | 320 |
| Farming movement acknowledgements | 17.833 ms | 35.621 ms | 52.483 ms | 64 |
| Farming plus large transfers: authored operations | 46.367 ms | 87.045 ms | 107.090 ms | 320 |
| Farming plus large transfers: movement | 20.588 ms | 38.747 ms | 58.316 ms | 64 |

The representative mixed workload completed 67 fresh downloads and cancelled
33, with 25 safely fenced stale-observation retries. The larger installation
combined farming with three pressure packages: 864 discovered modules, 780
assets and approximately 12.8 MB (12.2 MiB) of file content. It completed nine fresh
downloads and cancelled four, with 33 safe retries. Both workloads verified
finite inventories, unrelated movement, growth progress and recovery after
restart. Their paced gameplay portions lasted 11.156 and 10.482 seconds.
The larger installation's 12,784,526-byte encoded bundle reserved 38,484,650
transient verification bytes; discovery/startup took 2.192 seconds for the join
probe. Its cold/cached quantiles above exercise the target-byte installation,
including repeated preparation rather than just transfer completion.
These results show progress under this workload; lower timings are not a claim
that adding content makes the engine faster.

The independent four-package maximum-count probe discovered 1,024 modules and
1,024 valid PNG assets (17,045,648 file bytes). Server discovery/startup took
2.807 seconds; delivered source validation/preparation took 42.721 ms. Its
16,990,297-byte bundle joined over real TCP in 841.161 ms and charged 51,101,963
bytes to transient verification admission. PNG source pixels totalled 16,777,216
bytes, but these unregistered assets allocate no material GPU array. That pixel
count is an input-size calculation, not an observed decoded/GPU peak.

| Probe | Maximum process RSS | macOS peak memory footprint |
| --- | --- | --- |
| Old combined baseline mixed load | 90,079,232 bytes | 38,027,936 bytes |
| Farming cold/cached joins | 90,882,048 bytes | 38,634,192 bytes |
| Larger installation cold/cached joins | 262,193,152 bytes | 210,076,560 bytes |
| Farming mixed load | 92,438,528 bytes | 41,026,208 bytes |
| Farming plus large transfers | 293,208,064 bytes | 252,134,264 bytes |
| Maximum-count pressure join | 226,557,952 bytes | 178,406,216 bytes |

The farming join measurement used the original 41,828-byte bundle. Visual
inspection then shortened only the client panel heading, removing 12 encoded
bytes; the final standalone bundle is 41,816 bytes. The large mixed fixture
used the corrected heading. No gameplay declaration or callback behavior changed.

### Graphics verification and controls

Production release previews were rendered and inspected at 1,280×720 and
640×360. Inspection caught a clipped title; the shortened heading and both
buttons now fit. Production block previews covered a farming crop and a
256-texture/256-block/512-item package selecting a texture layer above 256,
exercising actual expanded device-array admission rather than only a unit test.
These are headless production renderer checks, not a live gameplay-window test.

`perf 300 6` retained exactly 18,573,688 mesh bytes and 88,026 visible triangles.
Scene setup, CPU submission and GPU pass timings remain separate:

| Release run | Scene setup | CPU steady p50/p95/p99 | GPU steady p50/p95/p99 |
| --- | --- | --- | --- |
| Earlier old binary | 2,325.6 ms | 0.315 / 0.428 / 0.472 ms | 0.235 / 0.322 / 0.372 ms |
| Initial new binary | 2,384.5 ms | 0.430 / 2.229 / 6.766 ms | 0.437 / 2.537 / 5.132 ms |
| Immediate paired old control | 2,335.0 ms | 1.415 / 3.063 / 3.552 ms | 0.908 / 3.115 / 4.261 ms |
| Immediate paired new control | 2,387.8 ms | 0.629 / 2.375 / 5.074 ms | 0.661 / 3.247 / 6.006 ms |

The immediately repeated old binary also showed elevated tails, demonstrating
substantial environmental variation. The resource guard affects initialization;
it adds no steady frame work, and geometry is identical. These controls do not
establish a precise performance improvement or a hardware-independent frame
latency promise. Rendering measurements are separate from script/join timings.
