# Spinning crates

Run `cargo run --release -- local-packages fixtures/rigid-bodies/packages /tmp/bloxgloom-rigid-bodies`.

Use the admin command `/rigid:launch` to throw a small spinning blue cuboid beside the player. It falls, bounces, rotates against terrain, and loses motion through drag and contact friction. Stop/restart in the same save to continue committed body poses. Bodies expire after 3,000 active logical ticks; unloaded or uninterested terrain pauses simulation. A blocked launch rejects the transaction.

The server publishes physical rotation and authoritative position. This fixture uses the cuboid fallback renderer and requires no external model assets. Terrain capture and spawn admission conservatively reserve space for all orientations, so give the crate room to rotate.
