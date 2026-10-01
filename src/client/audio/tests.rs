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
    assert_eq!(config.audio_master, 0.8);
}

#[test]
fn world_sound_is_remembered_during_preview_and_retired_with_session() {
    let config = Config::default();
    let mut state = State::new(&config);
    state.update_weather(10.0, 4.0, 0.5, 1.0);
    let sample = state.world;
    assert_eq!(sample.unwrap().rain_mm_h, 10.0);
    state.change_preview(true, &config);
    assert_eq!(state.world, sample);
    state.thunder(400.0, 0.0, 1.0);
    state.retire_session(&config);
    assert_eq!(state.world, None);
    assert_eq!(state.sent_world, None);
}
