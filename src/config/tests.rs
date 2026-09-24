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
        hotbar: [3, 2, 1, 3, 2, 1, 3, 2, 1],
        selected_slot: 7,
        debug_hud: true,
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
    assert_eq!(config.hotbar, [1, 2, 8, 1, 2, 3, 1, 2, 3]);
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
        hotbar: [0, 4, 2, 1, 1, 1, 1, 1, 1],
        selected_slot: usize::MAX,
        ..Config::default()
    };

    config.save(&path).unwrap();
    let loaded = Config::load(&path);
    assert_eq!(loaded.sensitivity, Config::default().sensitivity);
    assert_eq!(loaded.fov_degrees, MIN_FOV);
    assert_eq!(loaded.view_distance, MIN_VIEW_DISTANCE);
    assert_eq!(loaded.scale, MAX_SCALE);
    assert_eq!(loaded.hotbar[0..2], [1, 4]);
    assert_eq!(loaded.selected_slot, 8);
    fs::remove_dir_all(directory).unwrap();
}
