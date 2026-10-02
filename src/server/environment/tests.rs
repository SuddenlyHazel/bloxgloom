use super::*;
#[test]
fn captured_environment_is_historical_and_overrides_invalidate_fences() {
    let root = std::env::temp_dir().join(format!(
        "bloxgloom-env-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let mut clock = crate::server::world_time::Clock::open(&root).unwrap();
    let mut weather = crate::server::weather::Clock::open(&root, 7).unwrap();
    let captured = Capture::from_clocks(&clock, &weather);
    let historical = captured.value;
    let mut reads = TerrainReads::default();
    captured.fence(&mut reads);
    assert!(captured.is_current());
    assert!(
        reads.clock.as_ref().unwrap().is_current() && reads.weather.as_ref().unwrap().is_current()
    );
    clock.apply(clock.prepare(12345).unwrap()).unwrap();
    assert_eq!(captured.value, historical);
    assert!(!captured.is_current() && !reads.clock.as_ref().unwrap().is_current());
    let captured = Capture::from_clocks(&clock, &weather);
    assert!((12345..13345).contains(&captured.value.world_time.elapsed_ms));
    captured.fence(&mut reads);
    weather.apply(weather.prepare(1, 0).unwrap()).unwrap();
    assert!(!captured.is_current() && !reads.weather.as_ref().unwrap().is_current());
    clock.finish().unwrap();
    weather.finish().unwrap();
    std::fs::remove_dir_all(root).unwrap();
}
