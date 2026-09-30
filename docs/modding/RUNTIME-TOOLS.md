# Luau runtime tools

Server startup, generation, gameplay, owner systems, machines and creatures,
plus client startup and presentation, share this runtime surface. Each invocation
still gets a fresh bounded VM. This work does not change VM ownership, reuse,
callback scheduling or persisted state.

## Libraries

| Surface | Available today |
| --- | --- |
| Base | Ordinary Luau operations, assertions, errors, protected calls, iteration and conversions |
| `table`, `string` | Bundled Luau table and byte-string operations |
| `math` | Complete bundled library, including ordinary calculations, constants, `random` and `randomseed` |
| `utf8`, `bit32` | Unicode code-point operations and 32-bit bit operations |
| `buffer`, `vector`, `integer` | Binary buffers, native vector calculations and native 64-bit integer arithmetic |
| `coroutine` | Create, resume, yield and wrap within the current invocation |
| `debug` | Luau's `info` and `traceback` introspection |
| `os` | `difftime` only |
| `log`, `print` | Structured attempt diagnostics through the engine's `tracing` writer |

This is the library surface of the bundled Luau version, not every library from
desktop Lua or Roblox. No filesystem, socket, HTTP, process or native module
loader is supplied. `require` is absent; declared package imports use the
existing host import contract. `gcinfo`, `getfenv` and `setfenv` remain absent.
Globals and library tables are readonly after setup.

`os.clock`, `os.time` and `os.date` are absent because they would introduce
uncaptured process timing, wall time or local timezone into retryable decisions.
Use captured host ticks and the public world-clock service for game time. Profiling
remains host tooling. `debug` does not expose Rust objects or unrestricted Lua
debug hooks: Luau itself provides only `info` and `traceback` here.

Buffers, vectors and integers help calculations and state encoding; they do not
replace existing host argument schemas. For example, a callback returning state
bytes must still return a binary string, and entity identities remain opaque
host handles. Ordinary floating-point math is not a guarantee of bit-identical
results across every platform.

```luau
local direction = vector.normalize(vector.create(3, 0, 4))
local bytes = buffer.create(4)
buffer.writeu32(bytes, 0, 123)
assert(buffer.readu32(bytes, 0) == 123)
local exact = integer.fromstring("9007199254740993")
assert(tostring(integer.add(exact, integer.create(1))) == "9007199254740994")
```

Coroutines are local control flow. A coroutine cannot survive the invocation or
become a background task; use existing durable host scheduling for future work.
VM memory, interrupt and time ceilings apply to coroutine execution too. Catching
an execution-limit error with `pcall` or `coroutine.resume` does not make the
invocation eligible to publish effects.

## Deterministic randomness

The host seeds Luau's native PRNG **before any entry or imported module runs**.
Module initializers and callbacks share that invocation's stream. The native
`math.random` and `math.randomseed` implementations and argument semantics remain
available. Authors can deliberately reseed:

```luau
math.randomseed(71)
local unit = math.random()       -- [0, 1)
local choice = math.random(1, 8) -- inclusive integer bounds
```

The default seed combines the entry package/version/module identity with stable
host inputs:

| Invocation | Seed inputs before entry identity is mixed in |
| --- | --- |
| Server/client startup | Fixed startup seed |
| Generation | World seed and chunk coordinates |
| Gameplay with a durable action ID | World/handler random-stream seed and the full action ID |
| Other gameplay events | World/handler random-stream seed and captured event contents |
| Owner system | Owner identity and owner revision |
| Machine | Entity ID, scheduled due tick, private bytes, fuel and progress |
| Creature tick | Entity ID, scheduled due tick and private bytes |
| Creature interaction | Entity ID, revision, private bytes and interaction request |
| Client presentation | Request sequence |
| General integer runner | Supplied seed and tick |

A retry using the same identity and seed inputs starts with the same stream,
including module-initializer draws. A durable action retains its stream even if
it is retried on a later host tick. Changing code or the order/number of draws can
change decisions. Logging suppression and filtering do not consume PRNG draws.
The existing captured host random helpers remain available.

Seed mixing is stable and length-delimited; the result is folded into Luau's
signed 32-bit seed argument. Different invocations can collide. This is not
cryptographic randomness or a promise that future Luau/runtime versions preserve
the same sequence. Store durable decisions in authoritative state when they must
survive implementation changes. No script-visible OS entropy provider is added.

## Diagnostics

```luau
log.debug("growth selected", {recipe = "jade", count = 2, ready = true})
log.info("owner scheduled")
log.warn("unexpected state", {state = "dormant"})
log.error("missing recipe", {recipe = "jade"})
log.trace("branch visited", {branch = 3})
print("selected", "jade", 2)
```

`log.trace/debug/info/warn/error(message, fields?)` accept a UTF-8 message and an
optional plain table with string keys and string, finite-number or boolean values.
Fields are encoded as a JSON object in the tracing event's `fields` field, retaining
those value types. Nested tables, metatables, engine objects and nonfinite numbers
are rejected with an ordinary Lua error. The editor declaration accepts inline
field records; runtime validation enforces these restrictions.

`print(...)` emits INFO diagnostics through the same bridge. It joins up to 16
primitive values with tabs, replaces invalid UTF-8 in binary strings, and
truncates long output. Other values become type labels such as `<table>`;
printing never invokes their `__tostring` metamethods. For exact host-handle labels,
call `tostring(handle)` explicitly and pass the resulting string.

Each invocation buffers at most **64 records and 16 KiB** of message, field and
source text. A structured message is at most 2,048 bytes; fields have at most 16
entries, 64-byte keys and 1,024-byte string values. Oversized structured input
raises a Lua error. `print` instead truncates to 2,048 bytes with `[truncated]`.
Further records that exceed the invocation budget are discarded; one WARN summary
reports the suppression count. Once the buffer is full, calls discard content
without further field serialization. Normal callback resource ceilings still
apply to the author's own formatting and loops.

Records use tracing target `bloxgloom::script`, with entry package/version,
executing module, entry module, server/client side, callback kind, invocation
correlation and execution outcome. An imported helper retains its own module
source along with the caller's entry context. Diagnostics flush at the end of
the attempt, including a failed attempt.

Outcomes include `evaluated`, `script_error`, `instruction_limit`, `time_limit`
and `aborted`; client startup uses `execution_limit` for its latched limit.
`evaluated` means the runner produced a candidate result. Subsequent transaction
validation, admission, WAL durability or client installation may still reject it.
Diagnostics are **attempt records**, not commit receipts. Retries can repeat
records with the same correlation, and logs do not provide exactly-once delivery.
Use existing committed action receipts to establish gameplay success.

Caught invalid host operations still reject gameplay plans. The adapter preserves
the original host error, including unavailable inputs that must remain retryable.
Source errors retain module attribution and Lua traceback information; use
`debug.info` or `debug.traceback` when adding your own diagnostic context. Stale
dependencies and admission failures after evaluation remain host outcomes rather
than being misreported as script execution failures.

`RUST_LOG` controls filtering on both server and client. The default includes INFO
script records; enable more detail with:

```sh
RUST_LOG=warn,bloxgloom=info,bloxgloom::script=debug cargo run --release -- local-packages fixtures/combined-mod/packages /path/to/new-save
```

Filtering happens when buffered records are emitted; invocation budgets count
filtered records too. The existing background writer uses a bounded, lossy queue
(4,096 lines). Slow output can drop diagnostics instead of blocking gameplay;
logging does not mutate world state, inventory, owner bytes or presentation state.

## Examples and compatibility

The [Jade garden](../../fixtures/combined-mod/README.md) demonstrates server-side
branch diagnostics and client startup diagnostics, ordinary buffers and seeded
randomness. [IDE setup](IDE.md) describes the editor definitions for `log`.

Client host contract **3**, on unchanged wire version **13**, advertises this
runtime. Older client contracts are rejected before bundle execution. Use matching
client/server binaries. Source edits still affect frozen content fingerprints;
use a fresh garden save when testing the updated fixture. This change introduces
no world-format conversion or VM reuse.
