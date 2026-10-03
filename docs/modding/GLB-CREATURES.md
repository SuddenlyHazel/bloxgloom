# Packaged GLB creatures

Content packs can register native GLBs with embedded PNG textures, baked node or
skeletal animations, named layers/variants, and multiply/replace colors. Rust/wgpu
draws them with voxel lighting, fog, depth and matching cutout sun shadows. Models
are verified and decoded before play, then shared across instances.

## Try the fixture

```sh
cargo run --release -- local-packages fixtures/glb-creatures/packages /tmp/bloxgloom-glb-creatures-save
```

On clear ground, enter `spawn sprout:sproutling` in F4. Close the menu and
right-click its body within reach. Each pat toggles eyes, hat, mouth and body
color, and plays a baked nod once. The creature pauses and walks a short loop;
`bounce` demonstrates an authored walking clip. A second client sees the same
controls. Restarting the same save retains appearance and the private pat counter.

```sh
cargo run --release -- creature-preview sprout:sproutling /tmp/glb-sproutling.png fixtures/glb-creatures/packages
```

The fixture reuses the project's generated `fixtures/authored-model/model.glb`.
It contains no third-party character artwork. Package changes alter content
identity; use a fresh save when changing assets or declarations.

## Registration

In format-2 `package.txt`, declare these alongside server modules:

```text
requires bloxgloom:content/v1
requires bloxgloom:mobile_entities/v1
asset model sprout assets/models/sprout.glb
asset model-controls looks assets/models/looks.json
```

Register the asset before its creature:

```luau
host.register_model {
    key = 'sprout:model', asset = 'sprout:sprout', controls = 'sprout:looks', scale = 1,
}
host.register_creature {
    key = 'sprout:sproutling', module = 'sprout:creature', schema = 1, revision = 1,
    max_state_bytes = 8, initial_state = '0', interval = 10,
    body = {half_width = 0.36, height = 1.9, speed = 1}, interaction = 'pat',
    model = {key = 'sprout:model', idle = 'idle', walk = 'bounce', run = 'bounce'},
}
```

`model = 'sprout:model'` works without locomotion mappings. Optional model and
creature scales multiply. The separate `body` defines collision and reach;
animation never changes physics. Cuboid declarations continue to work.

Author GLB creatures facing **-Z**, with **+Y** up. Creature rendering applies
one 180° yaw correction to map that front to game **+Z** at yaw zero and **+X**
at yaw π/2. The same correction applies to sun casters. Keep it out of exported
animation tracks and scripts.

Asset keys belong to the declaring package. Frozen manifest identities include
exact GLB/controls bytes and scale; creature bindings also affect compatibility.
Clients download verified bytes through the existing package path and never run
the server creature callback.

## Controls and playback

Controls JSON follows the native `model-preview` schema. Targets are exported node
or material names. See the fixture's `assets/models/looks.json` for a complete
example: `variants` selects named node groups, `layers` controls subtree visibility,
`tints` selects materials/nodes, and `loops` maps clip names to loop intent.

Authored creatures can use these in tick or interaction callbacks:

```luau
c.set_variant('eyes', 'sleepy')
c.set_layer('hat', true)
c.set_tint('body', {rgb = {210, 130, 235}, mode = 'replace'})
c.play_animation {clip = 'nod', speed = 1, looping = false,
                  crossfade_ms = 180, restart = true}
-- Later: c.stop_animation(200)
```

RGB values are integer bytes. Multiply preserves painted color variation; replace
substitutes selected texture RGB while preserving alpha and world lighting.
Overrides persist and replicate. `c.visual` provides a readonly summary of the
current explicit clip, speed and looping selection.

Speed defaults to 1 (0–8), crossfade to 200 ms (0–5000), and looping to the controls
JSON, or false when absent. Locomotion mappings loop automatically. Selecting the same clip/speed/loop choice preserves
phase; `restart = true` retriggers it. Stopping or completing a nonloop clip
returns visually to idle/walk/run from presented grounded movement. Interrupted
fades start from the current pose. Playback uses committed server ticks so joining
observers start at its current phase. Clip completion never triggers gameplay.

At most 16 visual calls are allowed per callback. Unknown names, invalid values or
failed calls reject the whole candidate update, including errors caught by
`pcall`; no partial appearance update commits.

## Bounds and verification

- Eight models/package, 128/catalog; 512 visible actors.
- GLB up to 8 MiB, controls up to 64 KiB; package aggregate bounds also apply.
- Embedded PNGs up to 2048×2048. Conservative decoded admission is 128 MiB for a
  model/catalog, with a 256 MiB process reservation for live prepared assets.
- Replicated controls support 16 variant groups, 32 layers and 16 tints.
- Native geometry, skin, node, clip and sampler bounds also apply. Self-contained
  triangle meshes are required; unsupported transparency, morph targets, external
  resources and required extensions fail startup.

Allocation estimates are checked before decode. Shared assets hold their memory
reservation until released. Registration, download and renderer caps remain
independent.

`packaged_glb_creature_two_clients_share_controls_and_restart` exercises the real
nonblocking listener, downloaded assets, two native client projections,
interaction and durable restart with an isolated save. Renderer tests cover
independent instances, baked poses, layers, colors and shadow/depth behavior.
