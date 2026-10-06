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

#[path = "tests/primary_cloud.rs"]
mod primary_cloud;

#[test]
fn atmosphere_drives_surface_color_energy_and_keeps_legacy_uniform_offsets() {
    let noon = Atmosphere::at(crate::daylight::INITIAL_MS);
    let dawn = Atmosphere::at(crate::daylight::CYCLE_MS / 48);
    let night = Atmosphere::at(crate::daylight::CYCLE_MS * 3 / 4);
    let storm = crate::render::weather::Presentation::new(1.0, 1.8, [0.0; 2], 1.0, 0.0, 0.0)
        .atmosphere(noon);
    let (lower, upper) = noon.ambient();
    assert!(lower.distance(Vec3::new(0.000388, 0.000834, 0.001520)) < 0.00002);
    assert!(upper.distance(Vec3::new(0.025788, 0.065146, 0.152631)) < 0.00002);
    assert!(
        dawn.sun_radiance().x / dawn.sun_radiance().z
            > noon.sun_radiance().x / noon.sun_radiance().z
    );
    assert_eq!(night.sun_radiance(), Vec3::ZERO);
    assert_eq!(storm.sun_radiance(), Vec3::ZERO);
    assert!(night.ambient().1.length() < upper.length() * 0.05);
    assert!(storm.ambient().1.length() < upper.length());
    let data = noon.camera_data(Mat4::IDENTITY, Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(std::mem::size_of_val(&data), 224);
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

#[test]
fn lunar_phase_uses_authoritative_whole_days_and_wraps_after_eight() {
    let cycle = crate::daylight::CYCLE_MS;
    for (phase, multiplier) in [1.0, 0.875, 0.75, 0.625, 0.5, 0.625, 0.75, 0.875]
        .into_iter()
        .enumerate()
    {
        let day = phase as u64 * cycle;
        let start = Atmosphere::at(day);
        let night = Atmosphere::at(day + cycle * 3 / 4);
        assert_eq!(start.moon_phase, phase as u32);
        assert_eq!(night.moon_phase, start.moon_phase);
        assert_eq!(night.moon_multiplier(), multiplier);
        assert_eq!(Atmosphere::at(day + 8 * cycle).moon_phase, phase as u32);
    }
    let maximum = Atmosphere::at(u64::MAX);
    assert_eq!(maximum.moon_phase, ((u64::MAX / cycle) % 8) as u32);
    assert!(maximum.moon_multiplier().is_finite());
}

#[test]
fn scene_transport_cloud_uniform_preserves_sun_energy_and_legacy_offsets() {
    let clear = Atmosphere::at(crate::daylight::INITIAL_MS);
    let cloud = Atmosphere {
        cloud: 0.8,
        drift: [12.0, -7.0],
        scene_transport: true,
        ..clear
    };
    assert_eq!(cloud.sun_radiance(), clear.sun_radiance());
    assert_eq!(cloud.ambient(), clear.ambient());
    let data = cloud.camera_data(Mat4::IDENTITY, Vec3::ZERO);
    assert_eq!(&data[52..56], &[0.8, 12.0, -7.0, 1.0]);
    assert_eq!(
        &data[32..36],
        &clear.camera_data(Mat4::IDENTITY, Vec3::ZERO)[32..36]
    );
    let fallback = Atmosphere {
        scene_transport: false,
        ..cloud
    };
    assert!((fallback.sun_radiance() - clear.sun_radiance() * 0.04).length() < 0.000001);
}

#[path = "tests/sky_diffuse.rs"]
mod sky_diffuse;
