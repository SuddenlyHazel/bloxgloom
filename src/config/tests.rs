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
        lod_horizon: 1024,
        lod_quality: 2,
        scale: 1.25,
        fullscreen: true,
        bounced_gi: true,
        sun_shadow_quality: SunShadowQuality::High,
        parallax: parallax::Parallax {
            enabled: false,
            depth: 0.085,
            distance: 64.0,
            steps: 48,
        },
        exposure: 1.25,
        post_processing: false,
        bloom_enabled: false,
        bloom_strength: 0.0,
        audio_master: 0.4,
        audio_ambient: 0.2,
        audio_effects: 0.9,
        rain_audio: Default::default(),
        selected_slot: 7,
        debug_hud: true,
        profile: 0x1234,
        bindings: Bindings {
            drop: winit::keyboard::KeyCode::KeyT,
            ..Bindings::default()
        },
        named_bindings: NamedBindings::default(),
    };

    config.save(&path).unwrap();

    assert_eq!(Config::load(&path), config);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn parallax_settings_validate_old_files_bad_values_and_shader_bounds() {
    assert_eq!(
        parse_config("version=1\n").parallax,
        parallax::Parallax::default()
    );
    let settings = parse_config(
        "version=1\nparallax_enabled=false\nparallax_depth=99\nparallax_distance=1\nparallax_steps=999\n",
    ).parallax;
    assert!(!settings.enabled);
    assert_eq!(settings.depth, 0.15);
    assert_eq!(settings.distance, 8.0);
    assert_eq!(settings.steps, 64);
    assert_eq!(settings.uniform(), [0.0, 8.0, 64.0, 0.0]);
    let invalid =
        parse_config("version=1\nparallax_depth=NaN\nparallax_distance=inf\nparallax_steps=bad\n");
    assert_eq!(invalid.parallax, parallax::Parallax::default());
}

#[test]
fn sun_shadow_quality_defaults_parses_and_round_trips_every_level() {
    assert_eq!(SunShadowQuality::default(), SunShadowQuality::Medium);
    for source in [
        "version=1\nfov_degrees=92\naudio_master=0.4\n",
        "version=1\nfov_degrees=92\naudio_master=0.4\nsun_shadow_quality=ultra\n",
        "version=1\nfov_degrees=92\naudio_master=0.4\nsun_shadow_quality=\n",
    ] {
        let config = parse_config(source);
        assert_eq!(config.sun_shadow_quality, SunShadowQuality::Medium);
        assert_eq!(config.fov_degrees, 92.0);
        assert_eq!(config.audio_master, 0.4);
    }
    let directory = test_directory("sun-shadow-quality");
    let path = directory.join("config");
    for (quality, stored, label) in [
        (SunShadowQuality::Off, "off", "Off"),
        (SunShadowQuality::Low, "low", "Low"),
        (SunShadowQuality::Medium, "medium", "Medium"),
        (SunShadowQuality::High, "high", "High"),
    ] {
        assert_eq!(SunShadowQuality::parse(stored), Some(quality));
        assert_eq!(quality.as_str(), stored);
        assert_eq!(quality.label(), label);
        let config = Config {
            sun_shadow_quality: quality,
            ..Config::default()
        };
        config.save(&path).unwrap();
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains(&format!("sun_shadow_quality={stored}\n"))
        );
        assert_eq!(Config::load(&path), config);
    }
    assert_eq!(SunShadowQuality::parse("High"), None);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn sun_shadow_quality_cycles_forward_and_backward_including_off() {
    let levels = [
        SunShadowQuality::Off,
        SunShadowQuality::Low,
        SunShadowQuality::Medium,
        SunShadowQuality::High,
    ];
    for (index, quality) in levels.iter().enumerate() {
        assert_eq!(quality.cycle(true), levels[(index + 1) % levels.len()]);
        assert_eq!(quality.cycle(false), levels[(index + 3) % levels.len()]);
        assert_eq!(quality.cycle(true).cycle(false), *quality);
    }
}

#[test]
fn named_shortcuts_round_trip_but_never_bind_movement_or_builtin_keys() {
    use winit::keyboard::KeyCode;
    let mut config = Config::default();
    assert!(
        config
            .named_bindings
            .bind("demo:wave", KeyCode::KeyT, config.bindings)
    );
    assert!(
        !config
            .named_bindings
            .bind("demo:other", KeyCode::KeyT, config.bindings)
    );
    assert!(
        !config
            .named_bindings
            .bind("demo:walk", KeyCode::KeyW, config.bindings)
    );
    assert!(
        !config
            .named_bindings
            .bind("demo:inventory", KeyCode::KeyE, config.bindings)
    );
    assert_eq!(
        config.named_bindings.action(KeyCode::KeyT),
        Some("demo:wave")
    );
    let loaded = parse_config(&config.serialize());
    assert_eq!(loaded.named_bindings, config.named_bindings);
    let directory = test_directory("named-shortcut");
    let path = directory.join("config");
    config.save(&path).unwrap();
    assert_eq!(Config::load(&path).named_bindings, config.named_bindings);
    fs::remove_dir_all(directory).unwrap();
    let invalid = parse_config(
        "version=1\nbind_action.demo:wave=W\nbind_action.demo:other=E\nbind_action.fake=Z\n",
    );
    assert_eq!(invalid.named_bindings, NamedBindings::default());
}

#[test]
fn binding_conflicts_and_movement_keys_fail_back_to_defaults() {
    let mut config = parse_config("version=1\nbind_drop=R\n");
    assert_eq!(config.bindings, Bindings::default());
    config = parse_config("version=1\nbind_drop=W\n");
    assert_eq!(config.bindings, Bindings::default());
    config = parse_config("version=1\nbind_drop=T\n");
    assert_eq!(config.bindings.drop, winit::keyboard::KeyCode::KeyT);
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

#[test]
fn obsolete_classic_setting_is_ignored_without_losing_other_preferences() {
    let config =
        parse_config("version=1\nauthored_characters=false\nfov_degrees=92\nprofile=1234\n");
    assert_eq!(config.fov_degrees, 92.0);
    assert_eq!(config.profile, 0x1234);
    assert!(!config.serialize().contains("authored_characters"));
    assert_eq!(parse_config(&config.serialize()), config);
}

#[test]
fn audio_volumes_default_sanitize_and_round_trip_without_a_preview_preset() {
    let old = parse_config("version=1\nsensitivity=0.003\n");
    assert_eq!(
        (old.audio_master, old.audio_ambient, old.audio_effects),
        (1.0, 1.0, 1.0)
    );
    let bounded = parse_config(
        "version=1\naudio_master=NaN\naudio_ambient=-2\naudio_effects=9\naudio_preset=storm\n",
    );
    assert_eq!(
        (
            bounded.audio_master,
            bounded.audio_ambient,
            bounded.audio_effects
        ),
        (1.0, 0.0, 1.0)
    );
    assert!(!bounded.serialize().contains("audio_preset"));
    let mut direct = Config {
        audio_master: f32::INFINITY,
        audio_ambient: f32::NAN,
        audio_effects: -0.1,
        ..Config::default()
    };
    direct.sanitize();
    assert_eq!(
        (
            direct.audio_master,
            direct.audio_ambient,
            direct.audio_effects
        ),
        (1.0, 1.0, 0.0)
    );
    assert_eq!(parse_config(&bounded.serialize()), bounded);
}

#[test]
fn lod_settings_reject_unbounded_work_and_preserve_disable() {
    let off = parse_config("version=1\nlod_horizon=0\nlod_quality=255\n");
    assert_eq!(off.lod_horizon, 0);
    assert_eq!(off.lod_quality, 2);
    assert_eq!(
        parse_config("version=1\nlod_horizon=65535\n").lod_horizon,
        1024
    );
}

#[test]
fn live_rain_tuning_persists_and_old_configs_keep_native_defaults() {
    let directory = test_directory("rain-tuning");
    let path = directory.join("config");
    let mut config = Config::default();
    config.rain_audio.bed_gain = 0.03;
    config.rain_audio.drop_gain = 1.5;
    config.rain_audio.surfaces[9].lowpass_hz = 2300.0;
    config.rain_audio.advanced.wind.brightness = 2.0;
    config.rain_audio.advanced.cicadas.species = crate::audio::rain_tuning::CicadaSpecies::Pharaoh;
    config.rain_audio.advanced.preview.manual = true;
    config.save(&path).unwrap();
    assert_eq!(Config::load(&path), config);
    assert_eq!(
        parse_config("version=1\naudio_ambient=0.4\n").rain_audio,
        Default::default()
    );
    assert_eq!(
        parse_config("version=1\nrain_audio={broken}\n").rain_audio,
        Default::default()
    );
    fs::remove_dir_all(directory).unwrap();
}
