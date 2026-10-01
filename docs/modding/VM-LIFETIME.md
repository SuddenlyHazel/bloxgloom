# Luau VM lifetime and module state

The engine reuses bounded, execution-lane-owned Luau runtimes. Interpreter reuse
and retained Lua state have different contracts: an authoritative attempt starts
with fresh module exports and closures, while supported advisory and client
callbacks retain them until their realm ends or resets.

## Choose the right state

| Callback family | Module state |
| --- | --- |
| Server startup | Registration initialization only; registration authority ends after startup |
| Generation, gameplay decisions, admission, owner systems, machines and creatures | Fresh module evaluation and mutable state for every attempt, with reusable VM infrastructure and compiled code |
| Readonly committed observers | Retained ephemeral state for each entry on the world-installation observer lane |
| Client startup | Connection initialization; registration authority ends after startup |
| Client UI, replica and visual presentation | Retained state for each entry on its owning connection presentation worker |
| Client player-service callbacks | Retained state for each handler entry on the connection player-service worker, separate from presentation |

Module locals, tables, exported closures and manually resumed coroutines can
retain state in the supported retained realms. Writes to globals stay within that realm or module environment; base globals
and library tables remain readonly. A dependency's exports belong to its consuming realm; they are not a
process-wide or package-wide singleton. Different entry modules and workers/callback families have
independent copies even when they import the same source.

Use host-owned profile, entity, scheduled-owner or package session state for
authoritative decisions. Those stores supply revisions and commit ordering. Lua
tables and upvalues are not transaction snapshots, saved progress or commit
receipts. Observer delivery is advisory and may coalesce or be interrupted.

## Initialization and random streams

Retained modules initialize lazily on first use and keep their exports for that
realm. Their imports retain the existing namespace, direct dependency visibility,
cycle checks, depth limits and source attribution. Authoritative imports instead
cache exports only inside one attempt. Immutable compiled code is keyed by exact
source and module identity; discarding that cache does not discard retained state.

Authoritative execution seeds native `math.random` before evaluating the entry
and imports, using the same captured host inputs as before. Repeated inputs repeat
the initializer and callback draw stream regardless of earlier calls or worker
assignment. A retained initializer uses a stable identity seed; each callback
reseeds independently from its captured callback inputs. Initializer draws do not
advance the callback stream. An import first demanded inside a later callback
initializes using that callback's current stream, and its exports then remain
cached. Only imports evaluated during entry initialization share the stable
initialization stream. Moving a lazy import changes random-draw ordering; prefer
importing initialization dependencies at module scope when that distinction
matters. Authors can still call `math.randomseed` explicitly.
See [runtime tools](RUNTIME-TOOLS.md) for seed inputs and limitations.

## Contexts and coroutines

A host context exists for one invocation. Saving it or one of its methods does
not extend its authority: the saved method fails after that invocation ends.
Never cache a host context in a controller. Pass the current context into each
controller update instead. Copied readonly data can remain as historical data;
it does not become a fresh world view or permission to modify the world.

Retained logging helpers use the current callback's diagnostic correlation and
their own executing module identity. Ordinary callbacks still batch host outputs;
a failed callback cannot publish a partial output batch.

```luau
local updates = coroutine.create(function()
    local count = 0
    while true do
        count += 1
        coroutine.yield(count)
    end
end)

return function(host, event)
    local ok, count = coroutine.resume(updates)
    assert(ok, tostring(count))
    -- Use this invocation's host here, never inside a suspended old context.
    log.debug("controller updated", {callbacks = count})
end
```

A retained coroutine runs only when the mod explicitly resumes it. Its work is
charged to the current callback's instruction and time budgets. There is no new
`wait`, task scheduler, signal or promise API. Authoritative coroutines cannot
survive an attempt or yield across a commit boundary; delayed gameplay continues
to use the durable host scheduler.

## Failures and teardown

Each invocation receives fresh instruction, wall-time, diagnostic and staged
output limits. Catching a limit or invalid host operation does not clear its
failure latch. Resident Lua memory includes retained tables, imports, closures
and threads; retaining a large cache consumes the realm's memory allowance.
Collection and execution stay on workers rather than the window draw path.

The default realm reserves **8 MiB** of Lua heap. The process admits at most
**64 runtimes** and **256 MiB** of aggregate heap reservations; admission accounts for each realm's
full allowance rather than only its current use. Client worker families admit
at most **8 entry realms**, and an observer lane at most **32**. Admission failure
is reported rather than evicting another module's retained state. Each engine's
immutable source/bytecode cache has independent limits of **128 entries and
4 MiB**; cache eviction only causes recompilation. These reservations bound Lua
heaps, not total Rust/network/graphics process memory.

An error retires retained state rather than pretending partial Lua mutations
were rolled back. A subsequent admitted callback initializes fresh state; the shared retained
engine also increments its diagnostic generation.
Reset/admission diagnostics identify the affected realm and reason. Stateful
realms are not silently evicted as an ordinary compiled-code cache operation.

Reconnect, switching server, world shutdown and worker retirement release the
corresponding realms and references. Closing an authored UI panel alone does not
retire its connection worker. Disconnect hooks are bounded and advisory: cleanup
does not depend on a successful script finalizer. Package installations remain
frozen during play; this work adds neither hot reload nor save conversion.

## Runnable example and measurement

The [welcome package](../../fixtures/player-lifecycle/README.md) retains a local
label cache and manually resumed controller coroutine on its player-service
worker. Its server audit retains an ephemeral observation counter. Enable script
DEBUG logs, deliver multiple public snapshots and reconnect: callbacks increase
inside the connection, then start from one in the replacement connection.

Run the lifetime microbenchmark alone:

```sh
cargo test vm_lifetime_latency_baseline -- --ignored --nocapture --test-threads=1
```

It reports p50/p95/p99 latency, initializer evaluations and post-collection Lua
memory for 2,000 calls. A development-build baseline before the lifetime and
protected-memory-error changes on this machine measured:

| Execution strategy | p50 | p95 | p99 | Initializer evaluations |
| --- | ---: | ---: | ---: | ---: |
| Fresh VM, compile, initialize and call | 300.71 µs | 351.92 µs | 438.75 µs | 2,000 |
| Reused VM, compile, initialize and call | 81.04 µs | 88.62 µs | 115.67 µs | 2,000 |
| Retained initialized callback | 4.29 µs | 4.42 µs | 4.50 µs | 1 |

Post-collection memory was 321,696 bytes for fresh/re-evaluated modules and
356,464 bytes with the callback retained. This isolates setup, compilation and
module-lifetime costs; it excludes real adapters, queue contention, transactions
and per-call budget rebinding. It is not a gameplay latency or allocation-count
benchmark. Production-path measurements and correctness tests are the acceptance
evidence for the implemented runtime, rather than these timing ratios alone.

The production-runner comparison is also opt-in:

```sh
cargo test vm_lifetime_production_runner_latency -- --ignored --nocapture --test-threads=1
```

After implementation, the same development build measured:

| Production runner path | Warm p50 | Warm p95 | Warm p99 |
| --- | ---: | ---: | ---: |
| Isolated authoritative attempt | 57.08 µs | 60.54 µs | 78.75 µs |
| Retained client callback | 19.25 µs | 22.04 µs | 28.25 µs |
| Two alternating isolated attempts, including heavier unrelated work | 186.67 µs | 204.04 µs | 244.17 µs |

These include instruction/deadline setup, native RNG and diagnostic rebinding,
immutable compilation caching and worker-side cleanup/collection. The first cold
isolated initialization took 2.32 ms and retained initialization 0.83 ms in that
run; those are individual observations, separate from the 1,999 warm samples.
The paired workload has 2,000 samples and 4,000 module evaluations. Both single
paths sampled a maximum of 536,424 bytes immediately after the callback, before
cleanup; this is a sampled heap maximum, not an allocator high-water counter.

Fresh VM setup under the new protected-memory-error instrumentation measured
483.46 µs p50 / 579.33 µs p95 / 734.42 µs p99, and 452,576 bytes after collection.
Reuse amortizes that additional setup instead of paying it on each invocation.
The alternating workload tests one execution lane with unrelated script work;
it does not simulate network queues, movement latency, WAL transactions or
cross-worker contention. No performance threshold is asserted by either test.

The real listener measurement includes movement, a retry-isolated Luau action,
world editing and durable action receipts:

```sh
cargo test vm_lifetime_mixed_listener_latency -- --ignored --nocapture --test-threads=1
```

It starts the production nonblocking loopback listener with a unique temporary
save, sends 120 movement-only requests, then 120 paired movement/action requests
on the same admitted connection. The action initializes a lookup table, performs
1,000 calculations and toggles a block through the ordinary server transaction.
Each invocation asserts fresh authoritative module locals. All actions must be
accepted; reopening the save verifies the final block and conserved held item.
Client and server use TCP_NODELAY, as the production client does.

| End-to-end path | p50 | p95 | p99 |
| --- | ---: | ---: | ---: |
| Movement without action load, 120 samples | 19.596 ms | 27.970 ms | 28.891 ms |
| Movement paired with a Luau action, 120 samples | 20.172 ms | 30.317 ms | 32.633 ms |
| Warm durable Lua action receipt, 119 samples | 20.496 ms | 32.173 ms | 36.184 ms |

The first action receipt took 41.984 ms, including initial script execution,
network transport, coordinator scheduling and WAL durability. These are host
round trips, not isolated VM timings. The single-client paired workload increased
movement p99 by 3.742 ms in this run. This verifies live movement/action behavior
under bounded interleaved script work; it is not a saturation, multi-client or
before/after release performance study. Run the ignored test alone to avoid
concurrent builds/tests distorting its tails. Timing values remain evidence,
with behavior assertions rather than machine-dependent timing thresholds.
