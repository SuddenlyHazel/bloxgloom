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
    engine.set_world(Some(WeatherSound {
        rain_mm_h: 30.0,
        wind_m_s: 12.0,
        bearing: 0.7,
        exposure: 1.0,
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
    };
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
    }));
    assert_eq!(indoor.world, Some(WeatherSound::default()));
}
