# Health, damage, death and respawn

Every profile has server-owned health, including worlds without a health package.
The default is 100 current / 100 maximum health. Inventory is kept on death.
A native health bar and death screen show only committed server state; the
**Respawn** button submits an ordinary receipt-bound action. Health and death
survive disconnects and server restarts.

Run [the playable fixture](../../fixtures/player-health/README.md) to try
self-damage, healing, killing and manual respawn through native commands.
This feature does not automatically add fall, contact, weather or weapon damage.
Teams, weapons and additional modifier domains remain separate work.

## Gameplay bindings

Trusted server packages require `bloxgloom:players/v1` for mutations. A health
query accepts an exact online session handle and returns readonly
`{current, max, alive, revision, life}`. Revision is an exact `BloxRevision` token;
life is an exact `BloxTick` token used as an identity, not elapsed world time.
Neither handle is authority. All mutation targets must still be that captured,
live session and the captured health revision must match.

```luau
local player = c.player_by_profile(c.player_profile)
local hp = c.player_health(player.session)
c.damage_player(player.session, hp.revision, 25, "demo:burn")
c.heal_player(player.session, hp.revision, 10)
c.set_player_max_health(player.session, hp.revision, 150)
```

Amounts and maximum health are integers in 1..1,000,000. Current health stays
within 0..max. Healing a dead player is rejected; raising maximum health does
not revive them. Lowering maximum health clamps current health. Damage causes
must be valid namespaced keys owned by the executing package. Coordinates,
health amounts and targets supplied by a client are never accepted as outcomes.
Packages decide whom to damage from captured server observations.

The overlay reads its staged health changes, but the captured revision token
remains unchanged throughout one transaction. Absence has logical revision zero;
the first durable value has logical revision one. This distinguishes a fresh
profile from an initialized owner cell whose internal owner revision is zero.
Health and ordinary world/inventory/profile effects commit through one WAL.
Errors, caught-invalid bindings, unavailable terrain and conflicting revisions
publish none of those effects.

`c.respawn_player(session, revision, x, y, z)` is an authorized package operation
for a dead target. The final terrain, including edits made by callbacks in the
same transaction, must be authoritative, loaded, supported and unobstructed.
Unknown terrain defers the attempt. The native button chooses a safe origin
position using the server's frozen spawn policy; clients cannot choose that
position. Respawn restores maximum health and advances the life identity.

## Policies and transition hooks

Declare callbacks during startup, with own-package keys and modules:

```luau
host.register_damage_policy("demo:damage", 1, "demo:damage")
host.register_health_hook("demo:health", 1, "demo:hooks")
```

Each kind permits eight declarations per package, 32 per installation. They run
in lexical key order. Keys, explicit positive revisions and module contracts are
frozen and compared for compatible development reload. They are transient server
rules like chat moderation; their callbacks and private state are never delivered
to clients, and ordinary source edits can reload without changing saved health.

A damage policy receives immutable `{player, health, amount, cause}` and returns
an integer 0..1,000,000. Zero cancels damage; subsequent policies do not run.
It has no gameplay mutation host. A cancelled action with no other effects
follows the engine's ordinary “action made no change” result.

```luau
return function(event)
    return if event.cause == "demo:safe_zone" then 0 else event.amount
end
```

A health hook receives `(context, event)` and returns nil. Events are `died` and
`respawned`; fields are `{kind, player, before, health, cause}`. Cause is present
for death. Hooks see the proposed health overlay and can stage normal package
inventory/profile/world effects. Use `event.player.profile` for the affected
inventory: `"player"` refers to the initiating actor, who may be someone else.

Hooks execute in the original transaction, under their own package namespace.
Recursive health mutations inside a transition hook are rejected. A failed hook
rolls back the transition and its other effects. Retries may execute a callback
again; only one committed transition/reward is published. Receipt replay and WAL
recovery do not repeat callbacks. Logs and other observations remain advisory.

## Authority and recovery

Dead players cannot move, jump, change movement posture, collect drops or perform
ordinary inventory/world actions. Movement velocity, held input, sprint and
prediction are cleared at death/respawn through the existing reset barrier.
The exact session epoch stays stable; a separate life envelope rejects old queued
or newly replayed movement/actions after respawn. Chat protocol messages and receipt ACKs remain
usable; the native death screen focuses on manual respawn. The initial life uses the existing unwrapped input form; later lives
require the matching envelope. All queued commands retain their admitted life.

The reserved `bloxgloom:player_health` profile system cannot be accessed through
arbitrary `profile_state` writes or queries. Its bounded canonical codec stores
health and the last committed respawn checkpoint. Position files retain the last
applied life: reconnect applies a newer WAL checkpoint after a crash before
native teleport publication, while preserving movement saved after that respawn.
Malformed health/position data fails closed. No world converter is provided.
The default world folder is v26, wire contract 32 and client runtime 14.
