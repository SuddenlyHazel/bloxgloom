# Movement effects

Run `cargo run --release -- local-packages fixtures/player-modifiers/packages /tmp/bloxgloom-player-modifiers`.

Use the admin command `/pace:movement <player> 1` for a saved 25% speed boost, mode 2 for a temporary 50% speed multiplier lasting 600 logical ticks, or mode 3 to clear both. Walk and sprint to compare effective movement. A reconnect preserves the profile boost and clears the session slow. A restart pauses the expiry clock while stopped. The server remains authoritative and publishes a matching prediction reset when effective rules change.
