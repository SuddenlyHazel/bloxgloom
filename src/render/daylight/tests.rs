use super::*;

#[test]
fn sun_and_atmosphere_wrap_and_night_preserves_a_small_sky_light() {
    let noon = Atmosphere::at(crate::daylight::INITIAL_MS);
    let midnight = Atmosphere::at(crate::daylight::CYCLE_MS * 3 / 4);
    let wrapped = Atmosphere::at(crate::daylight::INITIAL_MS + crate::daylight::CYCLE_MS);
    assert!(noon.sun.y > 0.6);
    assert!(midnight.sun.y < -0.6);
    assert!((noon.strength - 1.0).abs() < 1e-6);
    assert!((midnight.strength - 0.035).abs() < 1e-6);
    assert_eq!(noon.sun, wrapped.sun);
    assert!(midnight.horizon.length() < noon.horizon.length() / 10.0);
    let dawn = Atmosphere::at(0);
    let before = Atmosphere::at(crate::daylight::CYCLE_MS - 1);
    assert!(dawn.sun.distance(before.sun) < 0.0001);
    assert!(dawn.horizon.distance(before.horizon) < 0.0001);
}

#[path = "calibration_tests.rs"]
mod calibration;

#[test]
fn atmosphere_drives_surface_color_energy_and_keeps_legacy_uniform_offsets() {
    let noon = Atmosphere::at(crate::daylight::INITIAL_MS);
    let dawn = Atmosphere::at(crate::daylight::CYCLE_MS / 48);
    let night = Atmosphere::at(crate::daylight::CYCLE_MS * 3 / 4);
    let storm = crate::render::weather::Presentation::new(1.0, 1.8, [0.0; 2], 1.0, 0.0, 0.0)
        .atmosphere(noon);
    let (lower, upper) = noon.ambient();
    assert!(lower.distance(Vec3::new(0.36, 0.335, 0.30)) < 1e-6);
    assert!(upper.distance(Vec3::new(0.55, 0.57, 0.60)) < 1e-6);
    assert!(
        dawn.sun_radiance().x / dawn.sun_radiance().z
            > noon.sun_radiance().x / noon.sun_radiance().z
    );
    assert_eq!(night.sun_radiance(), Vec3::ZERO);
    assert_eq!(storm.sun_radiance(), Vec3::ZERO);
    assert!(night.ambient().1.length() < upper.length() * 0.05);
    assert!(storm.ambient().1.length() < upper.length());
    let data = noon.camera_data(Mat4::IDENTITY, Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(std::mem::size_of_val(&data), 208);
    assert_eq!(&data[24..27], &[1.0, 2.0, 3.0]);
    assert_eq!(
        &data[32..36],
        &crate::config::parallax::Parallax::default().uniform()
    );
    assert_eq!(&data[36..39], &noon.sun_radiance().to_array());
    assert_eq!(&data[48..51], &upper.to_array());
}

#[test]
fn lighting_controls_are_independent_and_do_not_change_exposure() {
    let base = Atmosphere::at(crate::daylight::INITIAL_MS);
    let mut tuned = base;
    tuned.lighting.sun_intensity = 0.0;
    tuned.lighting.ambient_intensity = 2.0;
    tuned.lighting.environment_intensity = 0.4;
    let data = tuned.camera_data(Mat4::IDENTITY, Vec3::ZERO);
    assert_eq!(tuned.sun_radiance(), Vec3::ZERO);
    assert_eq!(tuned.ambient().0, base.ambient().0 * 2.0);
    assert_eq!(data[43], 0.4);
    assert_eq!(
        &data[20..24],
        &base.camera_data(Mat4::IDENTITY, Vec3::ZERO)[20..24]
    );
    assert_eq!(tuned.horizon, base.horizon);
    assert_eq!(tuned.zenith, base.zenith);
}

#[test]
fn atmosphere_is_continuous_across_dawn_and_the_clock_wrap() {
    let dawn = Atmosphere::at(0);
    let before = Atmosphere::at(crate::daylight::CYCLE_MS - 1);
    assert!(dawn.sun_radiance().distance(before.sun_radiance()) < 0.0001);
    assert!(dawn.ambient().0.distance(before.ambient().0) < 0.0001);
    assert!(dawn.ambient().1.distance(before.ambient().1) < 0.0001);
}

#[test]
fn local_directionality_is_bounded_and_uses_the_shared_camera_slot() {
    let mut atmosphere = Atmosphere::at(crate::daylight::INITIAL_MS);
    assert!((atmosphere.camera_data(Mat4::IDENTITY, Vec3::ZERO)[47] - 0.65).abs() < 1e-6);
    atmosphere.lighting.local_directionality = 0.0;
    assert_eq!(atmosphere.camera_data(Mat4::IDENTITY, Vec3::ZERO)[47], 0.0);
    atmosphere.lighting.local_directionality = 4.0;
    assert_eq!(atmosphere.camera_data(Mat4::IDENTITY, Vec3::ZERO)[47], 1.0);
}
