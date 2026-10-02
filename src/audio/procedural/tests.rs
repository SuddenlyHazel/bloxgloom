use super::*;
#[test]
fn low_lightning_rates_do_not_acquire_a_24_bit_probability_floor() {
    assert!(!lightning_hit(0, 0.0));
    assert!(!lightning_hit(0, 0.0001));
    assert!(lightning_hit(0, 0.001));
    assert!(!lightning_hit(1, 0.001));
    assert!(!lightning_hit(u32::MAX, 30.0));
}
#[test]
fn automatic_lightning_skips_distant_cells_without_admitting_a_voice() {
    let mut engine = Procedural::new(3);
    engine.weather = Weather {
        lightning: 1.0,
        distance: 1_000_000.0,
        ..Weather::default()
    };
    engine.frame = 1;
    engine.next(Preset::Storm);
    assert_eq!(engine.thunder.active_voices(), 0);
    assert_eq!(engine.thunder.rejected(), 0);
}

#[test]
fn world_weather_uses_continuous_inputs_without_autonomous_lightning() {
    let mut engine = Procedural::new(7);
    engine.set_scene(crate::audio::rain_scene::RainScene::patch(
        crate::audio::rain_scene::RainMaterial::Dirt,
    ));
    engine.set_world(Some(WeatherSound {
        rain_mm_h: 30.0,
        wind_m_s: 12.0,
        bearing: 0.7,
        exposure: 1.0,
        daylight: 0.0,
    }));
    // Even a preview Storm preset cannot schedule strikes in explicit world mode.
    for _ in 0..44_100 {
        let (ambient, thunder) = engine.next(Preset::Storm);
        assert!(ambient.into_iter().all(f32::is_finite));
        assert_eq!(thunder, [0.0; 2]);
    }
    assert!(engine.weather.rain > 25.0);
    assert_eq!(engine.weather.lightning, 0.0);
    assert_eq!(engine.thunder.active_voices(), 0);
    assert!(engine.trigger_world_thunder(400.0, 0.8, 0.0));
    assert_eq!(engine.thunder.active_voices(), 1);
}

#[test]
fn sheltered_world_weather_is_quieter_and_inputs_are_bounded() {
    let mut outdoor = Procedural::new(9);
    let mut indoor = Procedural::new(9);
    let target = WeatherSound {
        rain_mm_h: 20.0,
        wind_m_s: 8.0,
        bearing: 0.0,
        exposure: 1.0,
        daylight: 0.0,
    };
    outdoor.set_scene(crate::audio::rain_scene::RainScene::patch(
        crate::audio::rain_scene::RainMaterial::Dirt,
    ));
    indoor.set_scene(crate::audio::rain_scene::RainScene::patch(
        crate::audio::rain_scene::RainMaterial::Dirt,
    ));
    outdoor.set_world(Some(target));
    indoor.set_world(Some(WeatherSound {
        exposure: 0.0,
        ..target
    }));
    let mut energy = [0.0_f64; 2];
    for frame in 0..88_200 {
        let a = outdoor.next(Preset::Off).0;
        let b = indoor.next(Preset::Off).0;
        if frame >= 44_100 {
            energy[0] += a.into_iter().map(|x| f64::from(x).powi(2)).sum::<f64>();
            energy[1] += b.into_iter().map(|x| f64::from(x).powi(2)).sum::<f64>();
        }
    }
    assert!(energy[0] > 0.001);
    assert!(energy[1] < energy[0] * 0.1);
    indoor.set_world(Some(WeatherSound {
        rain_mm_h: f32::NAN,
        wind_m_s: f32::INFINITY,
        bearing: f32::NEG_INFINITY,
        exposure: f32::NAN,
        daylight: 0.0,
    }));
    assert_eq!(indoor.world, Some(WeatherSound::default()));
}

#[test]
fn material_scenes_produce_distinct_spectra_and_empty_geometry_has_no_fake_rain() {
    use crate::audio::rain_scene::{RainMaterial, RainScene};
    let render = |material| {
        let mut engine = Procedural::new(13);
        engine.set_listener([0.0, 1.6, 0.0], 0.0);
        if let Some(material) = material {
            engine.set_scene(RainScene::patch(material));
        }
        engine.set_world(Some(WeatherSound {
            rain_mm_h: 30.0,
            exposure: 1.0,
            daylight: 0.0,
            ..WeatherSound::default()
        }));
        let mut energy = 0.0_f64;
        let mut difference = 0.0_f64;
        let mut previous = [0.0; 2];
        for i in 0..44_100 {
            let (sample, _) = engine.next(Preset::Off);
            assert!(sample.into_iter().all(f32::is_finite));
            if i >= 22_050 {
                for ear in 0..2 {
                    energy += f64::from(sample[ear]).powi(2);
                    difference += f64::from(sample[ear] - previous[ear]).powi(2);
                }
            }
            previous = sample;
        }
        (
            energy,
            difference / energy.max(1e-20),
            engine.rain.stats().generated,
        )
    };
    let wood = render(Some(RainMaterial::Wood));
    let leaf = render(Some(RainMaterial::Leaf));
    let water = render(Some(RainMaterial::Water));
    assert!(wood.0 > 0.001 && leaf.0 > 0.001 && water.0 > 0.001);
    assert!(
        wood.1 < leaf.1,
        "wood should have less high-frequency energy: {wood:?} {leaf:?}"
    );
    assert!(
        (water.1 - leaf.1).abs() > 0.01,
        "bubble and leaf spectra should differ"
    );
    assert_eq!(render(None), (0.0, 0.0, 0));
}

#[test]
fn spatial_custom_profiles_change_actual_impacts_and_zero_gain_silences_them() {
    use crate::audio::rain_scene::{ImpactProfile, RainMaterial, RainScene};
    let render = |gain| {
        let mut scene = RainScene::patch(RainMaterial::Metal);
        for tile in &mut Arc::make_mut(&mut scene).tiles {
            tile.impact = Some(ImpactProfile {
                gain,
                click: 0.55,
                frequency_hz: [450., 1100.],
                damping_per_s: [180., 350.],
                resonance: 0.65,
                lowpass_hz: 5000.,
            });
        }
        let mut engine = Procedural::new(13);
        engine.set_listener([0., 1.6, 0.], 0.);
        engine.set_scene(scene);
        engine.set_world(Some(WeatherSound {
            rain_mm_h: 30.,
            exposure: 1.,
            ..Default::default()
        }));
        let mut out = Vec::with_capacity(22_050);
        for _ in 0..22_050 {
            let (sample, _) = engine.next(Preset::Off);
            assert!(sample.into_iter().all(f32::is_finite));
            out.push(sample);
        }
        assert!(engine.rain.stats().generated > 0);
        out
    };
    let audible = render(0.8);
    assert!(audible.iter().flatten().any(|v| v.abs() > 0.001));
    assert_eq!(
        audible,
        render(0.8),
        "custom impacts lost deterministic synthesis"
    );
    assert!(
        render(0.).iter().flatten().all(|v| *v == 0.),
        "tile custom gain was ignored in favor of preset metal"
    );
}

#[test]
fn changing_preview_keeps_live_rain_profile_and_world_listener() {
    let mut engine = Procedural::new(17);
    let profile = crate::audio::rain_tuning::RainConfig {
        bed_gain: 0.02,
        drop_gain: 1.5,
        ..Default::default()
    };
    engine.set_rain_config(profile);
    engine.set_listener([4.0, 8.0, 12.0], 1.2);
    for preset in [Preset::Rain, Preset::Storm, Preset::Off, Preset::Wind] {
        engine.set_preset(preset);
        assert_eq!(engine.rain_config, profile);
        assert_eq!(engine.listener_position, [4.0, 8.0, 12.0]);
        assert_eq!(engine.listener_yaw, 1.2);
        for _ in 0..1024 {
            assert!(engine.next(preset).0.into_iter().all(f32::is_finite));
        }
    }
}

#[test]
fn insect_volume_mutes_active_calls_and_their_own_reflections() {
    for (material, daylight) in [
        (crate::audio::rain_scene::RainMaterial::Dirt, 0.0),
        (crate::audio::rain_scene::RainMaterial::Leaf, 1.0),
    ] {
        let mut engine = Procedural::new(1);
        engine.set_scene(crate::audio::rain_scene::RainScene::patch(material));
        engine.set_listener([0.0, 1.6, 0.0], 0.0);
        engine.set_world(Some(WeatherSound {
            exposure: 1.0,
            daylight,
            ..Default::default()
        }));
        let mut config = crate::audio::rain_tuning::RainConfig {
            gain: 0.0,
            wind_gain: 0.0,
            ..Default::default()
        };
        engine.set_rain_config(config);
        let mut energy = 0.0;
        for _ in 0..3 * 44_100 {
            energy += engine
                .next(Preset::Off)
                .0
                .into_iter()
                .map(|s| s * s)
                .sum::<f32>();
        }
        assert!(energy > 1e-4);
        config.insect_gain = 0.0;
        engine.set_rain_config(config);
        for _ in 0..4410 {
            assert_eq!(engine.next(Preset::Off).0, [0.0; 2]);
        }
    }
}

#[test]
fn local_weather_lab_cannot_override_world_weather_or_schedule_world_lightning() {
    let mut engine = Procedural::new(7);
    let mut config = crate::audio::rain_tuning::RainConfig::default();
    config.advanced.preview.manual = true;
    config.advanced.preview.fixed.rain_mm_h = 150.0;
    config.advanced.preview.fixed.wind_m_s = 35.0;
    config.advanced.preview.fixed.lightning_per_min = 30.0;
    engine.set_rain_config(config);
    engine.set_world(Some(WeatherSound {
        rain_mm_h: 0.0,
        wind_m_s: 0.0,
        ..Default::default()
    }));
    for _ in 0..1000 {
        assert_eq!(engine.next(Preset::Storm).1, [0.0; 2]);
    }
    assert_eq!(engine.weather.rain, 0.0);
    assert_eq!(engine.weather.wind, 0.0);
    assert_eq!(engine.weather.lightning, 0.0);
    engine.set_world(None);
    engine.frame = 0;
    engine.next(Preset::Rain);
    assert_eq!(engine.weather.rain, 150.0);
    assert!(engine.weather.wind > 0.0);
}

#[test]
fn thunder_volume_mutes_existing_voices_and_returns() {
    let mut engine = Procedural::new(27);
    assert!(engine.trigger_thunder(200.0, 0.7));
    let mut audible = false;
    for _ in 0..44100 {
        audible |= engine.next(Preset::Off).1.iter().any(|v| v.abs() > 0.0001);
    }
    assert!(audible);
    let mut config = engine.rain_config;
    config.advanced.thunder.gain = 0.0;
    engine.set_rain_config(config);
    for _ in 0..44100 {
        assert_eq!(engine.next(Preset::Off).1, [0.0; 2]);
    }
    config.advanced.thunder.gain = 1.0;
    config.advanced.thunder.reverb_gain = 0.0;
    engine.set_rain_config(config);
    assert!(engine.next(Preset::Off).1.iter().all(|v| v.is_finite()));
}
