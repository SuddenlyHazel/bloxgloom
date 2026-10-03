# Packaged player models and baked animation

Packages register self-contained GLBs with embedded PNG textures and baked clips. Player rigs share the native Rust importer, wgpu renderer, named appearance controls and verified bundle delivery used by creature models. The builtin character remains the default.

Declare owned `asset model` and optional `asset model-controls` entries, then register:

```lua
host.register_player_model {
    key = "demo:player", asset = "demo:rig", controls = "demo:looks", scale = 1,
    clips = {
        idle = "idle", walk = "walk", run = "run", crouch = "crouch",
        tool_left = "tool_use_left", tool_right = "tool_use_right",
    },
    crossfade_s = 0.2,
    first_person_hide = {"head"},
    first_person_offset = {0, 0, -0.2},
}
```

All settings beyond key/asset are optional. Clip and node references must resolve at startup. Missing gait clips fall back to idle or rest; crouch and tool mappings are optional. Grounded motion drives walk/run selection, and explicit one-shot or looping playback temporarily overrides those baked mappings. Interrupted crossfades start from the visible pose. None of these visual clocks affect movement, collision, inventory or rewards.

Controls JSON supplies the same `variants`, `layers`, `tints` and `loops` as creature assets. Character shows the admitted player rigs and their actual controls, supports multiplying painted colors or replacing them, and previews their baked clips. Apply saves the model selection and look to the stable profile. Reconnect/restart clears transient explicit clip playback while preserving looks. Selecting a builtin character clears the package rig.

Player lifecycle and gameplay callbacks can use exact online session handles:

```lua
c.set_player_model(player.session, "demo:player") -- nil restores builtin
c.set_player_model_variant(player.session, "eyes", "sleepy")
c.set_player_model_layer(player.session, "hat", true)
c.set_player_model_tint(player.session, "iris", {rgb={80,180,210}, mode="replace"})
c.play_player_animation(player.session, {clip="wave", speed=1, looping=false, crossfade_ms=150})
c.stop_player_animation(player.session, 150) -- return to mapped locomotion
```

Passing nil to a variant/layer/tint setter restores that control's asset default. Missing models, clips or controls reject the callback plan; caught invalid calls still poison its transaction. Operations target the captured profile and action epoch, never a later replacement session.

`first_person_hide` lists named node subtrees hidden only from their owner's color rendering. Other players, third-person views and world shadows retain them. `first_person_offset` shifts only this camera presentation, without changing authoritative position or skinning in world shadows. Custom rigs do not inherit the builtin rig's arm framing or head-look IK; author their camera geometry and baked clips accordingly.

At most 8 model declarations per package and 128 per catalog are admitted. GLBs are bounded to 8 MiB, controls to 64 KiB, first-person hidden subtrees to 32 names, playback speed to 0..8 and fades to 0..5 seconds. Existing decoded CPU/GPU model limits still apply. Model bytes, controls, scale and player clip/camera settings enter content fingerprints; changing those contracts requires a fresh world. Ordinary compatible script behavior edits can reload or restart with the same save.

See [the playable fixture](../../fixtures/player-model/README.md). Full animation graphs, blend trees and vehicle features remain outside this baked-clip scope.
