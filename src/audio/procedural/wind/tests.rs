use super::*;
#[test]
fn wind_matches_upstream_c_reference() {
    // NoiseMachine e709f125, cc -O2, seed123, speed10m/s, bearing0.8, gain0.5.
    let expected = [
        [1.133_188_9e-6, 2.224_041e-6],
        [-9.476_577e-6, 2.361_895e-5],
        [-4.830_49e-5, -2.159_668e-5],
        [-7.132_094e-5, -4.452_542_6e-5],
        [-7.364_296e-5, 1.676_033e-5],
        [-6.883_75e-5, 6.087_612e-5],
        [-9.611_163e-5, 3.165_375e-5],
        [-1.158_104_2e-4, 5.990_588_3e-6],
    ];
    let mut wind = Wind::new(123);
    wind.follow(10.0, 0.8);
    for reference in expected {
        let actual = wind.next();
        for ear in 0..2 {
            assert!(
                (actual[ear] - reference[ear]).abs() < 1e-9,
                "{actual:?} != {reference:?}"
            );
        }
    }
}

#[test]
fn wind_width_balance_and_brightness_change_the_rendered_noise() {
    use crate::audio::rain_tuning::WindProfile;
    let render = |profile| {
        let mut wind = Wind::new(42);
        wind.configure(profile);
        wind.follow(10.0, std::f32::consts::FRAC_PI_2);
        let mut energy = [0.0f64; 2];
        let mut delta = 0.0;
        let mut previous = 0.0;
        for i in 0..44100 {
            let sample = wind.next();
            assert!(sample.into_iter().all(f32::is_finite));
            if i > 10000 {
                for ear in 0..2 {
                    energy[ear] += f64::from(sample[ear]).powi(2);
                }
                delta += f64::from(sample[0] - previous).powi(2);
            }
            previous = sample[0];
        }
        (energy, delta)
    };
    let centered = render(WindProfile {
        stereo_width: 0.0,
        balance: 0.0,
        ..Default::default()
    });
    assert_eq!(centered.0[0], centered.0[1]);
    let directional = render(WindProfile::default());
    assert!(directional.0[1] > directional.0[0] * 2.0);
    let bright = render(WindProfile {
        brightness: 4.0,
        rumble: 0.0,
        ..Default::default()
    });
    let dark = render(WindProfile {
        brightness: 0.25,
        rumble: 0.0,
        ..Default::default()
    });
    assert!(bright.1 / bright.0[0] > dark.1 / dark.0[0] * 4.0);
}
