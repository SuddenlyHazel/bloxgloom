use super::*;

fn enabled() -> CompressorConfig {
    CompressorConfig {
        enabled: true,
        threshold_db: -20.0,
        ratio: 4.0,
        attack_ms: 0.1,
        release_ms: 100.0,
        knee_db: 0.0,
        ..CompressorConfig::default()
    }
}

#[test]
fn default_mix_is_exactly_transparent_and_preserves_category_mapping() {
    let mut processor = Processor::default();
    let stems = [[0.1, -0.2], [0.3, 0.4], [-0.2, 0.1], [0.15, -0.1]];
    for _ in 0..100 {
        let output = processor.process(stems, [1.0; 3]);
        let expected: [f32; 2] =
            std::array::from_fn(|channel| stems.iter().map(|stem| stem[channel]).sum());
        assert_eq!(output, expected);
    }
    assert_eq!(processor.process(stems, [1.0, 1.0, 0.0]), [0.25, -0.3]);
    assert_eq!(processor.process(stems, [1.0, 0.0, 1.0]), [0.10000001, 0.5]);
}

#[test]
fn stereo_peak_compression_preserves_channel_balance() {
    let mut compressor = Compressor::new(enabled());
    let mut output = [0.0; 2];
    for _ in 0..2000 {
        output = compressor.process([1.0, 0.25]);
    }
    // Twenty dB above threshold at 4:1 yields fifteen dB reduction.
    let expected_gain = 10.0_f32.powf(-15.0 / 20.0);
    assert!((output[0] - expected_gain).abs() < 1e-5);
    assert_eq!(output[1], output[0] * 0.25);
    let reversed = compressor.process([0.25, 1.0]);
    assert!((reversed[1] - output[0]).abs() < 1e-5);
    assert_eq!(reversed[0], reversed[1] * 0.25);
}

#[test]
fn envelope_attack_and_release_follow_sample_time() {
    let mut config = enabled();
    config.attack_ms = 10.0;
    let mut compressor = Compressor::new(config);
    for _ in 0..441 {
        compressor.process([1.0; 2]);
    }
    let attacked = compressor.envelope;
    assert!((attacked - (1.0 - (-1.0_f32).exp())).abs() < 1e-4);
    for _ in 0..4410 {
        compressor.process([0.0; 2]);
    }
    assert!((compressor.envelope - attacked * (-1.0_f32).exp()).abs() < 1e-4);
}

#[test]
fn soft_knee_is_continuous_and_only_compresses_in_or_above_the_knee() {
    let mut config = enabled();
    config.knee_db = 6.0;
    let evaluate = |db: f32| {
        let input = 10.0_f32.powf(db / 20.0);
        let mut compressor = Compressor::new(config);
        compressor.envelope = input;
        let output = compressor.process([input; 2])[0];
        20.0 * (output / input).log10()
    };
    assert!(evaluate(-24.0).abs() < 1e-6);
    assert!((evaluate(-23.0)).abs() < 1e-6);
    assert!((evaluate(-20.0) + 0.5625).abs() < 1e-4);
    assert!((evaluate(-17.0) + 2.25).abs() < 1e-4);
    assert!((evaluate(-16.999) - evaluate(-17.001)).abs() < 0.002);
}

#[test]
fn disabled_compressor_ignores_makeup_and_history() {
    let mut compressor = Compressor::new(enabled());
    for _ in 0..1000 {
        compressor.process([1.0; 2]);
    }
    let mut config = enabled();
    config.enabled = false;
    config.makeup_db = 24.0;
    compressor.set_config(config);
    assert_eq!(compressor.process([0.125, -0.25]), [0.125, -0.25]);
    assert_eq!(compressor.envelope, 0.0);
}

#[test]
fn category_and_master_mutes_survive_maximum_makeup() {
    let mut processor = Processor::default();
    let mut config = MixConfig::default();
    for bus in &mut config.buses {
        bus.compressor = enabled();
        bus.compressor.makeup_db = 24.0;
    }
    config.master = enabled();
    config.master.makeup_db = 24.0;
    processor.set_config(config);
    for _ in 0..5000 {
        assert_eq!(processor.process([[1.0; 2]; 4], [1.0, 0.0, 0.0]), [0.0; 2]);
        assert_eq!(processor.process([[1.0; 2]; 4], [0.0, 1.0, 1.0]), [0.0; 2]);
    }
}

#[test]
fn bus_gain_changes_fade_and_zero_settles_to_exact_silence() {
    let mut processor = Processor::default();
    let mut config = MixConfig::default();
    config.buses[0].gain = 0.0;
    config.buses[0].compressor = enabled();
    config.buses[0].compressor.makeup_db = 24.0;
    processor.set_config(config);
    let first = processor.process([[0.01; 2], [0.0; 2], [0.0; 2], [0.0; 2]], [1.0; 3]);
    assert!(first[0] > 0.0);
    for _ in 0..SAMPLE_RATE {
        processor.process([[1.0; 2]; 4], [1.0; 3]);
    }
    assert_eq!(processor.gains[0], 0.0);
    assert_eq!(
        processor.process([[1.0; 2], [0.0; 2], [0.0; 2], [0.0; 2]], [1.0; 3]),
        [0.0; 2]
    );
}

#[test]
fn session_reset_discards_envelopes_but_retains_bus_configuration() {
    let mut processor = Processor::default();
    let mut config = MixConfig::default();
    config.buses[1].gain = 0.5;
    config.buses[1].compressor = enabled();
    config.master = enabled();
    processor.set_config(config);
    for _ in 0..2000 {
        processor.process([[1.0; 2]; 4], [1.0; 3]);
    }
    assert!(processor.master.envelope > 0.0);
    assert!(processor.compressors[1].envelope > 0.0);
    processor.reset();
    assert_eq!(processor.config, config);
    assert_eq!(processor.gains[1], 0.5);
    assert_eq!(processor.master.envelope, 0.0);
    assert!(
        processor
            .compressors
            .iter()
            .all(|compressor| compressor.envelope == 0.0)
    );
}

#[test]
fn nonfinite_inputs_do_not_poison_subsequent_audio() {
    let mut processor = Processor::default();
    let config = MixConfig {
        master: enabled(),
        ..MixConfig::default()
    };
    processor.set_config(config);
    let output = processor.process([[f32::NAN, f32::INFINITY]; 4], [1.0; 3]);
    assert_eq!(output, [0.0; 2]);
    let output = processor.process([[0.1; 2]; 4], [1.0; 3]);
    assert!(output.iter().all(|value| value.is_finite() && *value > 0.0));
}

#[test]
#[ignore = "direct DSP cost probe; run with --release --ignored --nocapture"]
fn sample_processing_cost_probe() {
    for (compressed, quiet) in [(false, false), (true, true), (true, false)] {
        let mut processor = Processor::default();
        let mut config = MixConfig::default();
        if compressed {
            config.master = enabled();
            for bus in &mut config.buses {
                bus.compressor = enabled();
            }
        }
        processor.set_config(config);
        const FRAMES: usize = 441_000;
        let start = std::time::Instant::now();
        for frame in 0..FRAMES {
            let sample = if quiet {
                0.001
            } else if frame % 4410 < 2205 {
                0.5
            } else {
                0.02
            };
            let output = processor.process(
                std::hint::black_box([[sample, -sample * 0.75]; 4]),
                std::hint::black_box([1.0; 3]),
            );
            std::hint::black_box(output);
        }
        let ns_frame = start.elapsed().as_nanos() as f64 / FRAMES as f64;
        println!(
            "bus compressors enabled={compressed}, quiet={quiet}: {ns_frame:.2} ns/frame, {:.3} ms/1024 frames, {:.3}% one core at 44.1 kHz",
            ns_frame * 1024.0 / 1_000_000.0,
            ns_frame * 44_100.0 / 10_000_000.0,
        );
    }
}
