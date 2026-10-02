use super::*;

fn hillside(x: i32, y: i32, z: i32, sealed: bool) -> Option<bool> {
    // A 3-block-wide, 2-block-high shelter dug from an otherwise solid hill.
    // Its entrance faces +X; exterior ground is at y=0.
    Some(if x >= 3 {
        y <= 0
    } else if (0..=2).contains(&x) && (-1..=1).contains(&z) && (1..=2).contains(&y) {
        sealed && x == 2
    } else {
        true
    })
}
#[test]
fn open_hillside_entrance_is_audible_and_closing_it_restores_muffling() {
    let eye = Vec3::new(0.5, 1.6, 0.5);
    let open = exposure(eye, 12, |x, y, z| hillside(x, y, z, false));
    assert!(open > 0.2 && open < 0.8, "open entrance: {open}");
    assert_eq!(exposure(eye, 12, |x, y, z| hillside(x, y, z, true)), 0.0);
    assert_eq!(
        exposure(Vec3::new(4.5, 1.6, 0.5), 12, |x, y, z| hillside(
            x, y, z, false
        )),
        1.0
    );
}
#[test]
fn unknown_cells_and_walls_do_not_create_outdoor_openings() {
    let eye = Vec3::new(0.5, 1.6, 0.5);
    assert_eq!(exposure(eye, 12, |_, _, _| None), 0.0);
    assert_eq!(
        exposure(eye, 12, |x, y, z| if x >= 3 {
            None
        } else {
            hillside(x, y, z, false)
        }),
        0.0
    );
    assert_eq!(
        exposure(eye, 12, |x, y, z| if x >= 3 && y >= 6 {
            None
        } else {
            hillside(x, y, z, false)
        }),
        0.0
    );
    // A skylit exterior behind a solid wall is not an audible doorway.
    assert_eq!(exposure(eye, 12, |x, y, z| hillside(x, y, z, true)), 0.0);
}

#[test]
fn hillside_opening_produces_audible_rain_through_the_game_mixer() {
    use crate::audio::{Command, Mixer, WeatherSound};
    use std::sync::Arc;
    let eye = Vec3::new(0.5, 1.6, 0.5);
    let catalog = crate::content::Catalog::builtins();
    let scene = Arc::new(super::super::rain_scene::sample(
        eye,
        12,
        &catalog,
        |x, y, z| {
            hillside(x, y, z, false).map(|solid| {
                if solid {
                    crate::world::STONE
                } else {
                    crate::world::AIR
                }
            })
        },
    ));
    let render = |shelter| {
        let mut mixer = Mixer::new(42);
        mixer.command(Command::RainScene(scene.clone()));
        mixer.command(Command::Listener {
            position: eye.to_array(),
            yaw: 0.0,
        });
        mixer.command(Command::Weather(Some(WeatherSound {
            rain_mm_h: 30.0,
            exposure: shelter,
            ..WeatherSound::default()
        })));
        let mut samples = vec![[0.0; 2]; 88_200];
        mixer.render(&mut samples);
        samples[44_100..]
            .iter()
            .flatten()
            .map(|v| {
                assert!(v.is_finite());
                f64::from(*v).powi(2)
            })
            .sum::<f64>()
    };
    let open = render(exposure(eye, 12, |x, y, z| hillside(x, y, z, false)));
    let sealed = render(0.0);
    let exposed = render(1.0);
    // Retain at least 10% of the exposed RMS level and clear shelter contrast,
    // independently of rain mix tuning.
    assert!(
        exposed > 0.0 && open > exposed * 0.01 && open > sealed * 2.0,
        "open {open}, sealed {sealed}, exposed {exposed}"
    );
}
