use super::*;

fn controls() -> Controls {
    Controls {
        master: 1.0,
        ambient: 0.25,
        effects: 0.75,
        preset: Preset::Storm,
    }
}

#[test]
fn audio_output_callback_converts_channels_and_silences_underruns() {
    let shared = Shared::new(controls());
    let (mut producer, mut consumer) = RingBuffer::new(RING_FRAMES);
    producer
        .push(Frame {
            epoch: 0,
            samples: [0.5, -0.25],
        })
        .unwrap();
    let mut surround = [9.0_f32; 12];
    write_output(&mut surround, 6, &mut consumer, &shared);
    assert_eq!(
        surround,
        [0.5, -0.25, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]
    );
    assert_eq!(shared.underruns.load(Ordering::Relaxed), 1);
    producer
        .push(Frame {
            epoch: 0,
            samples: [0.5, -0.25],
        })
        .unwrap();
    let mut mono = [0.0_f32; 1];
    write_output(&mut mono, 1, &mut consumer, &shared);
    assert_eq!(mono, [0.125]);
    let mut unsigned = [0_u16; 2];
    write_output(&mut unsigned, 2, &mut consumer, &shared);
    assert_eq!(
        unsigned, [32768; 2],
        "unsigned PCM silence is midpoint, not zero"
    );
}

#[test]
fn audio_output_reset_discards_buffered_old_session_and_shutdown_is_silent() {
    let shared = Shared::new(controls());
    let (mut producer, mut consumer) = RingBuffer::new(RING_FRAMES);
    for _ in 0..4 {
        producer
            .push(Frame {
                epoch: 0,
                samples: [1.0; 2],
            })
            .unwrap();
    }
    shared.epoch.store(1, Ordering::Release);
    producer
        .push(Frame {
            epoch: 1,
            samples: [0.25; 2],
        })
        .unwrap();
    let mut output = [0.0_f32; 2];
    write_output(&mut output, 2, &mut consumer, &shared);
    assert_eq!(output, [0.25; 2]);
    assert_eq!(shared.underruns.load(Ordering::Relaxed), 0);
    producer
        .push(Frame {
            epoch: 1,
            samples: [1.0; 2],
        })
        .unwrap();
    shared.shutdown.store(true, Ordering::Release);
    write_output(&mut output, 2, &mut consumer, &shared);
    assert_eq!(output, [0.0; 2]);
}

#[test]
fn audio_output_linear_resampling_preserves_time_and_stereo() {
    let mut source = Source::new();
    for (i, frame) in source.frames.iter_mut().enumerate() {
        *frame = [i as f32 * 0.001, -(i as f32) * 0.001];
    }
    source.cursor = 0;
    let mut resampler = Resampler::new(48_000);
    for index in 0..96 {
        let frame = resampler.next(&mut source);
        let expected = index as f32 * 44_100.0 / 48_000.0 * 0.001;
        assert!((frame[0] - expected).abs() < 0.000001);
        assert!((frame[1] + expected).abs() < 0.000001);
    }
    resampler.reset();
    source.cursor = 0;
    assert_eq!(resampler.next(&mut source), [0.0; 2]);
}

#[test]
fn audio_output_controls_are_coherent_bounded_and_queue_reset_is_guaranteed() {
    let shared = Arc::new(Shared::new(controls()));
    let (commands, receiver) = mpsc::sync_channel(COMMAND_CAPACITY);
    let (_complete, done) = mpsc::channel();
    drop(_complete);
    let output = AudioOutput {
        commands,
        shared,
        done,
        worker: None,
    };
    for _ in 0..COMMAND_CAPACITY {
        assert!(output.try_send(Command::Stop(1)));
    }
    assert!(!output.try_send(Command::Stop(2)));
    assert!(output.try_send(Command::Reset));
    assert_eq!(output.shared.epoch.load(Ordering::Acquire), 1);
    assert!(receiver.try_iter().all(|command| command.epoch == 0));
    output.set_controls(Controls {
        master: f32::NAN,
        ambient: -1.0,
        effects: 2.0,
        preset: Preset::Rain,
    });
    let updated = unpack_controls(output.shared.controls.load(Ordering::Acquire));
    assert_eq!(updated.master, 0.0);
    assert_eq!(updated.ambient, 0.0);
    assert_eq!(updated.effects, 1.0);
    assert!(matches!(updated.preset, Preset::Rain));
    assert_eq!(output.stats().rejected_commands, 1);
}
