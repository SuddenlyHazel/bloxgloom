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
