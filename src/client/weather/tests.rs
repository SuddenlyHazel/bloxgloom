use super::*;
use crate::weather::WeatherKind;

#[test]
fn shelter_uses_known_columns_and_roofs_instead_of_assuming_missing_air() {
    assert_eq!(column_cover(0, 20, 0, |_, _, _| None), f32::INFINITY);
    assert_eq!(
        column_cover(0, 20, 0, |_, _, _| Some(false)),
        f32::NEG_INFINITY
    );
    assert_eq!(column_cover(0, 20, 0, |_, y, _| Some(y == 24)), 25.0);
    assert_eq!(column_cover(0, 20, 0, |_, y, _| Some(y == 16)), 17.0);
    // A glass/leaf/other solid roof blocks rain regardless of opacity.
    assert_eq!(
        column_cover(-4, 20, -4, |x, y, z| Some(x == -4 && z == -4 && y == 35)),
        36.0
    );
}

#[test]
fn snapshots_interpolate_shared_time_and_ignore_stale_authority() {
    let mut state = State::default();
    let now = Instant::now();
    let mut snapshot = WeatherSnapshot::initial(4);
    snapshot.to = WeatherKind::Storm;
    snapshot.transition_duration_ms = 10_000;
    snapshot.revision = 1;
    state.synchronize(snapshot, now);
    assert_eq!(state.sample(now).rain, 0.0);
    assert_eq!(state.sample(now + Duration::from_secs(5)).rain, 0.5);
    let mut stale = snapshot;
    stale.revision = 0;
    stale.to = WeatherKind::Clear;
    state.synchronize(stale, now + Duration::from_secs(5));
    assert_eq!(state.snapshot.unwrap().to, WeatherKind::Storm);
}

#[test]
fn joining_a_storm_does_not_replay_old_lightning_and_flash_expires() {
    let mut state = State::default();
    let mut snapshot = WeatherSnapshot::initial(1);
    snapshot.to = WeatherKind::Storm;
    snapshot.elapsed_ms = 29_000;
    let strike = snapshot.lightning_at(snapshot.elapsed_ms).unwrap();
    state.synchronize(snapshot, Instant::now());
    assert_eq!(state.last_lightning, strike.id);
    assert_eq!(state.flash, None);
    assert!(state.pending_thunder.is_empty());
    assert_eq!(flash_at(0), 1.0);
    assert_eq!(flash_at(120), 0.65);
    assert_eq!(flash_at(1_000), 0.0);
}

#[test]
fn live_strikes_are_queued_once_even_when_crossing_regions_and_old_events_are_skipped() {
    let mut state = State::default();
    let mut snapshot = WeatherSnapshot::initial(4);
    snapshot.to = WeatherKind::Storm;
    state.synchronize(snapshot, Instant::now());
    let strike = snapshot.lightning_at(29_000).unwrap();
    state.track_strike(strike.elapsed_ms, Vec3::ZERO);
    assert_eq!(state.pending_thunder.len(), 1);
    assert_eq!(state.flash, Some(strike));
    state.track_strike(strike.elapsed_ms + 10, Vec3::ZERO);
    state.track_strike(strike.elapsed_ms + 20, Vec3::new(1_000.0, 40.0, -1_000.0));
    assert_eq!(state.pending_thunder.len(), 1);
    let later = snapshot.lightning_at(44_000).unwrap();
    state.track_strike(later.elapsed_ms + 2_000, Vec3::ZERO);
    assert_eq!(
        state.pending_thunder.len(),
        1,
        "stalled frames do not replay old strikes"
    );
    assert_eq!(state.last_lightning, later.id);
}
