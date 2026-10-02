# Vegetation rules

The server gradually updates builtin leaves, grass and dirt near players.
Confirmed player edits also prompt local checks. Scan work, timers and queued
attempts have fixed limits, so clearing a forest cannot create unbounded work.

- Leaves survive when a face-connected path through leaves reaches any log
  orientation within six steps. Otherwise they decay after a 5–15 second delay,
  using the normal leaf harvest policy for occasional drops.
- A solid block directly above grass converts it to dirt after 10–30 seconds.
  Flowers, fern and tall grass do not count as cover.
- Uncovered dirt with a clear vertical sky column grows grass during daylight
  after 20–60 seconds. It does not require neighboring grass. Nighttime, direct
  cover and opaque roofs prevent growth.

Delays start when a check finds an eligible cell; rotating scans add discovery
time. Eligibility is rechecked when the attempt reaches durable admission, so
restored logs or changed cover can cancel a pending change. Missing chunks
defer decisions and load asynchronously. Checks do not use procedural fallback
as world authority.

Soil transformations do not harvest the replaced block. Registered gameplay
handlers receive `RemovalCause::Transformation` (`"Transformation"` in Luau),
and placement/neighbor/lifecycle decisions retain the normal transaction path.
Edits, drops, revision updates and client deltas publish only after the journal
receipt. Timer and scan hints are transient; restart rebuilds them while keeping
already committed world changes.

Sky scans include saved and pending roofs above the builtin generation ceiling.
An individual scan is capped at 512 blocks; an unresolved longer column does
not grow grass. Custom generation contributors currently have no upper-height
contract, so automatic dirt regrowth is deferred in those worlds rather than
assuming a finite empty column proves sky visibility.

Implementation: `src/server/ecology/`, durable planning in
`src/server/durable/actions/ecology.rs`, and ceiling metadata in
`src/world/sky.rs`. Delay ranges are in `ecology/rules.rs`; scan and queue budgets
are in `ecology/schedule.rs`.
