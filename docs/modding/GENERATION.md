# Chunk generation for native contributors

The host passes a `bloxgloom_host_api::generation::Context` and bounded `Output`
to each contributor. Use `context.world_position(local)` for absolute `i64` XYZ
coordinates (`local` must be `[0, 16)` on every axis). `context.random_at`
mixes the seed, absolute position and a chosen salt; it does not depend on
chunk iteration or call order. Registered contributors run in lexical key order
after built-in terrain; later writes replace earlier ones at the same cell.

`context.builtin_terrain_height(x, z)` gives the built-in column's ground
height at absolute X/Z. `context.builtin_base_block([x, y, z])` returns the
namespaced built-in **base terrain** state key, including caves and surface
materials, before trees, plants, contributor writes and saved edits. These
queries use the actual host terrain algorithm, not a copy in host-api. They
depend only on seed and absolute coordinates, not the queried chunk or mutable
world state. Both return `SampleError::OutOfBounds` outside the addressable
coordinate range `i32::MIN * 16 ..= i32::MAX * 16 + 15` on each input axis.
Contexts constructed with `Context::new` outside the host lack a terrain
provider and return `SampleError::Unavailable` for in-range queries.

For cross-chunk features, choose **one anchor in absolute coordinates** (for
example, one candidate per global grid cell, with its own salted hash). That
anchor owns the decision that the feature exists and its shape. Each intersected
destination chunk independently recomputes the same anchor from the seed and
absolute cell and emits **only the portion within its own local bounds**. Scan
all anchor cells whose maximum feature radius/height can intersect this chunk,
including negative neighbors (use Euclidean division for negative coordinates).
Do not consider only anchors inside the current chunk: that clips features at
seams. Do not try to write neighboring cells through `Output::set`, whose local
coordinates must stay in `[0, 16)` and whose total writes (including repeated
writes to one cell) cannot exceed 4096 per contributor invocation. Resolve
overlapping features in a stable absolute anchor order so every destination
chunk makes the same decision. Built-in trees use this same intersection rule;
sampling the base block does not include their decoration or later contributors.

## Luau contributor composition

A package can call `host.register_generator(key, revision, module)` eight times
with distinct owned keys. The installation still permits at most 256 contributors.
Builtin terrain runs first, then contributors in lexical namespaced-key order;
later overlapping writes win. Registration call order has no effect. Contributors
sample builtin terrain rather than earlier contributor output. All scripted calls
share a 100 ms execution allowance per candidate chunk, with each call receiving
the smaller of its remaining allowance and the existing 50 ms per-call limit,
with existing instruction/output limits. Failure discards
the complete candidate; it does not publish earlier contributors or fallback air.
Physical VM/compiled-code reuse preserves fresh mutable authoritative attempts.
See [farming scale](../../fixtures/farming-scale/README.md) for two named terrain
contributors and [package composition](PACKAGE-COMPOSITION.md) for capacity.

## Persisted algorithm identity

`world.meta` records each contributor key, declared revision and optional frozen
source digest. Native contributors default to no digest and must bump their
revision when the algorithm changes. Authored contributors can implement
`Contributor::source_identity()` with a precomputed `[u8; 32]`; the host captures
it when installing the generator. The method must not read files.

Luau contributors provide SHA-256 over their entry and all server/shared source
in the declaring package and transitive dependencies, including relevant manifest
contracts. Client-only code and visual assets are excluded. Changed source at an
unchanged revision still rejects an existing world, so newly explored terrain
cannot silently switch algorithms. See [save compatibility](SAVE-COMPATIBILITY.md).
