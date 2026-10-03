use super::*;
fn schema() -> VisualSchema {
    VisualSchema {
        clips: vec!["idle".into(), "attack".into()],
        clip_loops: vec![true, false],
        variants: vec![("eyes".into(), vec!["open".into(), "closed".into()])],
        layers: vec!["hat".into()],
        tints: vec!["fur".into()],
    }
}
#[test]
fn visual_codec_preserves_playback_identity_and_appearance_without_private_data() {
    let mut state = VisualState {
        sample_tick: 101,
        sequence: 4,
        playback: Some(ClipPlayback {
            clip: 1,
            speed: 1.5,
            looping: false,
            crossfade_s: 0.2,
            started_tick: 90,
            sequence: 4,
        }),
        ..Default::default()
    };
    state.variants[0] = 1;
    state.layers[0] = 0;
    state.tints[0] = Some(Tint {
        rgb: [12, 128, 240],
        mode: TintMode::Replace,
    });
    let bytes = state.encode(&schema()).unwrap();
    assert!(bytes.len() <= MAX_VISUAL_BYTES);
    assert_eq!(VisualState::decode(&bytes, &schema()).unwrap(), state);
    for end in 0..bytes.len() {
        assert!(VisualState::decode(&bytes[..end], &schema()).is_err());
    }
    let mut tail = bytes;
    tail.push(0);
    assert!(VisualState::decode(&tail, &schema()).is_err());
}
#[test]
fn visual_schema_rejects_unregistered_controls_and_invalid_playback() {
    let base = VisualState::default();
    let schema = schema();
    let mut state = base;
    state.variants[0] = 2;
    assert!(state.encode(&schema).is_err());
    state = base;
    state.layers[1] = 1;
    assert!(state.encode(&schema).is_err());
    state = base;
    state.tints[1] = Some(Tint {
        rgb: [0; 3],
        mode: TintMode::Multiply,
    });
    assert!(state.encode(&schema).is_err());
    for speed in [f32::NAN, f32::INFINITY, -1., 8.1] {
        state = base;
        state.playback = Some(ClipPlayback {
            clip: 0,
            speed,
            looping: true,
            crossfade_s: 0.,
            started_tick: 0,
            sequence: 1,
        });
        assert!(state.encode(&schema).is_err());
    }
    state = base;
    state.playback = Some(ClipPlayback {
        clip: 0,
        speed: 1.,
        looping: true,
        crossfade_s: 0.,
        started_tick: 1,
        sequence: 1,
    });
    assert!(state.encode(&schema).is_err());
}
