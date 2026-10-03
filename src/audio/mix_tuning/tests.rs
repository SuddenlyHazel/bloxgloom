use super::*;

#[test]
fn old_or_empty_configs_preserve_transparent_defaults() {
    let config: MixConfig = serde_json::from_str("{}").unwrap();
    assert_eq!(config, MixConfig::default());
    assert!(config.buses.iter().all(|bus| bus.gain == 1.0));
    assert!(config.buses.iter().all(|bus| !bus.compressor.enabled));
    assert!(!config.master.enabled);
}

#[test]
fn bounds_reject_nonfinite_and_clamp_excessive_controls() {
    let invalid = CompressorConfig {
        enabled: true,
        threshold_db: f32::NAN,
        ratio: f32::INFINITY,
        attack_ms: -1.0,
        release_ms: 9000.0,
        makeup_db: -2.0,
        knee_db: 90.0,
    };
    let mut config = MixConfig::default();
    config.buses[0] = BusConfig {
        gain: f32::NEG_INFINITY,
        compressor: invalid,
    };
    config.buses[1].gain = 50.0;
    config.buses[2].gain = -50.0;
    config.master = invalid;
    let config = config.sanitized();
    assert_eq!(config.buses[0].gain, 1.0);
    assert_eq!(config.buses[1].gain, 4.0);
    assert_eq!(config.buses[2].gain, 0.0);
    assert_eq!(config.master, config.buses[0].compressor);
    assert_eq!(config.master.threshold_db, -18.0);
    assert_eq!(config.master.ratio, 4.0);
    assert_eq!(config.master.attack_ms, 0.1);
    assert_eq!(config.master.release_ms, 2000.0);
    assert_eq!(config.master.makeup_db, 0.0);
    assert_eq!(config.master.knee_db, 24.0);
}

#[test]
fn clipboard_export_round_trips_sanitized_bus_order() {
    let mut config = MixConfig::default();
    config.buses[3].gain = 0.37;
    config.master.enabled = true;
    let value: serde_json::Value = serde_json::from_str(&config.export()).unwrap();
    assert_eq!(value["format"], "bloxgloom-audio-mix-v1");
    assert_eq!(value["bus_order"][3], "Music");
    let decoded: MixConfig = serde_json::from_value(value["mix"].clone()).unwrap();
    assert_eq!(decoded, config);
}
