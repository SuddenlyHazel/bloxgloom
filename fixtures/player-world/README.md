# Regions and chat

Run `cargo run --release -- local-packages fixtures/player-world/packages /tmp/bloxgloom-region-test-v26`.

Close menus, then press T or Enter to open chat. Type `hello` and press Enter;
your own message appears in the transcript. `spoiler` is rejected by the demo
moderator; `/self hello` is delivered only to the sender.

Walk or fly anywhere: crossing any 16×16×16 chunk boundary prints
`Left chunk x,y,z; entered chunk x,y,z` in chat. The server samples the player's
authoritative position every five logical ticks (about 200 ms). Standing still
does not repeat messages; reconnecting starts a new session tracker. Fast
teleports report the observed endpoints rather than every intermediate chunk.

Press F3 to see XYZ and chunk coordinates. The named spawn region requires **both** X and Z to be
between -16 (inclusive) and 16 (exclusive), with feet Y between -256 and 512.
An X boundary crossing while Z is outside this range does not enter the region.
At X=0, Z=0 you are inside; move to X greater than or equal to 16 while keeping
Z inside to leave, then return. Region notices appear in chat history as well
as the brief status message, so T lets you read them again.
