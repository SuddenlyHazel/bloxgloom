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
