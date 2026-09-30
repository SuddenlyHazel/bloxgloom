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

## Remaining implementation

Typed command targeting, lifecycle/admission/spawn registration, atomic durable
profile state, session state, scheduling, authorized player operations,
committed observers and client lifecycle/state delivery remain in progress.
The client/server/save contracts will be versioned when their formats change.
