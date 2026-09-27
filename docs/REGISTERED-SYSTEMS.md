# Public persistent owner systems

`Registrar::owner_system` exposes the existing owner-worker and durable-wave
runtime through `bloxgloom_host_api::system`. It is entity-independent background
computation, not a terrain-editing API.

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
It does **not** complete the planned world-system parity row: the existing owner
runtime has no captured terrain/neighbour view or atomic terrain/entity-effect
interface. Those services, dynamic owner creation/removal, wake subscriptions,
built-in support behavior, and deferred fire migration remain explicit followups.
An entity/profile owner key is an identity, not a grant to mutate that entity or
player. Callbacks are trusted deterministic Rust functions, not sandboxed plugins.
