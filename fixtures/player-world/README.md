# Regions and chat

Run `cargo run --release -- local-packages fixtures/player-world/packages /tmp/bloxgloom-region-test-v25`.

Close menus, then press T or Enter to open chat. Type `hello` and press Enter;
your own message appears in the transcript. `spoiler` is rejected by the demo
moderator; `/self hello` is delivered only to the sender.

Press F3 to see position coordinates. The region requires **both** X and Z to be
between -16 (inclusive) and 16 (exclusive), with feet Y between -256 and 512.
An X boundary crossing while Z is outside this range does not enter the region.
At X=0, Z=0 you are inside; move to X greater than or equal to 16 while keeping
Z inside to leave, then return. Region notices appear in chat history as well
as the brief status message, so T lets you read them again.
