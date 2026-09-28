# Durable neighborhood relay

From the repository root, use a new save and separate terminals:

```sh
cargo run --release -- server-packages fixtures/neighborhood/packages 127.0.0.1:4000 /path/to/new-relay-save
cargo run --release -- client 127.0.0.1:4000
```

The owner at chunk `(0,5,0)` reads `(18,80,2)` in its east neighbor and
durably sends that block-state key to the same system at owner `(1,5,0)`.
Only after the receipt, on a later tick, that owner reads `(34,80,2)` in
**its** east neighbor. If both samples are air, it conditionally places
glowstone there. Non-air input is deliberately left unchanged. Owner bytes,
inbox acknowledgement and the edit commit together; restart does not repeat
the operation. No client scripts or procedural client terrain are inputs.

`read_radius_chunks=1` requires `read_world=true` and the existing
`bloxgloom:owner_systems/v1` capability. Omit the radius (or use `0`) for
owner-only reads/edits. The host captures at most 27 authoritative chunks per
job, requests unavailable chunks asynchronously, and retries without consuming
the inbox. Each invocation permits 64 explicit reads and 16 conditional edits
(each also validates its captured preimage), with the existing bounded intent
outbox. Caught host errors reject the entire plan.

Run `cargo test luau_neighborhood` for deterministic unavailable-input,
read-fence, receipt-loss/recovery, denial, batch-overlap and real nonblocking
TCP tests of these package files. This is a server behavior fixture, not a
release-window visual verification.
