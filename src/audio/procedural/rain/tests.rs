use super::*;

#[test]
fn rain_bed_is_diffuse_even_when_nearby_impacts_favor_one_ear() {
    let mut left = Rain::new(23);
    let mut right = Rain::new(23);
    let weather = RainWeather {
        rain_mm_h: 35.0,
        ..RainWeather::default()
    };
    for band in 0..BED_BANDS {
        left.bed.power[0][band] = 0.04;
        left.bed.power[1][band] = 0.01;
        right.bed.power[0][band] = 0.01;
        right.bed.power[1][band] = 0.04;
    }
    left.follow(weather).unwrap();
    right.follow(weather).unwrap();
    assert!(left.bed.ratio > 0.0);
    assert_eq!(left.bed.gain[0], left.bed.gain[1]);
    assert_eq!(left.bed.gain, right.bed.gain);
    for band in 0..BED_BANDS {
        let energy = left.bed.gain[0][band].powi(2);
        let expected = left.bed.gain_scale[band] * 0.025;
        assert!((energy - expected).abs() <= expected * 1e-6);
    }
    let mut stereo_difference = 0.0;
    for _ in 0..4410 {
        let a = left.bed.next([0.0; 2]);
        let b = right.bed.next([0.0; 2]);
        assert_eq!(a, b);
        stereo_difference += (a[0] - a[1]).powi(2);
    }
    assert!(stereo_difference > 0.0);
}

fn drop(surface: usize) -> Droplet {
    Droplet {
        surface,
        radius_m: 0.0005,
        velocity_m_s: 4.0,
        bubble_radius_m: 0.0,
        distance_m: 1.0,
        angle_rad: PI * 0.5,
    }
}
#[test]
fn active_drop_turns_with_listener_without_restarting_its_tail() {
    let mut rain = Rain::new(23);
    let listener = Listener::default();
    rain.start_drop(drop(1), listener).unwrap();
    for _ in 0..8 {
        rain.next(listener);
    }
    let phase = rain.voices[0].modes[0];
    let energy = |mut spatial: Spatial| {
        let mut bus = Bus::default();
        let mut energy = [0.0; 2];
        for n in 0..256 {
            spatial.emit(listener, &mut bus, if n == 0 { 1.0 } else { 0.0 });
            let sample = bus.next();
            for ear in 0..2 {
                energy[ear] += sample[ear].powi(2);
            }
        }
        energy
    };
    let right_energy = energy(rain.voices[0].spatial);
    assert!(right_energy[1] > right_energy[0]);
    rain.set_listener([0.0; 3], PI);
    // Listener updates preserve the existing voice and oscillator state.
    assert_eq!(rain.stats.generated, 1);
    assert_eq!(rain.voices[0].modes[0].next(), {
        let mut expected = phase;
        expected.next()
    });
    rain.next(listener);
    let left_energy = energy(rain.voices[0].spatial);
    assert!(left_energy[0] > left_energy[1]);
    assert_eq!(rain.stats.generated, 1);
    assert_eq!(rain.active_voices(), 1);
}
#[test]
fn rain_bubble_matches_physical_frequency_damping_delay_and_gain() {
    let mut rain = Rain::new(1);
    let mut config = RainConfig::default();
    config.surfaces[0].click_gain = [1.0; 2];
    config.surfaces[0].bubble_gain = [2.0; 2];
    config.surfaces[0].bubble_decay = [1.0; 2];
    rain.configure(config).unwrap();
    let mut droplet = drop(0);
    droplet.bubble_radius_m = 0.0004;
    rain.start_drop(droplet, Listener::default()).unwrap();
    let mut bubble = rain.voices[0].modes[3];
    let delay = (0.002 * SAMPLE_RATE) as usize;
    let radius = 0.0004_f64;
    let frequency = (3.0_f64 * 1.4 * 101325.0 / 1000.0).sqrt() / (std::f64::consts::TAU * radius);
    let damping = 0.13 / radius + 0.0072 / (radius * radius.sqrt());
    assert!((frequency - 8208.11).abs() < 0.01);
    for n in 0..delay + 120 {
        let t = n.saturating_sub(delay) as f64 / f64::from(SAMPLE_RATE);
        let expected = if n < delay {
            0.0
        } else {
            0.00875 * (-damping * t).exp() * (std::f64::consts::TAU * frequency * t).sin()
        };
        assert!(
            (f64::from(bubble.next()) - expected).abs() < 2.5e-7,
            "bubble sample {n}"
        );
    }
}
#[test]
fn rain_dense_pool_rejects_overload_and_reclaims_every_tail() {
    let mut rain = Rain::new(42);
    let listener = Listener::default();
    for _ in 0..MAX_DROPS {
        rain.start_drop(drop(1), listener).unwrap();
    }
    assert!(rain.start_drop(drop(1), listener).is_err());
    assert_eq!(rain.stats().generated, 128);
    assert_eq!(rain.stats().dropped, 1);
    assert_eq!(rain.stats().peak_active, 128);
    let capacity = rain.voices.capacity();
    let mut energy = [0.0; 2];
    for _ in 0..44100 {
        let (sample, send) = rain.next(listener);
        assert!(send.is_finite());
        for ear in 0..2 {
            assert!(sample[ear].is_finite());
            energy[ear] += sample[ear] * sample[ear];
        }
    }
    assert_eq!(rain.active_voices(), 0);
    assert_eq!(rain.voices.capacity(), capacity);
    assert!(energy[0] > 0.0 && energy[1] > 0.0);
    assert!((energy[0] - energy[1]).abs() > 1e-6);
    for _ in 0..128 {
        assert_eq!(rain.next(listener).0, [0.0; 2]);
    }
}
#[test]
fn rain_size_flux_grows_and_wind_drives_vertical_surfaces() {
    let mut rain = Rain::new(7);
    rain.follow(RainWeather {
        rain_mm_h: 1.0,
        ..RainWeather::default()
    })
    .unwrap();
    let weak_flux = rain.flux;
    rain.follow(RainWeather {
        rain_mm_h: 25.0,
        ..RainWeather::default()
    })
    .unwrap();
    assert!(rain.flux > weak_flux);
    assert!(rain.size_cdf.windows(2).all(|pair| pair[0] <= pair[1]));
    assert_eq!(rain.size_cdf[49], 1.0);
    assert!(diameter(&rain.size_cdf, 0.99, 0.5) > diameter(&rain.size_cdf, 0.1, 0.5));
    let mut config = RainConfig::default();
    for surface in &mut config.surfaces {
        surface.vertical = true;
    }
    rain.configure(config).unwrap();
    rain.follow(RainWeather {
        rain_mm_h: 25.0,
        ..RainWeather::default()
    })
    .unwrap();
    assert_eq!(rain.played_per_s, 0.0);
    rain.follow(RainWeather {
        rain_mm_h: 25.0,
        wind_m_s: 5.0,
        wind_mean_m_s: 5.0,
        ..RainWeather::default()
    })
    .unwrap();
    assert!(rain.played_per_s > 0.0);
}
#[test]
fn rain_streams_are_deterministic_bed_matches_rain_and_dry_weather_drains() {
    let mut a = Rain::new(23);
    let mut b = Rain::new(23);
    let listener = Listener::default();
    let weather = RainWeather {
        rain_mm_h: 35.0,
        wind_m_s: 4.0,
        wind_mean_m_s: 4.0,
        ..RainWeather::default()
    };
    let mut bed_energy = 0.0;
    for n in 0..44100 {
        if n % 441 == 0 {
            a.follow(weather).unwrap();
            b.follow(weather).unwrap();
        }
        let x = a.next(listener);
        let y = b.next(listener);
        assert_eq!(x, y);
        assert!(x.0.iter().all(|v| v.is_finite()));
        bed_energy += a.bed.gain[0].iter().sum::<f32>();
    }
    assert!(a.stats().generated > 100);
    assert!(a.bed.ratio > 0.0 && bed_energy > 0.0);
    assert!(a.stats().peak_active <= MAX_DROPS);
    a.follow(RainWeather::default()).unwrap();
    for _ in 0..88200 {
        a.next(listener);
    }
    assert_eq!(a.active_voices(), 0);
    assert_eq!(a.next(listener).0, [0.0; 2]);
}
#[test]
fn rain_tiny_rates_and_extreme_finite_gusts_remain_finite() {
    let mut rain = Rain::new(91);
    let config = RainConfig {
        max_drops_per_s: 1e-20,
        ..RainConfig::default()
    };
    rain.configure(config).unwrap();
    let weather = RainWeather {
        rain_mm_h: 500.0,
        wind_m_s: 100.0,
        wind_mean_m_s: 1e-40,
        wind_bearing_rad: 0.0,
    };
    for n in 0..4410 {
        if n % 441 == 0 {
            rain.follow(weather).unwrap();
        }
        let (direct, send) = rain.next(Listener::default());
        assert!(direct.iter().all(|v| v.is_finite()) && send.is_finite());
    }
    assert!(rain.bed.ratio.is_finite() && rain.arrival_probability.is_finite());
    rain.follow(RainWeather {
        rain_mm_h: 1e-30,
        ..RainWeather::default()
    })
    .unwrap();
    assert_eq!(rain.flux, 0.0);
}
#[test]
fn rain_invalid_inputs_reject_without_mutating_configuration() {
    let mut rain = Rain::new(3);
    let original = rain.config.gain;
    let mut invalid = RainConfig::default();
    invalid.surfaces[0].bubble_decay[0] = f32::NAN;
    assert!(rain.configure(invalid).is_err());
    assert_eq!(rain.config.gain, original);
    assert!(
        rain.follow(RainWeather {
            rain_mm_h: f32::NAN,
            ..RainWeather::default()
        })
        .is_err()
    );
    let mut invalid = drop(0);
    invalid.surface = RAIN_MATERIALS;
    assert!(rain.start_drop(invalid, Listener::default()).is_err());
    assert_eq!(rain.stats().generated, 0);
    for (index, expected) in [
        "Water",
        "Dirt",
        "Leaf",
        "Concrete",
        "Glass",
        "Metal",
        "Plastic",
        "Asphalt",
        "Asphalt roof",
    ]
    .iter()
    .enumerate()
    {
        assert_eq!(rain.config.surfaces[index].name, *expected);
    }
}

#[test]
fn drop_volume_and_reverb_are_independent_of_the_diffuse_bed() {
    let listener = Listener::default();
    let setup = |drop_gain, reverb_gain, bed_gain| {
        let mut rain = Rain::new(37);
        rain.configure(RainConfig {
            drop_gain,
            reverb_gain,
            bed_gain,
            ..Default::default()
        })
        .unwrap();
        rain.start_drop(drop(1), listener).unwrap();
        rain.bed.ratio = 1.0;
        rain.bed.gain = [[0.001; BED_BANDS]; 2];
        rain
    };
    let mut dry = setup(1.0, 1.0, 0.0);
    let mut louder = setup(2.0, 0.5, 0.0);
    let mut bed_only = setup(0.0, 1.0, 0.25);
    let mut combined = setup(1.0, 1.0, 0.25);
    let mut bed_energy = 0.0;
    let mut drop_energy = 0.0;
    for _ in 0..4096 {
        let (a, send_a) = dry.next(listener);
        let (b, send_b) = louder.next(listener);
        let (bed, bed_send) = bed_only.next(listener);
        let (mix, _) = combined.next(listener);
        assert!((send_a - send_b).abs() < 1e-7);
        assert_eq!(bed_send, 0.0);
        for ear in 0..2 {
            assert!((b[ear] - 2.0 * a[ear]).abs() < 1e-7);
            assert!((mix[ear] - bed[ear] - a[ear]).abs() < 1e-7);
            bed_energy += bed[ear].powi(2);
            drop_energy += a[ear].powi(2);
        }
    }
    assert!(bed_energy > 0.0);
    assert!(drop_energy > 0.0);
}

#[test]
fn changing_spatial_hearing_retargets_sounding_drops_without_respawning() {
    let mut rain = Rain::new(23);
    rain.start_drop(drop(1), Listener::default()).unwrap();
    for _ in 0..8 {
        rain.next(Listener::default());
    }
    let centered = Listener {
        width_m: 0.0,
        head_amount: 0.0,
        rear_amount: 0.0,
    };
    rain.next(centered);
    assert_eq!(rain.stats.generated, 1);
    assert_eq!(rain.voices[0].listener, centered);
    let mut spatial = rain.voices[0].spatial;
    let mut bus = Bus::default();
    let mut energy = [0.0; 2];
    for n in 0..256 {
        spatial.emit(centered, &mut bus, if n == 0 { 1.0 } else { 0.0 });
        let sample = bus.next();
        for ear in 0..2 {
            energy[ear] += sample[ear].powi(2);
        }
    }
    assert_eq!(energy[0], energy[1]);
    assert!(energy[0] > 0.0);
}
