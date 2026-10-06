use super::*;

#[test]
fn old_settings_are_preserved_and_preset_bundles_round_trip() {
    let old = crate::config::parse_config(
        "version=1\nview_distance=6\nsun_shadow_quality=high\nparallax_steps=60\nparallax_distance=80\nscale=1.5\nprofile=abc\n",
    );
    assert_eq!(old.quality_preset, QualityPreset::Custom);
    assert_eq!(old.render_scale, 1.0);
    assert!(old.reflections_enabled);
    assert_eq!(old.parallax.steps, 60);
    assert_eq!(old.parallax.distance, 80.0);
    for preset in [
        QualityPreset::Performance,
        QualityPreset::Balanced,
        QualityPreset::Quality,
    ] {
        let mut config = old.clone();
        config.exposure = 1.8;
        config.parallax.depth = 0.075;
        config.apply_quality_preset(preset);
        config.sanitize();
        assert_eq!(config.effective_quality_preset(), preset);
        assert_eq!(config.profile, 0xabc);
        assert_eq!(config.scale, 1.5);
        assert_eq!(config.exposure, 1.8);
        assert_eq!(config.parallax.depth, 0.075);
        assert_eq!(config.lighting, old.lighting);
        assert!(!config.bounced_gi);
        assert_eq!(crate::config::parse_config(&config.serialize()), config);
    }
}

#[test]
fn manual_changes_and_contradictory_files_cannot_claim_a_preset() {
    let mut config = Config::default();
    config.apply_quality_preset(QualityPreset::Balanced);
    let serialized = config.serialize();
    let contradictory = serialized.replace("render_scale=0.5", "render_scale=0.35");
    assert_eq!(
        crate::config::parse_config(&contradictory).quality_preset,
        QualityPreset::Custom
    );
    config.parallax.steps = 60;
    config.sanitize();
    assert_eq!(config.quality_preset, QualityPreset::Custom);
    assert_eq!(config.parallax.steps, 60);
    config.apply_quality_preset(QualityPreset::Quality);
    config.exposure = 2.0;
    config.sanitize();
    assert_eq!(config.quality_preset, QualityPreset::Quality);
    assert_eq!(config.exposure, 2.0);
}

#[test]
fn render_scale_is_bounded_independently_of_ui_scale() {
    for (source, expected) in [
        ("0.1", MIN_RENDER_SCALE),
        ("2", 1.0),
        ("NaN", 1.0),
        ("inf", 1.0),
        ("0.67", 0.67),
    ] {
        let config =
            crate::config::parse_config(&format!("version=1\nrender_scale={source}\nscale=1.5\n"));
        assert_eq!(config.render_scale, expected);
        assert_eq!(config.scale, 1.5);
    }
}

#[test]
fn laptop_and_balanced_budgets_reduce_pixels_and_show_distinct_names() {
    let mut laptop = Config::default();
    laptop.apply_quality_preset(QualityPreset::Performance);
    let mut balanced = Config::default();
    balanced.apply_quality_preset(QualityPreset::Balanced);
    let mut quality = Config::default();
    quality.apply_quality_preset(QualityPreset::Quality);
    assert!(laptop.render_scale * laptop.render_scale < 0.15);
    assert!(balanced.render_scale > laptop.render_scale);
    assert!(quality.render_scale > balanced.render_scale);
    assert_eq!(quality.render_scale, 1.0);
    assert_eq!(QualityPreset::Performance.label(), "MacBook");
    assert_ne!(
        QualityPreset::Performance.label(),
        QualityPreset::Balanced.label()
    );
    laptop.render_scale = 0.0;
    laptop.sanitize();
    assert_eq!(laptop.render_scale, MIN_RENDER_SCALE);
    assert_eq!(
        laptop.effective_quality_preset(),
        QualityPreset::Performance
    );
}
