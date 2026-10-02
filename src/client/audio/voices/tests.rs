use super::*;

fn state() -> State {
    let mut state = State::new(&Config::default());
    state.install_sounds(crate::audio::sounds::builtin_cached().unwrap());
    state.obstruction.enable_for_test();
    state
}

fn event(kind: Kind) -> Event {
    Event {
        owner: "demo".into(),
        voice: "impact".into(),
        kind,
    }
}

fn play(entity: Option<u64>, looping: bool) -> Event {
    event(Kind::Play {
        clip: "bloxgloom:break".into(),
        position: [1.0; 3],
        entity,
        gain: 1.0,
        pitch: 1.0,
        looping,
    })
}

#[test]
fn pending_one_shot_expiry_begins_at_atomic_native_admission() {
    let mut state = state();
    let now = Instant::now();
    state.sounds(true, None, vec![play(None, false)], now);
    let source = state.obstruction_sources()[0];
    assert!(state.sent.borrow().is_empty());
    assert!(
        state
            .voices
            .active
            .values()
            .next()
            .unwrap()
            .expires
            .is_none()
    );
    let start = now + Duration::from_millis(70);
    assert!(state.apply_obstruction(source.id, source.position, 0.2, 800.0, start));
    let voice = state.voices.active.values().next().unwrap();
    assert!(voice.pending.is_none());
    assert!(voice.expires.unwrap() > start);
    assert!(matches!(
        state.sent.borrow().as_slice(),
        [Command::PlayObstructed {
            transmission: 0.2,
            lowpass_hz: 800.0,
            looping: false,
            ..
        }]
    ));
}

#[test]
fn stopping_pending_voice_emits_no_native_stop_and_late_profile_cannot_resurrect_it() {
    let mut state = state();
    let now = Instant::now();
    state.sounds(true, None, vec![play(Some(7), true)], now);
    let source = state.obstruction_sources()[0];
    state.sounds(true, None, vec![event(Kind::Stop)], now);
    assert!(state.sent.borrow().is_empty());
    assert!(state.obstruction_sources().is_empty());
    assert!(!state.apply_obstruction(source.id, source.position, 0.2, 800.0, now));
    state.start_overdue_obstruction(now + Duration::from_secs(1));
    assert!(state.sent.borrow().is_empty());
}

#[test]
fn pending_profile_retries_without_starting_or_expiring_when_native_queue_is_full() {
    let mut state = state();
    let now = Instant::now();
    state.sounds(true, None, vec![play(None, false)], now);
    let source = state.obstruction_sources()[0];
    state.blocked.set(true);
    assert!(!state.apply_obstruction(source.id, source.position, 0.3, 1500.0, now));
    assert!(
        state
            .voices
            .active
            .values()
            .next()
            .unwrap()
            .pending
            .is_some()
    );
    assert!(
        state
            .voices
            .active
            .values()
            .next()
            .unwrap()
            .expires
            .is_none()
    );
    assert!(state.sent.borrow().is_empty());
    state.blocked.set(false);
    assert!(state.apply_obstruction(source.id, source.position, 0.3, 1500.0, now));
    assert!(matches!(
        state.sent.borrow().as_slice(),
        [Command::PlayObstructed {
            transmission: 0.3,
            lowpass_hz: 1500.0,
            ..
        }]
    ));
}

#[test]
fn moving_pending_entity_updates_capture_without_native_update_and_rejects_old_voxel() {
    let mut state = state();
    let now = Instant::now();
    state.sounds(true, None, vec![play(Some(7), true)], now);
    let source = state.obstruction_sources()[0];
    state.follow_sounds(now, |_| Some([0.99, 1.0, 1.0]));
    assert!(state.sent.borrow().is_empty());
    assert_eq!(state.obstruction_sources()[0].position, [0.99, 1.0, 1.0]);
    assert!(!state.apply_obstruction(source.id, source.position, 0.2, 800.0, now));
    assert!(state.apply_obstruction(source.id, [0.99, 1.0, 1.0], 0.2, 800.0, now));
    assert!(matches!(
        state.sent.borrow().as_slice(),
        [Command::PlayObstructed {
            position: [0.99, 1.0, 1.0],
            ..
        }]
    ));
}

#[test]
fn pending_start_falls_back_after_100ms_and_pending_changes_are_used_at_admission() {
    let mut state = state();
    let now = Instant::now();
    state.sounds(true, None, vec![play(None, false)], now);
    state.sounds(
        true,
        None,
        vec![event(Kind::Update {
            position: Some([80.0, 1.0, 1.0]),
            gain: 0.4,
            pitch: 2.0,
        })],
        now,
    );
    state.start_overdue_obstruction(now + Duration::from_millis(99));
    assert!(state.sent.borrow().is_empty());
    state.start_overdue_obstruction(now + Duration::from_millis(100));
    assert!(matches!(
        state.sent.borrow().as_slice(),
        [Command::PlayObstructed {
            position: [80.0, 1.0, 1.0],
            gain: 0.4,
            pitch: 2.0,
            transmission: 0.35,
            lowpass_hz: 2400.0,
            ..
        }]
    ));
    assert!(
        state
            .voices
            .active
            .values()
            .next()
            .unwrap()
            .expires
            .is_some()
    );
}
