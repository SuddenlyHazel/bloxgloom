use super::*;
fn schema() -> api::VisualSchema {
    api::VisualSchema {
        clips: vec!["idle".into(), "attack".into()],
        clip_loops: vec![true, false],
        variants: vec![("eyes".into(), vec!["open".into(), "closed".into()])],
        layers: vec!["hat".into()],
        tints: vec!["fur".into()],
    }
}
fn call(source: &str, initial: api::VisualState, tick: u64) -> mlua::Result<api::VisualState> {
    let lua = Lua::new();
    let f: Function = lua.load(source).eval()?;
    let host = lua.create_table()?;
    let (_, state) = visuals::with(&lua, &host, Some(&schema()), Some(initial), tick, || {
        host.set_readonly(true);
        f.call::<()>(host.clone())
    })?;
    Ok(state.unwrap())
}
#[test]
fn named_visual_calls_choose_registered_controls_and_preserve_time_until_restart() {
    let source = "return function(h) h.play_animation{clip='attack',speed=1.5,crossfade_ms=125}; h.set_variant('eyes','closed'); h.set_layer('hat',false); h.set_tint('fur',{rgb={12,128,240},mode='replace'}) end";
    let state = call(source, Default::default(), 100).unwrap();
    let clip = state.playback.unwrap();
    assert_eq!(clip.clip, 1);
    assert!(!clip.looping);
    assert_eq!(clip.started_tick, 100);
    assert_eq!(state.variants[0], 1);
    assert_eq!(state.layers[0], 0);
    assert_eq!(state.tints[0].unwrap().mode, api::TintMode::Replace);
    let repeated = call(source, state, 120).unwrap();
    assert_eq!(repeated.playback, Some(clip));
    assert_eq!(repeated.sample_tick, 120);
    let restarted = call(
        "return function(h) h.play_animation{clip='attack',restart=true} end",
        repeated,
        125,
    )
    .unwrap();
    assert_eq!(restarted.playback.unwrap().started_tick, 125);
    assert_ne!(restarted.playback.unwrap().sequence, clip.sequence);
    let stopped = call(
        "return function(h) h.stop_animation(50) end",
        restarted,
        130,
    )
    .unwrap();
    assert!(stopped.playback.is_none());
    assert_eq!(stopped.transition_s, 0.05);
}
#[test]
fn caught_invalid_visual_calls_and_excessive_calls_abort_entire_candidate() {
    for source in [
        "return function(h) pcall(function() h.play_animation{clip='missing'} end); h.set_layer('hat',true) end",
        "return function(h) pcall(function() h.set_tint('fur',{rgb={256,0,0}}) end) end",
        "return function(h) pcall(function() h.set_layer('missing',true) end) end",
        "return function(h) for i=1,17 do pcall(function() h.set_layer('hat',true) end) end end",
        "return function(h) h.play_animation{clip='idle',speed=0/0} end",
        "return function(h) h.play_animation{clip='idle',undeclared=true} end",
    ] {
        assert!(call(source, Default::default(), 100).is_err(), "{source}");
    }
}
#[test]
fn creature_codec_persists_visuals_and_only_projects_public_state() {
    let creature = ScriptCreature::client_authored(64, schema());
    let visual = call(
        "return function(h) h.play_animation{clip='idle'};h.set_layer('hat',false) end",
        Default::default(),
        100,
    )
    .unwrap();
    let state = Payload::new(State {
        yaw: 0.4,
        velocity: -1.,
        grounded: true,
        private: b"secret inventory".to_vec(),
        visual: Some(visual),
    });
    let bytes = creature.encode(&state).unwrap();
    let decoded = creature.decode(&bytes).unwrap();
    assert_eq!(creature.encode(&decoded).unwrap(), bytes);
    let public = creature.public(&decoded).unwrap();
    assert!(!public.windows(6).any(|w| w == b"secret"));
    assert_eq!(creature.visual(&public).unwrap(), Some(visual));
    assert!(creature.pose(&public).unwrap().grounded);
    let inert = ScriptCreature::client_authored(64, schema());
    assert_eq!(inert.visual(&public).unwrap(), Some(visual));
    let mut malformed = public;
    malformed[5] = 0;
    assert!(inert.pose(&malformed).is_err());
}
#[test]
fn cuboid_creatures_keep_the_existing_private_and_public_codec() {
    let creature = ScriptCreature::client(8);
    let mut expected = vec![1];
    expected.extend(0.4_f32.to_le_bytes());
    expected.extend((-1.0_f32).to_le_bytes());
    expected.push(1);
    expected.extend(3_u16.to_le_bytes());
    expected.extend(b"old");
    let payload = creature.decode(&expected).unwrap();
    assert_eq!(creature.encode(&payload).unwrap(), expected);
    let public = creature.public(&payload).unwrap();
    assert_eq!(public, [0.4_f32.to_le_bytes().as_slice(), &[1]].concat());
    assert!(creature.visual(&public).unwrap().is_none());
    assert!(creature.pose(&public).unwrap().grounded);
}

#[test]
fn stop_then_play_in_the_same_tick_always_has_a_new_persisted_identity() {
    let first = call(
        "return function(h) h.play_animation{clip='idle'} end",
        Default::default(),
        100,
    )
    .unwrap();
    let next = call(
        "return function(h) h.stop_animation();h.play_animation{clip='idle'} end",
        first,
        100,
    )
    .unwrap();
    assert_eq!(
        first.playback.unwrap().started_tick,
        next.playback.unwrap().started_tick
    );
    assert_ne!(
        first.playback.unwrap().sequence,
        next.playback.unwrap().sequence
    );
    let encoded = next.encode(&schema()).unwrap();
    let restored = api::VisualState::decode(&encoded, &schema()).unwrap();
    assert_eq!(restored.sequence, next.sequence);
    let restarted = call(
        "return function(h) h.play_animation{clip='idle',restart=true} end",
        restored,
        100,
    )
    .unwrap();
    assert_ne!(
        restarted.playback.unwrap().sequence,
        next.playback.unwrap().sequence
    );
}
#[test]
fn interaction_uses_captured_current_tick_and_is_identical_on_retry() {
    let packages =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/glb-creatures/packages");
    let declarations = crate::server::script::startup::Declarations::discover(&packages).unwrap();
    let definition = &declarations.creatures[0];
    let initial = definition.behavior.initial();
    let before = definition.behavior.encode(&initial).unwrap();
    let interacted = definition
        .behavior
        .interact_at_tick(&initial, b"pat", 9, 1, 1000)
        .unwrap();
    let retry = definition
        .behavior
        .interact_at_tick(&initial, b"pat", 9, 1, 1000)
        .unwrap();
    let visual = definition
        .behavior
        .visual(&definition.behavior.public(&interacted).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(visual.sample_tick, 1000);
    assert_eq!(visual.playback.unwrap().started_tick, 1000);
    assert_eq!(
        definition.behavior.encode(&interacted).unwrap(),
        definition.behavior.encode(&retry).unwrap()
    );
    assert_eq!(definition.behavior.encode(&initial).unwrap(), before);
}
