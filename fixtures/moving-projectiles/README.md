# Moving seeds

Run with an isolated save:

```sh
cargo run --release -- local-packages fixtures/moving-projectiles/packages /tmp/bloxgloom-moving-seeds
```

Use F4 to give yourself `throw:seed` items. Select them and activate **Throw seed**
or **Launch guided seed** in the registered item actions. Launches travel east
from above your feet; they do not follow the camera's aim. Both actions consume
one seed atomically with the spawn, and blocked spawns leave the seed untouched.

The green cuboid uses gravity and bounces. When it lands on stone or grass with
an empty cell above, its impact transaction places tall grass and removes the
projectile. The blue cuboid flies in a loop using a persisted phase byte and
revision-fenced steering. Both return their consumed seed as a world drop on
expiry if they still exist; a planted seed is already consumed.

The server owns collision, lifetime, terrain changes and drop allocation.
Client sparks only mark visible replicas. Shapes and models use center-origin
coordinates in blocks. Stop and restart with the same package sources and save
to resume committed motion; dormant terrain pauses active lifetime.

Change package sources using a fresh save directory. This prerelease rejects
incompatible package records and does not convert them.
