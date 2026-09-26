use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

fn test_directory(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    env::temp_dir().join(format!("bloxgloom-{label}-{}-{unique}", std::process::id()))
}

#[test]
fn config_round_trips_through_explicit_path() {
    let directory = test_directory("config-roundtrip");
    let path = directory.join("settings/config");
    let config = Config {
        sensitivity: 0.006,
        fov_degrees: 92.5,
        view_distance: 5,
        scale: 1.25,
        fullscreen: true,
        bounced_gi: true,
        exposure: 1.25,
        bloom_strength: 0.0,
        selected_slot: 7,
        debug_hud: true,
        profile: 0x1234,
    };

    config.save(&path).unwrap();

    assert_eq!(Config::load(&path), config);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn invalid_values_are_clamped_and_corrupt_files_fall_back() {
    let directory = test_directory("config-invalid");
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("settings");
    fs::write(
        &path,
        "version=1\nsensitivity=NaN\nfov_degrees=500\nview_distance=255\nscale=-4\nfullscreen=true\nhotbar=0,2,99,1,2,3,1,2,3\nselected_slot=40\ndebug_hud=true\n",
    )
    .unwrap();

    let config = Config::load(&path);
    assert_eq!(config.sensitivity, Config::default().sensitivity);
    assert_eq!(config.fov_degrees, MAX_FOV);
    assert_eq!(config.view_distance, MAX_VIEW_DISTANCE);
    assert_eq!(config.scale, MIN_SCALE);
    assert_eq!(config.selected_slot, 8);
    assert!(config.fullscreen && config.debug_hud);
    assert!(!config.bounced_gi);

    fs::write(&path, "version=99\nfov_degrees=80\n").unwrap();
    assert_eq!(Config::load(&path), Config::default());
    assert_eq!(Config::load(directory.join("missing")), Config::default());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn saving_sanitizes_public_values() {
    let directory = test_directory("config-save-clamp");
    let path = directory.join("settings");
    let config = Config {
        sensitivity: f32::INFINITY,
        fov_degrees: -1.0,
        view_distance: 0,
        scale: 9.0,
        exposure: f32::NAN,
        bloom_strength: -1.0,
        selected_slot: usize::MAX,
        ..Config::default()
    };

    config.save(&path).unwrap();
    let loaded = Config::load(&path);
    assert_eq!(loaded.exposure, 1.0);
    assert_eq!(loaded.bloom_strength, 0.0);
    assert_eq!(loaded.sensitivity, Config::default().sensitivity);
    assert_eq!(loaded.fov_degrees, MIN_FOV);
    assert_eq!(loaded.view_distance, MIN_VIEW_DISTANCE);
    assert_eq!(loaded.scale, MAX_SCALE);
    assert_eq!(loaded.selected_slot, 8);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn profile_is_generated_once_and_survives_reload() {
    let directory = test_directory("profile");
    let path = directory.join("config");
    let mut config = Config::default();
    config.ensure_profile(&path).unwrap();
    assert_ne!(config.profile, 0);
    let profile = config.profile;
    let mut loaded = Config::load(&path);
    loaded.ensure_profile(&path).unwrap();
    assert_eq!(loaded.profile, profile);
    fs::remove_dir_all(directory).unwrap();
}
