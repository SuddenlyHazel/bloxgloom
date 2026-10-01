use super::*;

fn weather(rain_mm_h: f32) -> WeatherSound {
    WeatherSound {
        rain_mm_h,
        wind_m_s: 5.0,
        bearing: 0.0,
        exposure: 1.0,
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
