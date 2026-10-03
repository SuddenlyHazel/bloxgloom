# Player movement modifiers

Gameplay and player lifecycle handlers requiring `bloxgloom:players/v1` can set package-owned movement effects. A profile handle selects durable effects; an exact session handle selects temporary effects that end on disconnect. Both lifetimes support optional expiry.

```luau
c.set_player_modifier(player.profile, "demo:training", {speed=1.25})
c.set_player_modifier(player.session, "demo:slow", {speed=0.5, duration_ticks=600})
c.remove_player_modifier(player.session, "demo:slow")
local ownEffects = c.player_modifiers(player.profile)
```

Options are `speed`, `sprint`, `jump`, `gravity` and `duration_ticks`. Omitted multipliers are 1. Each multiplier must be finite and between 0.1 and 4. Duration is an integer from 1 to 1,000,000 logical world ticks; omission means indefinite. Expiry pauses while the server is stopped, and continues for disconnected profiles while the server runs. Expiry cleanup uses the profile WAL so expired effects cannot return after an otherwise idle restart.

The readonly query returns active effects belonging to the calling package, sorted by key. Each view includes key, the four multipliers and an optional opaque `expires_at` tick. Effect IDs must use the calling package's namespace. One package cannot edit or query another package's effects. Replacing an existing key replaces that effect. The same key can exist in both lifetimes; both contribute.

There are at most 32 profile effects and 32 session effects per player. The server multiplies active effects in lexical order, then bounds the result: speed and sprint to 0.1–4; jump and gravity to 0.1–2. Final movement speed is capped at 16 blocks per second. The frozen base player rules remain unchanged. The server uses effective rules for movement authority and sends matching rules and a reset fence to client prediction.

Profile writes commit atomically with inventory and world edits through the existing WAL. Session writes are receipt-bound and target the captured profile and session epoch; they never replay onto replacement sessions after disconnect or restart. Invalid calls reject the entire invocation even when caught with `pcall`. The internal profile storage is reserved for this API and unavailable through generic `profile_state` reads or writes.

See the runnable fixture at `fixtures/player-modifiers/packages` and supplemental types in `docs/modding/types/player-modifiers.luau`.
