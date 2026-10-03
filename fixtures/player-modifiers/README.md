# Movement effects

Run `cargo run --release -- local-packages fixtures/player-modifiers/packages /tmp/bloxgloom-movement-test-v25b`.

Open F4, type `pace:self 1` into the Command field and click Run for a saved
25% speed boost. `pace:self 2` applies a temporary 50% speed multiplier lasting
600 logical ticks; `pace:self 3` clears both. Close F4 and disable Flying to
compare walking and sprinting. Effects multiply, so applying both gives 62.5%
of base speed until the temporary effect expires.

To target someone else, use `pace:movement "Player Name" 1` with their actual
online display name (quote names containing spaces). The first argument is a
name or exact session token, not a player number. F4 lists registered commands;
clicking one fills the text field for editing, rather than selecting arguments.

A reconnect preserves the profile boost and clears the session slow. A restart
pauses the expiry clock while stopped. The server remains authoritative and
publishes a matching prediction reset when effective rules change. Use a fresh
save for this updated fixture because its registered commands changed.
