use super::*;
#[test]
fn mixer_is_buffer_partition_independent_and_limits_overload() {
    let clip = std::sync::Arc::new(Clip::click());
    let mut a = Mixer::new(4);
    let mut b = Mixer::new(4);
    for m in [&mut a, &mut b] {
        m.set_controls(Controls {
            master: 1.0,
            effects: 1.0,
            ..Controls::default()
        });
        for id in 1..=32 {
            assert!(m.command(Command::Play {
                bus: bloxgloom_host_api::sound::Bus::Effects,
                clip: clip.clone(),
                position: None,
                gain: 4.0,
                pitch: 1.0,
                looping: true,
                id
            }));
        }
        assert!(!m.command(Command::Play {
            bus: bloxgloom_host_api::sound::Bus::Effects,
            clip: clip.clone(),
            position: None,
            gain: 1.0,
            pitch: 1.0,
            looping: false,
            id: 33
        }));
    }
    let mut whole = vec![[0.0; 2]; 10_000];
    a.render(&mut whole);
    let mut split = vec![[0.0; 2]; 10_000];
    for block in split.chunks_mut(137) {
        b.render(block);
    }
    assert_eq!(whole, split);
    assert!(
        whole
            .iter()
            .flatten()
            .all(|v| v.is_finite() && v.abs() <= 0.98)
    );
    assert!(whole.iter().flatten().any(|v| v.abs() > 0.1));
    a.command(Command::Reset);
    let mut after = [[1.0; 2]; 512];
    a.render(&mut after);
    assert!(after.into_iter().flatten().all(|x| x == 0.0));
}
#[test]
fn positioned_clip_follows_listener_and_stop_releases_voice() {
    let clip = std::sync::Arc::new(Clip::click());
    let mut m = Mixer::new(1);
    m.set_controls(Controls {
        master: 1.0,
        effects: 1.0,
        ..Controls::default()
    });
    assert!(m.command(Command::Play {
        bus: bloxgloom_host_api::sound::Bus::Effects,
        clip,
        position: Some([0.0, 0.0, 1.0]),
        gain: 1.0,
        pitch: 1.0,
        looping: true,
        id: 1
    }));
    let mut frames = vec![[0.0; 2]; 5000];
    m.render(&mut frames);
    let energy = |ear| frames.iter().map(|f| f[ear] * f[ear]).sum::<f32>();
    assert!(energy(1) > energy(0) * 10.0);
    assert!(m.command(Command::Listener {
        position: [0.0; 3],
        yaw: std::f32::consts::PI
    }));
    m.render(&mut frames);
    let energy = |ear| {
        frames
            .iter()
            .skip(2000)
            .map(|f| f[ear] * f[ear])
            .sum::<f32>()
    };
    assert!(energy(0) > energy(1) * 10.0);
    m.command(Command::Stop(1));
    m.render(&mut frames);
    assert!(frames.iter().skip(1010).flatten().all(|x| *x == 0.0));
}

#[test]
fn wav_decode_preserves_mono_rate_and_rejects_nonfinite_audio() {
    let root = std::env::temp_dir().join(format!(
        "bloxgloom-audio-wave-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("mono.wav");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&path, spec).unwrap();
    for sample in [0i16, 16384, -16384] {
        writer.write_sample(sample).unwrap();
    }
    writer.finalize().unwrap();
    let clip = Clip::load(&path).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let prepared = Clip::decode(&bytes).unwrap();
    assert_eq!(prepared.frames, clip.frames);
    assert!(Clip::decode(&bytes[..bytes.len() - 1]).is_err());
    assert_eq!(clip.rate, 48_000);
    assert_eq!(clip.frames, [[0.0; 2], [0.5; 2], [-0.5; 2]]);
    let bad = root.join("nan.wav");
    let spec = hound::WavSpec {
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
        ..spec
    };
    let mut writer = hound::WavWriter::create(&bad, spec).unwrap();
    writer.write_sample(f32::NAN).unwrap();
    writer.finalize().unwrap();
    assert!(Clip::load(&bad).is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn manual_thunder_is_audible_without_weather_and_reset_clears_it() {
    let mut mixer = Mixer::new(2);
    mixer.set_controls(Controls {
        master: 1.0,
        effects: 1.0,
        ..Controls::default()
    });
    assert!(mixer.command(Command::Thunder {
        distance: 400.0,
        angle: 1.0
    }));
    let mut frames = vec![[0.0; 2]; 44_100];
    mixer.render(&mut frames);
    assert!(frames.iter().flatten().any(|v| v.abs() > 0.01));
    mixer.command(Command::Reset);
    mixer.render(&mut frames);
    assert!(frames.iter().flatten().all(|v| *v == 0.0));
}

#[test]
fn procedural_weather_mixer_is_partition_independent() {
    let controls = Controls {
        preset: Preset::Rain,
        ..Controls::default()
    };
    let mut a = Mixer::new(8);
    let mut b = Mixer::new(8);
    a.set_controls(controls);
    b.set_controls(controls);
    let mut whole = vec![[0.0; 2]; 22_050];
    let mut split = vec![[0.0; 2]; 22_050];
    a.render(&mut whole);
    for block in split.chunks_mut(317) {
        b.render(block);
    }
    assert_eq!(whole, split);
    assert!(whole.iter().flatten().any(|x| x.abs() > 0.001));
    assert!(
        whole
            .iter()
            .flatten()
            .all(|x| x.is_finite() && x.abs() <= 0.98)
    );
}

#[test]
fn world_weather_mixer_is_partition_independent_and_reset_silences_it() {
    let mut a = Mixer::new(12);
    let mut b = Mixer::new(12);
    for mixer in [&mut a, &mut b] {
        mixer.command(Command::RainScene(rain_scene::RainScene::patch(
            rain_scene::RainMaterial::Dirt,
        )));
        assert!(mixer.command(Command::Weather(Some(WeatherSound {
            rain_mm_h: 20.0,
            wind_m_s: 8.0,
            bearing: 0.7,
            exposure: 1.0,
            daylight: 0.0,
        }))));
    }
    let mut whole = vec![[0.0; 2]; 22_050];
    let mut split = vec![[0.0; 2]; 22_050];
    a.render(&mut whole);
    for block in split.chunks_mut(317) {
        b.render(block);
    }
    assert_eq!(whole, split);
    assert!(whole.iter().flatten().any(|x| x.abs() > 0.001));
    assert_eq!(a.diagnostics().3, 0);
    assert!(a.command(Command::WorldThunder {
        distance: 400.0,
        angle: 0.5,
        exposure: 0.0
    }));
    a.render(&mut whole);
    assert!(whole.iter().flatten().any(|x| x.abs() > 0.001));
    a.command(Command::Reset);
    a.render(&mut whole);
    assert!(whole.iter().flatten().all(|x| *x == 0.0));
}

#[test]
fn zero_rain_settings_leave_wind_audible_but_ambient_mute_silences_the_full_mixer() {
    use rain_tuning::RainConfig;
    let zero_rain = RainConfig {
        gain: 0.0,
        bed_gain: 0.0,
        drop_gain: 0.0,
        reverb_gain: 0.0,
        max_drops_per_s: 0.0,
        min_distance_m: 0.25,
        max_distance_m: 0.25,
        sheet_depth: 0.0,
        wind_gain: 1.0,
        ..Default::default()
    };
    assert_eq!(
        zero_rain.sanitized(),
        zero_rain,
        "user's zero settings must not reset to defaults"
    );
    for world_weather in [
        None,
        Some(WeatherSound {
            rain_mm_h: 35.0,
            wind_m_s: 12.0,
            exposure: 0.4,
            ..Default::default()
        }),
    ] {
        let mut mixer = Mixer::new(23);
        mixer.set_controls(Controls {
            master: 1.0,
            ambient: 1.0,
            effects: 1.0,
            preset: Preset::Rain,
        });
        if let Some(weather) = world_weather {
            assert!(mixer.command(Command::Weather(Some(weather))));
            assert!(
                mixer.command(Command::RainScene(rain_scene::RainScene::patch(
                    rain_scene::RainMaterial::Leaf
                )))
            );
        }
        mixer.set_rain_config(zero_rain);
        let mut samples = vec![[0.0; 2]; SAMPLE_RATE as usize];
        mixer.render(&mut samples);
        let rms = (samples
            .iter()
            .flatten()
            .map(|v| f64::from(*v).powi(2))
            .sum::<f64>()
            / samples.len() as f64
            / 2.0)
            .sqrt();
        assert!(
            rms > 0.001,
            "wind should reproduce the reported residual noise: {rms}"
        );
        let mut muted = zero_rain;
        muted.mute_ambient();
        mixer.set_rain_config(muted);
        mixer.render(&mut samples);
        assert!(
            samples.iter().skip(4096).flatten().all(|v| v.abs() < 1e-7),
            "all ambient layers must mute including shelter filters and limiter delay"
        );
        assert!(mixer.command(Command::Click(1)));
        mixer.render(&mut samples);
        assert!(
            samples.iter().flatten().any(|v| v.abs() > 0.001),
            "ambient mute must preserve gameplay effects"
        );
    }
}

#[test]
fn muting_active_rain_clears_its_reverb_return_even_in_world_weather() {
    let mut mixer = Mixer::new(13);
    mixer.set_controls(Controls {
        master: 1.0,
        ambient: 1.0,
        effects: 1.0,
        ..Default::default()
    });
    mixer.command(Command::Weather(Some(WeatherSound {
        rain_mm_h: 35.0,
        wind_m_s: 3.0,
        exposure: 1.0,
        ..Default::default()
    })));
    mixer.command(Command::RainScene(rain_scene::RainScene::patch(
        rain_scene::RainMaterial::Metal,
    )));
    let mut config = rain_tuning::RainConfig {
        wind_gain: 0.0,
        insect_gain: 0.0,
        ..Default::default()
    };
    mixer.set_rain_config(config);
    let mut samples = vec![[0.0; 2]; SAMPLE_RATE as usize];
    mixer.render(&mut samples);
    assert!(samples.iter().flatten().any(|v| v.abs() > 0.001));
    config.gain = 0.0;
    mixer.set_rain_config(config);
    mixer.render(&mut samples);
    assert!(
        samples.iter().skip(1024).flatten().all(|v| *v == 0.0),
        "muted rain must not leak through existing reverb"
    );
}
