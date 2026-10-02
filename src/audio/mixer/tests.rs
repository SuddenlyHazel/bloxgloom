use super::*;

fn weather(rain_mm_h: f32) -> WeatherSound {
    WeatherSound {
        rain_mm_h,
        wind_m_s: 5.0,
        bearing: 0.0,
        exposure: 1.0,
        daylight: 0.0,
    }
}

#[test]
fn world_and_preview_sources_change_only_at_fade_boundary() {
    let mut mixer = Mixer::new(19);
    mixer.command(Command::Weather(Some(weather(20.0))));
    mixer.render(&mut [[0.0; 2]; 10_000]);
    assert!(mixer.synth.world_active());
    assert!(mixer.fade > 0.99);

    mixer.set_controls(Controls {
        preset: Preset::Rain,
        ..Controls::default()
    });
    mixer.command(Command::Weather(None));
    mixer.render(&mut [[0.0; 2]; 256]);
    assert!(
        mixer.synth.world_active(),
        "world source must survive fade-out"
    );
    assert_eq!(mixer.preset, Preset::Off);
    assert!(mixer.fade > 0.7);
    mixer.render(&mut [[0.0; 2]; 12_000]);
    assert!(!mixer.synth.world_active());
    assert_eq!(mixer.preset, Preset::Rain);

    mixer.render(&mut [[0.0; 2]; 10_000]);
    mixer.set_controls(Controls::default());
    mixer.command(Command::Weather(Some(weather(25.0))));
    mixer.render(&mut [[0.0; 2]; 256]);
    assert!(!mixer.synth.world_active(), "preview must survive fade-out");
    assert_eq!(mixer.preset, Preset::Rain);
    mixer.render(&mut [[0.0; 2]; 12_000]);
    assert!(mixer.synth.world_active());
    assert_eq!(mixer.preset, Preset::Off);

    // Intensity changes within world mode do not trigger another fade/reset.
    mixer.render(&mut [[0.0; 2]; 10_000]);
    let generated = mixer.diagnostics().0;
    mixer.command(Command::Weather(Some(weather(30.0))));
    mixer.render(&mut [[0.0; 2]; 256]);
    assert!(mixer.fade > 0.99);
    assert!(mixer.diagnostics().0 >= generated);
    mixer.command(Command::Reset);
    assert!(mixer.desired_world.is_none());
    assert!(!mixer.synth.world_active());
}

#[test]
fn live_voice_updates_glide_pitch_gain_and_follow_position_without_restarting() {
    let mut mixer = Mixer::new(4);
    assert!(mixer.command(Command::Play {
        clip: std::sync::Arc::new(Clip::click()),
        position: Some([0.0, 0.0, 1.0]),
        gain: 1.0,
        pitch: 1.0,
        looping: true,
        id: 91,
    }));
    mixer.render(&mut [[0.0; 2]; 512]);
    assert_eq!(mixer.voices[0].cursor, 512.0);
    assert!(mixer.command(Command::Update {
        id: 91,
        position: Some([0.0, 0.0, -1.0]),
        gain: 0.5,
        pitch: 2.0
    }));
    mixer.render(&mut [[0.0; 2]; 512]);
    let voice = &mixer.voices[0];
    assert!(voice.cursor > 1100.0 && voice.cursor < 1536.0);
    assert!(voice.pitch > 1.0 && voice.pitch < 2.0);
    assert!(voice.gain > 0.5 && voice.gain < 1.0);
    mixer.render(&mut [[0.0; 2]; 3000]);
    assert!(mixer.voices[0].ears[0] > 10.0 * mixer.voices[0].ears[1]);
    assert!(!mixer.command(Command::Update {
        id: 91,
        position: None,
        gain: 1.0,
        pitch: f32::NAN
    }));
    assert!(!mixer.command(Command::Update {
        id: 92,
        position: None,
        gain: 1.0,
        pitch: 1.0
    }));
    assert!(mixer.command(Command::Stop(91)));
    assert!(!mixer.command(Command::Update {
        id: 91,
        position: None,
        gain: 1.0,
        pitch: 1.0
    }));
    mixer.render(&mut [[0.0; 2]; 1000]);
    assert!(mixer.voices.is_empty());
}

fn signal_clip(signal: impl Fn(usize) -> f32) -> Arc<Clip> {
    signal_clip_frames(SAMPLE_RATE as usize, signal)
}

fn signal_clip_frames(frames: usize, signal: impl Fn(usize) -> f32) -> Arc<Clip> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: SAMPLE_RATE,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::new(&mut bytes, spec).unwrap();
        for index in 0..frames {
            writer.write_sample(signal(index)).unwrap();
        }
        writer.finalize().unwrap();
    }
    Arc::new(Clip::decode(bytes.get_ref()).unwrap())
}

#[test]
fn initial_obstruction_muffles_short_one_shot_from_its_first_attack() {
    let clip = signal_clip_frames(1102, |i| {
        0.2 * (std::f32::consts::TAU * 6000.0 * i as f32 / SAMPLE_RATE as f32).sin()
    });
    let mut open = Mixer::new(4);
    let mut blocked = Mixer::new(4);
    for (mixer, lowpass_hz) in [(&mut open, 20_000.0), (&mut blocked, 500.0)] {
        mixer.set_controls(Controls {
            master: 1.0,
            effects: 1.0,
            ..Controls::default()
        });
        assert!(mixer.command(Command::PlayObstructed {
            clip: clip.clone(),
            position: [1.0, 0.0, 0.0],
            gain: 1.0,
            pitch: 1.0,
            looping: false,
            id: 81,
            transmission: 1.0,
            lowpass_hz,
        }));
    }
    let mut a = vec![[0.0; 2]; 2000];
    let mut b = a.clone();
    open.render(&mut a);
    blocked.render(&mut b);
    let energy = |frames: &[[f32; 2]]| {
        frames
            .iter()
            .flatten()
            .map(|sample| sample * sample)
            .sum::<f32>()
    };
    assert!(energy(&b) < energy(&a) * 0.02);
    assert!(energy(&b[..256]) < energy(&a[..256]) * 0.03);
    assert!(open.voices.is_empty() && blocked.voices.is_empty());

    let mut rejected = Mixer::new(4);
    for (transmission, lowpass_hz) in [(f32::NAN, 500.0), (0.5, 199.0), (1.1, 500.0)] {
        assert!(!rejected.command(Command::PlayObstructed {
            clip: clip.clone(),
            position: [1.0, 0.0, 0.0],
            gain: 1.0,
            pitch: 1.0,
            looping: false,
            id: 81,
            transmission,
            lowpass_hz,
        }));
        assert!(rejected.voices.is_empty());
    }
}

fn clip_mixer(clip: Arc<Clip>, position: Option<[f32; 3]>) -> Mixer {
    let mut mixer = Mixer::new(17);
    mixer.set_controls(Controls {
        master: 1.0,
        effects: 1.0,
        ..Controls::default()
    });
    assert!(mixer.command(Command::Play {
        clip,
        position,
        gain: 1.0,
        pitch: 1.0,
        looping: true,
        id: 71,
    }));
    mixer
}

#[test]
fn obstruction_reduces_energy_and_high_frequencies_in_production_mixer() {
    let clip = signal_clip(|i| {
        let t = i as f32 / SAMPLE_RATE as f32;
        0.15 * ((std::f32::consts::TAU * 200.0 * t).sin()
            + (std::f32::consts::TAU * 6000.0 * t).sin())
    });
    let mut open = clip_mixer(clip.clone(), Some([1.0, 0.0, 0.0]));
    let mut blocked = clip_mixer(clip, Some([1.0, 0.0, 0.0]));
    assert!(blocked.command(Command::Obstruction {
        id: 71,
        gain: 0.3,
        lowpass_hz: 800.0,
    }));
    let mut a = vec![[0.0; 2]; 44_100];
    let mut b = a.clone();
    open.render(&mut a);
    for chunk in b.chunks_mut(137) {
        blocked.render(chunk);
    }
    let energy = |frames: &[[f32; 2]]| frames[30_000..].iter().map(|f| f[0] * f[0]).sum::<f32>();
    let amplitude = |frames: &[[f32; 2]], frequency: f32| {
        let (mut real, mut imaginary) = (0.0f64, 0.0f64);
        for (i, frame) in frames.iter().enumerate().skip(30_000) {
            let angle =
                std::f64::consts::TAU * f64::from(frequency) * i as f64 / f64::from(SAMPLE_RATE);
            real += f64::from(frame[0]) * angle.cos();
            imaginary += f64::from(frame[0]) * angle.sin();
        }
        real.hypot(imaginary)
    };
    assert!(energy(&b) < energy(&a) * 0.1);
    let open_ratio = amplitude(&a, 6000.0) / amplitude(&a, 200.0);
    let blocked_ratio = amplitude(&b, 6000.0) / amplitude(&b, 200.0);
    assert!(blocked_ratio < open_ratio * 0.2);
    assert!(b.iter().flatten().all(|sample| sample.is_finite()));
}

#[test]
fn obstruction_glides_without_restarting_a_moving_loop_and_restores_open_path() {
    let mut mixer = clip_mixer(signal_clip(|_| 0.2), Some([1.0, 0.0, 0.0]));
    let mut warm = vec![[0.0; 2]; 20_000];
    mixer.render(&mut warm);
    let before = warm.last().unwrap()[0];
    let cursor = mixer.voices[0].cursor;
    assert!(mixer.command(Command::Obstruction {
        id: 71,
        gain: 0.1,
        lowpass_hz: 500.0,
    }));
    assert!(mixer.command(Command::Update {
        id: 71,
        position: Some([1.0, 0.0, 0.001]),
        gain: 1.0,
        pitch: 1.0,
    }));
    let mut after = [[0.0; 2]; 1];
    mixer.render(&mut after);
    assert_eq!(mixer.voices[0].cursor, cursor + 1.0);
    assert!((after[0][0] - before).abs() < 0.001);
    mixer.render(&mut warm);
    assert!(warm.last().unwrap()[0] < before * 0.12);
    assert!(mixer.command(Command::Obstruction {
        id: 71,
        gain: 1.0,
        lowpass_hz: 20_000.0,
    }));
    mixer.render(&mut warm);
    assert!((warm.last().unwrap()[0] - before).abs() < 0.002);
}

#[test]
fn obstruction_is_partition_independent_and_ignores_stopped_or_nonpositional_voices() {
    let clip = Arc::new(Clip::click());
    let mut a = clip_mixer(clip.clone(), Some([1.0, 0.0, 0.0]));
    let mut b = clip_mixer(clip.clone(), Some([1.0, 0.0, 0.0]));
    for mixer in [&mut a, &mut b] {
        assert!(mixer.command(Command::Obstruction {
            id: 71,
            gain: 0.4,
            lowpass_hz: 900.0
        }));
    }
    let mut whole = vec![[0.0; 2]; 12_000];
    let mut split = whole.clone();
    a.render(&mut whole);
    for chunk in split.chunks_mut(117) {
        b.render(chunk);
    }
    assert_eq!(whole, split);

    let mut a = clip_mixer(clip.clone(), None);
    let mut b = clip_mixer(clip, None);
    assert!(b.command(Command::Obstruction {
        id: 71,
        gain: 0.0,
        lowpass_hz: 200.0
    }));
    a.render(&mut whole);
    b.render(&mut split);
    assert_eq!(whole, split);
    assert!(b.command(Command::Stop(71)));
    assert!(b.command(Command::Obstruction {
        id: 71,
        gain: 1.0,
        lowpass_hz: 20_000.0
    }));
    b.render(&mut split);
    assert!(b.voices.is_empty());
    assert!(b.command(Command::Obstruction {
        id: 71,
        gain: 0.5,
        lowpass_hz: 1000.0
    }));
    assert!(b.voices.is_empty());
    assert!(b.command(Command::Reset));
    assert!(b.command(Command::Obstruction {
        id: 71,
        gain: 0.1,
        lowpass_hz: 500.0
    }));
    b.render(&mut split);
    assert!(split.iter().flatten().all(|sample| *sample == 0.0));
}

#[test]
fn malformed_obstruction_cannot_poison_audio_or_change_playback() {
    let clip = Arc::new(Clip::click());
    let mut mixer = clip_mixer(clip.clone(), Some([1.0, 0.0, 0.0]));
    let mut reference = clip_mixer(clip, Some([1.0, 0.0, 0.0]));
    for (gain, lowpass_hz) in [
        (f32::NAN, 1000.0),
        (f32::INFINITY, 1000.0),
        (-0.1, 1000.0),
        (1.1, 1000.0),
        (0.5, f32::NAN),
        (0.5, f32::INFINITY),
        (0.5, 199.0),
        (0.5, 20_001.0),
    ] {
        assert!(!mixer.command(Command::Obstruction {
            id: 71,
            gain,
            lowpass_hz
        }));
    }
    assert_eq!(mixer.rejected, 8);
    let mut frames = vec![[0.0; 2]; 2000];
    let mut unchanged = frames.clone();
    mixer.render(&mut frames);
    reference.render(&mut unchanged);
    assert_eq!(frames, unchanged);
    assert!(frames.iter().flatten().all(|sample| sample.is_finite()));
    assert!(frames.iter().flatten().any(|sample| sample.abs() > 1e-7));
}
