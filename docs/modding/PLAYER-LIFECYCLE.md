# Player lifecycle implementation

This is the active reference for the player-services goal. The
[gap assessment](../../LUAU-SCRIPTING-GAPS.md#2-player-and-lifecycle-hooks)
records the completed agreed scope and deferred mechanics. VM lifetime, chat/combat, region hooks and save
converters are outside this work.

## Landed: exact identity and action queries

Gameplay contexts expose `player_profile`, an exact `BloxProfileId` for the
acting player when present. Action callbacks also receive a captured directory:

```luau
return function(c: BloxGameplayContext, event: BloxGameplayEvent)
    local me = if c.player_profile then c.player_by_profile(c.player_profile) else nil
    for _, player in c.players() do
        log.debug("online", {name = player.name, session = tostring(player.session)})
    end
    if me then
        assert(c.player_by_session(me.session) ~= nil)
    end
end
```

Views contain `profile`, `session`, optional public `entity`, Hello `name`,
readonly authoritative `position` and `appearance` (skin/shirt/pants indices and
reserved flags), `online=true` and
`identity_trust="claimed_profile"`. The list is readonly and sorted by exact
profile identity. An unknown/offline profile or departed session returns nil.
These are captured values, not references to mutable client objects.

Session handles combine the stable profile with the server-issued durable
action epoch. Reconnecting grants a new epoch; session handles are distinct
from profile and entity handles. Numbers, strings and lookalike tables are not
accepted as handles. A caught invalid identity operation still rejects the
gameplay plan. Player queries share the normal host operation budget.

The current Hello protocol claims a profile ID. It does not cryptographically
prove ownership of an account. Operator-selected local admin identity and
server-issued action epochs do not change that trust level. No authentication
service is introduced by exposing these handles.

## Landed: lifecycle state and scheduling

Declare `requires bloxgloom:players/v1` and register an own-package service:

```luau
h.register_player_lifecycle("demo:progress", 1, 64, "", "demo:player")
```

The arguments are key, revision (1..65535), private byte bound (1..4096), initial
binary state and callback module. Eight services per package and 128 per
installation are allowed. Service keys cannot collide with owner-system keys.
Changing the frozen package implementation changes the save compatibility identity.
The client bundle carries an inert service identity with runtime contract 7;
private initial state and executable registrations are not projected into metadata.

The module returns `function(context, event)` and may return nil or a decision:

```luau
return function(c: BloxGameplayContext, e: BloxPlayerLifecycleEvent): BloxPlayerLifecycleDecision?
    if e.kind == "PlayerJoined" and e.state == "" then
        assert(c.give("player", {item = "bloxgloom:stick", count = 3}))
        return {state = "kit", public_state = "level:1", session_state = "connected"}
    end
    return nil
end
```

`event` is readonly. It contains exact `profile`, optional captured `player`,
`transition`, `identity_trust`, private `state`, selected `public_state` and
`session_state` binary strings. Initial state is used when that package/profile
has no committed cell. Progress is scoped by the service key and stable profile,
without conflating avatar identity or server session epochs.

Events are `PlayerJoining`, `PlayerJoined`, `PlayerSpawned`, `PlayerLeaving`,
`PlayerLeft`, `ProfileTick` and `SessionTick`. Joining runs after content readiness
and durable epoch allocation; a cancelled or failed pending join never queues
Joined/Spawned. Joining may return `deny` (1..255 UTF-8 bytes) or a `spawn` triple.
The host validates finite coordinates, world bounds and authoritative collision,
requests missing terrain and retries before installing the avatar. Joining cannot
publish state, inventory rewards or timers. Use Joined for these effects.
Admission services run in stable service-key order and all must accept; the first
denial/error ends admission. Denial reasons are logged server-side; the current
pre-Welcome protocol closes the connection without a dedicated rejection frame.

Joined/Spawned queue after successful admission. Leaving/Left capture the final
player view and session bytes without blocking disconnection or permitting a veto.
Callbacks run in bounded fresh VMs on the server coordinator, independent of the
socket reactor. Four queued callbacks are attempted per tick. Profile/inventory
changes share one revision-fenced WAL record and become visible only after sync.
Script errors, caught invalid host calls, oversized states and unsupported
operations publish no partial reward or progress. Critical first-join behavior
belongs to this durable state/reward callback, not a transient notification.

Decisions may replace `state`, `public_state` (at most 1024 bytes), and
`session_state` (at most 4096 bytes). Session bytes and timers apply after the
receipt only if the exact admitted session is still live; disconnect removes
live session state immediately. Final leave callbacks receive captured bytes.
Lifecycle callbacks support ordinary inventory operations for their profile and
other authorized profiles, plus gameplay queries; world/entity writes are
rejected as a whole.

`profile_delay` and `session_delay` accept 1..100000 logical ticks or false to
suspend. Omitting them retains an existing deadline. Timer callbacks consume their
one-shot deadline unless they return another delay. Profile deadlines are saved
with the state, resume on the recovered logical tick timeline and run while the
profile is offline. Downtime does not consume wall-clock time. Session deadlines
cancel on disconnect and cannot target a replacement connection. A terminal timer
callback failure is logged and suppressed in this process until its registration/
deadline changes; it never silently commits a replacement value.

## Landed: typed player commands and client roster

An empty-target action's command schema may contain `{kind = "player"}`.
Text input accepts a unique exact online name or
`session:<32 lowercase profile hex digits>:<16 lowercase epoch hex digits>`.
Tab completes a unique name prefix to that exact session token. Duplicate names,
unknown/offline players and completed tokens from an older connection are rejected.
The wire argument uses 16 profile bytes plus 8 session bytes; neither passes through
a floating-point number. The existing eight-field/130-byte command bound applies.

`event.command_arguments` is a readonly ordered list. Player fields are
`BloxSessionId` handles; key fields are strings and counts are numbers. Raw
`event.arguments` remains available. The host checks command-caller permission,
actor inventory revision and every player target's live profile/session pair
before invoking the handler, including retries. A handle grants no admin authority.

Wire version 17 carries a bounded, sorted roster of at most 256 admitted players,
with profile/session/name and a roster revision. Queue pressure retains the pending
revision so the next publish retries. Clients accept newer snapshots and clear the
roster on disconnect/server switch; late messages cannot revive it. The roster is
command discovery metadata, without private profile state or global positions.

## Landed: committed observers

With `requires bloxgloom:actions/v1`, register
`h.register_committed_observer("demo:audit", 1, "demo:observe")`. Keys and modules
belong to the declaring package; revisions are 1..65535. Eight observers per
package and 128 per installation are allowed. Invalid declarations refuse startup
even when caught by `pcall`. The bundle exports inert compatibility fingerprints,
without server source or executable client observers.

The callback returns `function(event: BloxCommittedEvent)`. Readonly `blocks`
contain cell/state; `entities` contain Spawned/Updated/Removed, exact entity
handles and public fields; optional `inventory` contains an exact profile handle
and resulting revision, without slots. There is no mutation context. This is the
native commit surface; it does not invent events for profile-only state updates,
movement, appearance or world-clock changes.

Delivery occurs after WAL sync/publication on a separate observer worker, in
frozen registration order. The queue holds 32 events; events exceeding 256 KiB
or arriving under pressure can drop. Shutdown need not drain the queue, and
restart never replays advisory observations. Each Luau callback uses a fresh
bounded VM. An error or exhausted budget is logged and does not block a commit
or poison later callbacks. Use lifecycle state plus atomic inventory decisions
and durable profile deadlines for critical rewards/progression. Observer logs
and presentation notifications cannot promise exactly-once delivery.

## Remaining implementation

Additional authorized player operations remain in progress.
The client/server/save contracts will be versioned when their formats change.

## Landed: local public state and client lifecycle

Wire 17 delivers a full local-player snapshot containing each frozen player-service
key, committed owner revision and explicit public bytes (at most 1024 per service).
Missing profile cells have revision zero and empty public state. The packet includes
the exact profile/session pair and an increasing snapshot sequence. It sends after
admission and retries under queue pressure. Only that profile's committed projections
are sent; private profile/session bytes are never implicitly included. Publication
invalidates only the affected profile's delivery cache. Clients validate identities
and negotiated service keys, ignore older snapshots, and clear all values on retirement.

During client startup, register one own-package callback with
`h.set_player_handler("demo:player")`; this requires `players/v1`. Separate packages
compose rather than replacing one another's handler. The module returns
`function(host: BloxClientPlayerHost, event: BloxClientPlayerEvent)`. It can update
owned UI text/state and declared visual parameters. Registration methods are
startup-only; no callback can request gameplay actions or modify server state.
Imports use the same client/shared dependency rules as startup.

`SessionReady` runs after the first valid local-player snapshot is installed.
`PlayerStateChanged` receives later snapshots. `SessionDisconnected` receives the
last captured snapshot and bounded reason; its presentation replies are discarded.
Inputs contain exact `profile`/`session`, honest `identity_trust`, and a readonly
`states` map of this package's service keys to `{revision, public}`. Updates coalesce;
callbacks do not promise one invocation per intermediate commit. Public-state
delivery contains no event for private inventory contents.

All callbacks execute on a session-owned worker in fresh bounded VMs (8 MiB,
10000 interrupts, 50 ms per invocation), including imports. An invalid host call,
even when caught, refuses the entire local output; other packages and later events
continue. Disconnect signals bypass the update/reply queues, so retirement never
waits on script execution or a full queue. An admitted callback may finish before
the final disconnect hook, but its old replies cannot update a replacement session.
Shutdown can interrupt final advisory hooks; server cleanup never relies on them.

The runnable welcome package updates an authored profile panel from these bytes.
Open it with F6; reconnect should retain `level:1` while replacing the session handle.

## Landed: targeted notices and session kicks

Server action callbacks and admitted lifecycle callbacks can call
`c.message_player(session, text)` and `c.kick_player(session, reason)`. The
executing server package must declare `requires bloxgloom:players/v1`. An exact
session handle identifies the target; it does not grant authority. Command
caller permissions remain independently enforced by the registered command
schema. Server rules may act on other players when this capability is granted.

```luau
local target = c.player_by_session(session)
if target then
    c.message_player(target.session, "Checkpoint reached")
end
```

Text is nonempty UTF-8, at most 255 bytes, without control characters. A gameplay
plan may contain at most 64 session operations, sharing the existing host-call
budget. Invalid identities, unavailable targets, denied authority or invalid text
reject the whole plan even when caught with `pcall`; preceding inventory changes
and notices are discarded. `PlayerJoining` cannot stage session operations.
Other gameplay contexts currently reject these operations rather than silently
dropping them.

Effects publish only after the action's WAL receipt. Duplicate action requests
return the existing receipt without sending another notice or repeating a kick.
These are transient session effects: crash recovery does not replay them. The
host checks the exact profile/epoch again at publication; a departed connection
is skipped, and its replacement is never targeted. Acceptance confirms the
authorized intent, not guaranteed client delivery.

Notices use the client's existing transient HUD status. A kick immediately
removes the authoritative client, avatar, movement queue and session timers. The
reactor drains its queued removal reason before closing when queue capacity
allows; delivery of that reason is best effort under pressure. The client retires
its entire session and shows the supplied reason. A self-kick can close before
the action result reaches its caller; committed inventory/profile effects still
recover normally. Kicks do not create durable admission bans. Use durable
profile policy for bans through the general profile-state service below.

Wire version 17 adds the exact-session notice/removal frame. Profile inventory
transactions reuse the existing inventory frames and WAL domain.

## Landed: runtime appearance

`c.set_player_appearance(session, skin, shirt, pants)` accepts three exact integer
palette indices in 0..255. Each must exist in the frozen startup appearance
catalog. Arbitrary colors, model replacements and reserved appearance flags
cannot be supplied. The executing package needs `players/v1`; caller permissions
are checked independently. It is available in action and admitted lifecycle
callbacks under the same 64-operation plan bound as notices and kicks.

Player views expose a readonly `appearance` table with `skin`, `shirt`, `pants`
and `flags`. Directory/profile/session queries see proposed appearance changes
within their own plan; another callback sees them only after publication. Script
errors, invalid palettes, forged/stale sessions or caught invalid host calls
discard the plan's cosmetic and inventory proposals together.

After the action WAL receipt, publication rechecks the exact session and calls
the native cosmetic path. The atomic per-profile appearance file commits before
the public avatar revision changes. Both target and nearby peers receive normal
entity replication, without terrain remeshing. Repeating an identical cosmetic
selection is a no-op; replaying an action receipt does not repeat any operation.
Reconnect/restart reads the saved appearance for that profile. A replacement
session is never affected by an old intent.

Appearance keeps its existing separate file durability boundary: inventory and
profile progression are recorded in the WAL, while the cosmetic file is saved
after its receipt. A crash between those boundaries can preserve progress and
the previous cosmetic. These cosmetic intents are not replayed during recovery.
Storage failure stops server mutation before publishing the new avatar; the
atomic file remains the recovery authority. Do not use a costume as a critical
reward flag; store that flag in durable profile state.

The lifecycle fixture includes `/welcome:manage <player> 1` as an Admin command
and uses builtin palette indices 1, 2 and 3. Player arguments support names and
exact session tokens, including Tab completion. Set the server's local admin
profile to exercise it; declaring the package capability alone does not grant
its command to non-admin callers. This operation uses existing wire version 17
and client host contract 7.

## Landed: runtime teleport

`c.teleport_player(session, x, y, z)` stages an absolute authoritative feet
position. It requires the executing package's `players/v1` capability and an
exact online session, in an action or admitted lifecycle callback. Coordinates
must be finite and strictly inside ±1,000,000, above bedrock. Player directory
queries see the proposed position in the same plan. The captured actor position
used by other gameplay authorization remains the invocation's original position.

The native planner checks the frozen body against authoritative destination
terrain after all proposed block edits. It captures collision reads, including
air, as transaction dependencies: changes to those chunks conflict until the
receipt is applied. Unknown chunks are requested through the normal background
terrain workers and the action retries; they never become guessed air. An
obstruction, invalid coordinate or forged target rejects all proposed inventory,
terrain and player effects. There is no automatic search for a nearby safe spot.

After receipt, the exact session is checked again. The native position file
saves before authoritative movement, avatar motion and streaming center change.
New interest and chunk snapshots use that center. The position file keeps its
existing separate durability boundary from WAL inventory/progression, as cosmetic
files do: a crash between boundaries can retain progress with the old position.
Session teleport intents are not replayed during recovery. Reconnect uses the
saved position, revalidating collision under the existing spawn policy.

Wire version 17 and client host contract 7 add a movement reset and acknowledgment.
The server clears queued movement and movement credit, then gates old input until
the client acknowledges the latest reset with its next movement sequence. The
client clears unacknowledged prediction deltas and installs the new position
before acknowledging. Old or duplicate reset acknowledgments cannot reopen a
later reset, and acknowledged old sequence numbers cannot move the player. Each
new reset has a session-scoped monotonic identity. A replacement session is
never affected. Clients that refuse to acknowledge remain unable to move.

A teleport request replay returns its existing receipt without repeating the
reset. Multiple teleports are applied in plan order; the last reset must be
acknowledged. Storage failures stop mutation before movement publication.
Per-player physics changes remain
outside this goal.

The lifecycle fixture includes `/welcome:recall <player>`, an Admin command
that teleports the selected session to the caller's captured feet position and
sends a status notice. It uses the same native validation and reset handshake.

## Landed: general package-owned profile state

Ordinary action callbacks and player lifecycle callbacks can query any known
profile's state in a service registered by their own package, with `players/v1`:

```luau
local saved = c.profile_state("welcome:policy", target.profile)
c.set_profile_state("welcome:policy", target.profile, "role:builder", "builder")
```

`profile_state` returns a readonly `{revision, initialized, state, public_state,
next_tick}` view. State fields are binary strings, revision is an exact
`BloxRevision`, and the optional deadline is an exact `BloxTick`. Missing cells
return the registered initial private value and empty public bytes with
`initialized = false`. A newly persisted owner cell can have revision zero too;
use `initialized` to distinguish absence. Proposed writes appear in subsequent
reads while retaining the captured revision, initialization flag and deadline.

A profile does not need a live connection. Obtain online identities from the
captured directory, or explicitly decode a previously saved token with
`c.profile_id("profile:<32 lowercase hexadecimal digits>")`. This constructor
rejects zero, noncanonical tokens, numeric identities and lookalike tables. Other
methods still require a profile handle; they do not silently coerce strings.
Identity handles grant no authority or proof of account ownership.

Every access checks the executing package's namespace, capability and frozen
service registration, including reads already present in another handler's
shared overlay. Another package's private/public server cells are inaccessible.
Only queried values are captured; the engine does not copy every player's private
state into every invocation. Up to 64 distinct cells can be read or written in
one plan, under the shared operation budget. Private bytes obey the service's
registered bound (at most 4096); public bytes are at most 1024.

`set_profile_state` replaces both byte strings and preserves that profile's
scheduled deadline. Ordinary actions and admitted lifecycle callbacks may write;
`PlayerJoining` may query policy but cannot write. The lifecycle decision remains
the scheduling API. A lifecycle callback can use this setter for its current
service/profile instead of returning state, and can update other owned services
or profiles. Setting the current cell both ways rejects the entire callback to
avoid ambiguous precedence.

State changes and inventory rewards share one WAL transaction. Reads reserve
owner keys, including missing cells; revision and existence checks protect against
stale writes and insert races. Pending profile-state reservations also delay that
profile's admission until publication. Public delivery updates only affected local
profiles after receipt. Errors, including caught errors, discard the entire plan;
request replay returns its receipt without running the callback again. General
writes also clear failed timer suppression for the affected service/profile.

The welcome fixture demonstrates package roles and bans. Its Admin policy
command saves a package role in mode 2; native command permissions are unchanged. Mode 3
saves ban policy and stages a kick for the selected session. The durable policy
controls subsequent admission, while the transient kick cannot affect a
replacement session and is not replayed after a crash. Admission denial currently
closes the pending handshake; the reason is logged server-side rather than sent
as a dedicated admission-rejection frame. These policies use claimed profiles,
not authenticated remote accounts or a new global role database.


## Profile inventory transactions

Gameplay actions and post-admission lifecycle callbacks can pass a
`BloxProfileId` to `inventory`, `give`, `take`, `move_slots`, and
`transfer_inventory`. `"player"` remains the host-selected actor alias. Obtain
another profile from the captured directory or explicitly decode a canonical
`profile:<32 lowercase hex digits>` through `c.profile_id`. Session handles,
strings, table lookalikes and zero profiles are rejected. A profile identity does
not grant authority: every other-profile access, including reads and cached
reads across composed handlers, requires the executing package's
`bloxgloom:players/v1` capability. Command caller permissions remain separate.
Joining cannot publish inventory mutations; use Joined or a durable profile timer.

```luau
local target = c.player_by_session(e.command_arguments[1])
assert(target)
assert(c.transfer_inventory("player", e.slot, target.profile, 0, 1))
c.set_profile_state("welcome:policy", target.profile, "role:builder", "builder")
```

Profile inventories have 36 slots. Method slot arguments are zero-based and
returned readonly sequences are one-based. Transfers preserve exact component
bytes and total counts, respect insertion/extraction permissions and the 128-item
stack cap, and return false without partial fulfillment if the requested move
cannot be made. `give` explicitly creates items; `take` explicitly consumes them.
A callback can capture its actor and up to seven other profile inventories, under
the shared 4096-operation budget. Inventory, package-owned profile-state, and
supported ordinary-action world/entity effects share one WAL transaction. Invalid
host calls latch rejection even through `pcall`; terminal errors, stale captures,
quota failures and oversized WAL records publish none of the candidate effects.
Readonly queries and exact round trips retain dependencies without writing an
unchanged inventory.

An offline profile loads asynchronously through a bounded worker pool. While a
load is pending, the whole candidate is deferred and retried in a fresh VM; partial
work is discarded. This also applies to an offline profile timer's actor inventory.
A missing file uses the native empty inventory; a corrupt file rejects access and
never silently replaces saved items. Committed overlays take precedence over
pending disk results. Inventory revisions and read reservations fence conflicting
native/script writes and admission until the outstanding receipt is applied.
Nothing reads a client-supplied inventory as authoritative.

The target is the durable profile, so a completed transfer survives disconnect,
reconnect and restart and appears in that profile's next inventory snapshot.
Each affected online profile receives its own reliable committed inventory frame;
queue pressure uses the normal disconnect/reconnect recovery policy. Appearance,
teleport, notices and kicks still target an exact session instead. Action receipt
replay never repeats transfers or profile-state changes. Readonly committed
observers receive each affected profile and inventory revision, never private
slots, and retain their advisory delivery guarantees.

The welcome package's Admin command `/welcome:manage <player> 4` demonstrates a
one-item transfer from the caller's selected slot into the first compatible target
slot, followed by a receipt-bound notice. It creates no items and refuses self,
empty-source and full-target attempts. No world schema or wire-version change is
needed for these existing inventory-domain participants.
