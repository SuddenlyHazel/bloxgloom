# Public persistent owner systems

`Registrar::owner_system` exposes the existing owner-worker and durable-wave
runtime through `bloxgloom_host_api::system`. It is entity-independent background
computation; chunk owners may opt into authoritative read-only terrain, but it
is not yet a terrain-editing API.

An extension declares a namespaced system, schema fingerprint, partition
(chunk/entity/profile), state-byte bound, job budget, simulation-phase dependencies,
seed owners, and a pure behavior. Each invocation receives its owner identity,
revision, logical tick, and immutable canonical bytes. It returns replacement bytes
and a strictly later deadline. The host validates bytes at seed, recovery, and
result boundaries, fences revisions, journals the wave, and publishes it only after
confirmation. Cursor and deadline persistence retain fairness and scheduling across
restart. Recovered owner values take precedence over startup seeds.

Bounds are 64 KiB per value, 16,384 seed owners per system, 16,384 jobs per system
per tick (also constrained by aggregate phase limits), and 128 public systems.
Missing/cyclic dependencies fail registry construction before play. Handshake/save
manifests include `Y` owner-system identities and descriptor fingerprints, including
schema, bounds, dependencies, and seeds. Runtime values remain private journal data.

The separately compiled `fixture:region_clock` owns two chunk keys. It advances a
durable counter every fifty logical ticks without requiring a creature or machine.
The real nonblocking-listener test starts a fixture world, connects a loopback
client, runs the system, restarts, checks exact recovered values, and verifies
continued progress. Additional tests reject oversized outputs, nonadvancing
deadlines, incompatible manifest schemas, and missing dependencies.

This supplies the persistent-system portion of the external integration proof.
`read_radius_chunks: Some(0)` gives a chunk-partitioned system (at most 64
jobs per tick) `Context::block(cell)` within its owner chunk. `Some(1)` allows
the immediate 3×3×3 chunk neighborhood (at most 8 jobs per tick); `None` grants
no world read. The host captures **all** required resident authoritative chunks
for each worker, defers the entire wave and asynchronously requests missing
chunks, and reserves every captured chunk's read key through WAL admission and
receipt. Out-of-scope cells are `Unavailable`, never procedural fallback. The
owner-only and `Some(0)` declaration fingerprints are unchanged; `Some(1)`
has its own manifest identity. The independent `fixture:world_probe` and
`fixture:neighbor_probe` exercise missing chunks, authoritative air, scope
rejection, conflicting same/adjacent-chunk edits, and replay, including a
real nonblocking-listener join.

`Plan::wakes` can name other registered `(system, owner)` pairs. Each job may
request at most 32 and a wave at most 2,048. The host validates the destination
partition and journals wake flags together with the producing owner's new
bytes, deadline and cursor. A flagged destination runs from its ordinary job
budget on a later tick, even if its persisted deadline is further away; an
unloaded owner retains the flag through restart until it is present. Duplicate
flags coalesce. A served flag reasserted in the same wave is replaced by one
WAL change rather than a conflicting clear and set. This is **scheduling**, not
payload delivery: the destination must decide its work from its own durable
state/world reads, not from an advisory observation or wake timing. The
separately compiled `fixture:wake_pair` proves a one-shot cross-owner wake
across restart; `fixture:wake_loop` covers refreshing served flags and recovery.
The owner-world loopback test also installs both declarations together and
checks the restored destination after a real nonblocking-listener join.

This still does **not** complete world-system parity: atomic terrain/entity
effects, dynamic owner creation/removal, and durable
cross-owner **payload intent** remain explicit followups, as do public fire propagation
and delivery. Built-in support removal uses the shared neighbor decision path
for existing edit producers rather than an owner-system callback.
An entity/profile owner key is an identity, not a grant to mutate that entity or
player. Callbacks are trusted deterministic Rust functions, not sandboxed plugins.
