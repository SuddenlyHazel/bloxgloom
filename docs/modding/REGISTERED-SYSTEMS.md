# Public persistent owner systems

`Registrar::owner_system` exposes the existing owner-worker and durable-wave
runtime through `bloxgloom_host_api::system`. It is entity-independent background
computation; chunk owners may opt into authoritative read-only terrain, but it
supports bounded conditional block edits through the same owner WAL record.

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

`Plan::edits` accepts up to 16 conditional block-state transitions per job
(256 per wave), sourced inside the owner chunk. The host checks the captured
preimage and routes removal, placement and neighbor decisions through the
shared gameplay planner. Resulting terrain writes within the declared read
neighborhood commit atomically with owner bytes, deadline, cursor and wakes;
players and anchored entities block unsafe placements. Gameplay-generated
entity and drop participants commit in that same WAL record; effects that
cannot be admitted defer the entire owner wave. `fixture:world_writer` verifies
one placement and owner state recover together from the journal.

An opt-in `Behavior::edit_cause() == EditCause::Burn` uses the shared gameplay
removal and neighbor planner with `RemovalCause::Burn` for **every** conditional
edit in that owner system. It requires authoritative chunk reads and only
allows removal from a non-air block to air. Ordinary `WorldEdit` systems retain
their existing semantics and fingerprint; the burn declaration changes the
system's persisted identity. Default harvest does not award burnt blocks, while
targeted burn handlers and support-loss effects still participate in the same
owner WAL transaction. This capability does not itself ignite or schedule fire;
native fire remains the sole production propagator pending its migration.

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

For gameplay payloads rather than scheduling hints, an opt-in chunk system with
declared world reads uses `Behavior::accepts_intents`, receives at most eight
`IntentDelivery` values in `plan_with_intents`, and sends at most eight bounded
512-byte payloads through `IntentOutbox`. The host assigns source revision and
ordinal identities; production shares the source owner's WAL transaction.
Delivery starts on a later logical tick, and a successful destination plan
acknowledges its inbox atomically with owner/world/entity changes and forwarded
messages. Rejection preserves the inbox. Mailboxes hold at most 64 messages per
destination and 2,048 globally; a wave emits at most 128. Payload delivery is
not an advisory wake and cannot be shed to relieve queue pressure.

By default a destination must already have an owner state. Opt-in
`Behavior::intent_bootstrap()` supplies one canonical initial byte template for
absent chunk owners. The host validates, fingerprints and captures it at
registration; revision-zero destination state and its mailbox are created in
the producer's WAL record, even if terrain is unloaded. Existing owner state
always wins. No creation is visible before receipt, and capacity/WAL pressure
defers the entire producer. The template cannot depend on destination position;
the destination's first plan can read its owner and authoritative terrain.
There is no owner reclamation yet, so the 16,384-cell store-wide limit remains
a hard cap. Cross-system payloads and entity/profile bootstrap are not bound.

This still does **not** complete world-system parity: native fire propagation
and delivery have not migrated onto this contract, and Luau owner callbacks do
not expose these payloads or all generated entity/drop operations. Built-in
support removal uses the shared neighbor decision path for existing edit
producers rather than an owner-system callback.
An entity/profile owner key is an identity, not a grant to mutate that entity or
player. Callbacks are trusted deterministic Rust functions, not sandboxed plugins.
