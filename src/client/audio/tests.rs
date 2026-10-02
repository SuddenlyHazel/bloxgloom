use super::*;
#[test]
fn local_audio_preview_is_ephemeral_and_listener_poll_is_bounded() {
    let config = Config::default();
    let mut state = State::new(&config);
    assert!(
        state.output.is_none(),
        "unit test constructors must not open host devices"
    );
    state.change_preview(true, &config);
    assert_eq!(state.preset(), Preset::Rain);
    state.test_sound();
    assert_eq!(state.next_voice, 2);
    let now = Instant::now();
    state.poll_listener([0.0; 3], 0.0, now);
    state.poll_listener([1.0; 3], 1.0, now + Duration::from_millis(10));
    assert_eq!(state.last_poll, Some(now));
    state.retire_session(&config);
    assert_eq!(state.preset(), Preset::Off);
    assert_eq!(state.last_poll, None);
    assert_eq!(
        state.next_voice, 2,
        "session reset cannot reuse live voice IDs"
    );
    assert_eq!(config.audio_master, 1.0);
}

#[test]
fn world_sound_is_remembered_during_preview_and_retired_with_session() {
    let config = Config::default();
    let mut state = State::new(&config);
    state.update_weather(10.0, 4.0, 0.5, 1.0, 0.0);
    let sample = state.world;
    assert_eq!(sample.unwrap().rain_mm_h, 10.0);
    state.change_preview(true, &config);
    assert_eq!(state.world, sample);
    state.thunder(400.0, 0.0, 1.0);
    state.retire_session(&config);
    assert_eq!(state.world, None);
    assert_eq!(state.sent_world, None);
}

fn event(voice: &str, entity: Option<u64>, looping: bool) -> bloxgloom_host_api::sound::Event {
    use bloxgloom_host_api::sound::{Event, Kind};
    Event {
        owner: "demo".into(),
        voice: voice.into(),
        kind: Kind::Play {
            clip: "bloxgloom:break".into(),
            position: [1.0; 3],
            entity,
            gain: 1.0,
            pitch: 1.0,
            looping,
        },
    }
}
#[test]
fn sound_delivery_deduplicates_follows_entities_and_retires_loops() {
    let mut state = State::new(&Config::default());
    state.install_sounds(crate::audio::sounds::builtin_cached().unwrap());
    let now = Instant::now();
    state.sounds(true, Some(9), vec![event("machine", Some(7), true)], now);
    state.sounds(true, Some(9), vec![event("machine", Some(7), true)], now);
    state.sounds(true, Some(10), vec![event("machine", Some(7), true)], now);
    assert_eq!(state.sent.borrow().len(), 1);
    state.follow_sounds(now, |id| {
        assert_eq!(id, 7);
        Some([2.0; 3])
    });
    assert!(matches!(
        state.sent.borrow().last(),
        Some(Command::Update {
            position: Some([2.0, 2.0, 2.0]),
            ..
        })
    ));
    state.follow_sounds(now + Duration::from_millis(50), |_| None);
    assert!(matches!(state.sent.borrow().last(), Some(Command::Stop(_))));
    state.retire_session(&Config::default());
    state.install_sounds(crate::audio::sounds::builtin_cached().unwrap());
    state.sounds(true, Some(9), vec![event("machine", Some(7), true)], now);
    assert!(matches!(
        state.sent.borrow().last(),
        Some(Command::Play { .. })
    ));
}
#[test]
fn invalid_sound_batches_are_atomic_and_client_voices_cannot_stop_server_voices() {
    use bloxgloom_host_api::sound::{Event, Kind};
    let mut state = State::new(&Config::default());
    state.install_sounds(crate::audio::sounds::builtin_cached().unwrap());
    let now = Instant::now();
    state.sounds(
        true,
        Some(1),
        vec![
            event("good", None, false),
            Event {
                owner: "demo".into(),
                voice: "bad".into(),
                kind: Kind::Play {
                    clip: "demo:missing".into(),
                    position: [0.0; 3],
                    entity: None,
                    gain: 1.0,
                    pitch: 1.0,
                    looping: false,
                },
            },
        ],
        now,
    );
    assert!(state.sent.borrow().is_empty());
    state.sounds(true, Some(2), vec![event("loop", Some(7), true)], now);
    state.sounds(
        false,
        None,
        vec![Event {
            owner: "demo".into(),
            voice: "loop".into(),
            kind: Kind::Stop,
        }],
        now,
    );
    assert_eq!(state.sent.borrow().len(), 1);
    state.sounds(
        true,
        Some(3),
        vec![Event {
            owner: "demo".into(),
            voice: "loop".into(),
            kind: Kind::Update {
                position: None,
                gain: 0.5,
                pitch: 2.0,
            },
        }],
        now,
    );
    assert!(matches!(
        state.sent.borrow().last(),
        Some(Command::Update {
            gain: 0.5,
            pitch: 2.0,
            ..
        })
    ));
}

#[test]
fn audio_voice_admission_is_bounded_and_finished_one_shots_release_slots() {
    let mut state = State::new(&Config::default());
    state.install_sounds(crate::audio::sounds::builtin_cached().unwrap());
    let now = Instant::now();
    let events = (0..32)
        .map(|n| event(&format!("voice-{n}"), None, false))
        .collect();
    state.sounds(false, None, events, now);
    state.sounds(false, None, vec![event("extra", None, false)], now);
    assert_eq!(state.sent.borrow().len(), 32);
    state.follow_sounds(now + Duration::from_secs(1), |_| None);
    state.sounds(
        false,
        None,
        vec![event("extra", None, false)],
        now + Duration::from_secs(1),
    );
    assert_eq!(state.sent.borrow().len(), 33);
}

#[test]
fn entity_stop_retries_after_native_queue_pressure() {
    let mut state = State::new(&Config::default());
    state.install_sounds(crate::audio::sounds::builtin_cached().unwrap());
    let now = Instant::now();
    state.sounds(false, None, vec![event("motor", Some(5), true)], now);
    state.blocked.set(true);
    state.follow_sounds(now, |_| None);
    assert_eq!(state.sent.borrow().len(), 1);
    state.blocked.set(false);
    state.follow_sounds(now + Duration::from_millis(50), |_| None);
    assert!(matches!(state.sent.borrow().last(), Some(Command::Stop(_))));
}

#[test]
fn ordered_sound_batches_suppress_old_delivery_for_the_entire_session() {
    let mut state = State::new(&Config::default());
    state.install_sounds(crate::audio::sounds::builtin_cached().unwrap());
    let now = Instant::now();
    state.sounds(true, Some(300), vec![event("current", None, false)], now);
    state.sounds(true, Some(1), vec![event("old", None, false)], now);
    assert_eq!(state.sent.borrow().len(), 1);
}

#[test]
fn latest_rain_geometry_retries_queue_pressure_and_retires_with_session() {
    use crate::audio::rain_scene::{RainMaterial, RainScene};
    let config = Config::default();
    let mut state = State::new(&config);
    state.blocked.set(true);
    state.update_rain_scene((*RainScene::patch(RainMaterial::Leaf)).clone());
    state.update_weather(30.0, 4.0, 0.35, 1.0, 0.0);
    assert!(state.scene_dirty);
    state.update_rain_scene((*RainScene::patch(RainMaterial::Wood)).clone());
    state.blocked.set(false);
    state.update_weather(30.0, 4.0, 0.35, 1.0, 0.0);
    assert!(!state.scene_dirty);
    assert!(
        matches!(&state.sent.borrow()[0], Command::RainScene(scene) if scene.tiles.iter().all(|t| t.material == RainMaterial::Wood))
    );
    let count = state.sent.borrow().len();
    state.update_rain_scene((*RainScene::patch(RainMaterial::Wood)).clone());
    state.update_weather(30.0, 4.0, 0.35, 1.0, 0.0);
    assert_eq!(state.sent.borrow().len(), count);
    state.retire_session(&config);
    assert!(state.rain_scene.is_none());
    assert!(!state.scene_dirty);
}
