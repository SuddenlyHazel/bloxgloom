use super::*;
fn camera() -> Camera {
    Camera {
        position: Vec3::new(0.0, 10.0, 0.0),
        yaw: 0.0,
        pitch: 0.0,
        fov_y_radians: 1.2,
    }
}
#[test]
fn rain_is_bounded_and_roof_clipping_keeps_streaks_above_cover() {
    let mut weather = Presentation::new(1.0, 1.0, [18.0, 0.0], 1.0, 7.0, 0.0);
    let open = weather.vertices(camera());
    assert_eq!(open.len(), MAX_STREAKS * 27);
    weather.set_cover([-8, -8], [12.0; 256]);
    let covered = weather.vertices(camera());
    assert!(covered.len() < open.len());
    assert!(covered.chunks_exact(9).all(|v| v[1] >= 12.0));
    let mut edge = [f32::NEG_INFINITY; 256];
    for z in 0..16 {
        for x in 8..16 {
            edge[z * 16 + x] = 12.0;
        }
    }
    weather.set_cover([-8, -8], edge);
    assert!(weather.vertices(camera()).chunks_exact(9).all(|v| {
        let x = (v[0].floor() as i32 + 8) as usize;
        let z = (v[2].floor() as i32 + 8) as usize;
        v[1] >= edge[z * 16 + x]
    }));
    weather.set_cover([-8, -8], [f32::INFINITY; 256]);
    assert!(weather.vertices(camera()).is_empty());
}
#[test]
fn clear_and_sheltered_rain_are_empty_and_flash_preserves_sun_direction() {
    assert!(Presentation::default().vertices(camera()).is_empty());
    let hidden = Presentation::new(1.0, 1.0, [0.0; 2], 0.0, 0.0, 1.0);
    assert!(hidden.vertices(camera()).is_empty());
    let night = Atmosphere::at(crate::daylight::CYCLE_MS * 3 / 4);
    let flash = hidden.atmosphere(night);
    assert_eq!(flash.sun, night.sun);
    assert!(flash.strength > night.strength);
}

#[test]
fn cloud_advection_is_continuous_across_wind_changes_and_clock_wrap() {
    let base = Atmosphere::at(crate::daylight::INITIAL_MS);
    let clouds = |seconds, wind| {
        Presentation::new(1.0, 1.0, [wind, 0.0], 1.0, seconds, 0.0)
            .atmosphere(base)
            .drift
    };
    assert_eq!(clouds(600.0, 2.0), clouds(600.0, 18.0));
    let before = clouds(3599.99, 18.0);
    let after = clouds(0.01, 2.0);
    assert!((before[0] - after[0]).abs() < 0.001);
    assert!((before[1] - after[1]).abs() < 0.001);
}
