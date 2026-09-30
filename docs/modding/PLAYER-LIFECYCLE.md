# Player lifecycle implementation

This is the active reference for the player-services goal. The
[gap assessment](../../LUAU-SCRIPTING-GAPS.md#2-player-and-lifecycle-hooks)
defines the remaining scope. VM lifetime, chat/combat, region hooks and save
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
readonly authoritative `position`, `online=true` and
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
The client bundle carries an inert service identity with runtime contract 6;
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
Lifecycle callbacks currently support ordinary inventory operations for their
profile and gameplay queries; world/entity writes are rejected as a whole.

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

Wire version 15 carries a bounded, sorted roster of at most 256 admitted players,
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

Wire 15 delivers a full local-player snapshot containing each frozen player-service
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
