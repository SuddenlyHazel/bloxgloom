use super::*;
use crate::weather::WeatherKind;

#[test]
fn grounded_and_hovering_weather_stays_exposed_at_vertical_chunk_edges() {
    use crate::world::{AIR, CHUNK_SIZE, CHUNK_VOLUME, Chunk, ChunkKey, STONE};
    use std::sync::Arc;
    // Include a negative boundary: streaming and weather must both floor-divide.
    for ground_y in [15.0, -1.0] {
        let mut app = super::super::ClientApp::new(
            super::super::Network::disconnected_for_test(),
            crate::config::Config::default(),
            std::env::temp_dir().join(format!("weather-cover-{}", std::process::id())),
        );
        let now = Instant::now();
        for (step, feet_y) in [ground_y, ground_y + 1.0].into_iter().enumerate() {
            app.position = Vec3::new(8.5, feet_y, 8.5);
            app.chunks.clear();
            let center = (feet_y.floor() as i32).div_euclid(CHUNK_SIZE as i32);
            let ground = ground_y as i32 - 1;
            for y in (center - crate::protocol::VERTICAL_VIEW_DISTANCE)
                ..=(center + crate::protocol::VERTICAL_VIEW_DISTANCE)
            {
                let key = ChunkKey { x: 0, y, z: 0 };
                let mut blocks = vec![AIR; CHUNK_VOLUME];
                if ground.div_euclid(CHUNK_SIZE as i32) == y {
                    for z in 0..CHUNK_SIZE {
                        for x in 0..CHUNK_SIZE {
                            blocks[Chunk::index([
                                x,
                                ground.rem_euclid(CHUNK_SIZE as i32) as usize,
                                z,
                            ])
                            .unwrap()] = STONE;
                        }
                    }
                }
                app.chunks
                    .insert(key, Arc::new(Chunk::from_blocks(key, 1, blocks)));
            }
            let camera = app.camera();
            app.present_weather(camera, now + Duration::from_secs(step as u64));
            assert_eq!(app.weather.target_exposure, 1.0, "feet at {feet_y}");
            assert!(
                app.weather.cover.iter().all(|roof| *roof == ground_y),
                "feet at {feet_y}"
            );
        }
    }
}

#[test]
fn audio_cover_ignores_missing_ground_but_keeps_real_and_unknown_roofs() {
    assert_eq!(
        column_cover(0, 20, 0, 79, |_, y, _| (y >= 20).then_some(false)),
        f32::NEG_INFINITY
    );
    assert_eq!(column_cover(0, 20, 0, 79, |_, y, _| Some(y == 25)), 26.0);
    assert_eq!(
        column_cover(0, 20, 0, 79, |_, y, _| (y != 30).then_some(false)),
        f32::INFINITY
    );
}

#[test]
fn shelter_uses_known_columns_and_roofs_instead_of_assuming_missing_air() {
    assert_eq!(column_cover(0, 4, 0, 84, |_, _, _| None), f32::INFINITY);
    assert_eq!(
        column_cover(0, 4, 0, 84, |_, _, _| Some(false)),
        f32::NEG_INFINITY
    );
    assert_eq!(column_cover(0, 4, 0, 84, |_, y, _| Some(y == 24)), 25.0);
    assert_eq!(column_cover(0, 4, 0, 84, |_, y, _| Some(y == 16)), 17.0);
    // A glass/leaf/other solid roof blocks rain regardless of opacity.
    assert_eq!(
        column_cover(-4, 4, -4, 84, |x, y, z| Some(x == -4 && z == -4 && y == 35)),
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
