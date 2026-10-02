use super::*;

fn camera(position: Vec3) -> Camera {
    Camera {
        position,
        yaw: 0.7,
        pitch: -0.2,
        fov_y_radians: 1.0,
    }
}

#[test]
fn quality_is_bounded_and_unsupported_resolution_disables_shadow_work() {
    let off = Settings::for_quality(SunShadowQuality::Off, 4096);
    assert_eq!((off.resolution, off.distance), (1, 0.0));
    let medium = Settings::for_quality(SunShadowQuality::Medium, 1024);
    assert_eq!(medium.resolution, 1024);
    assert!(medium.distance > 0.0);
    assert_eq!(
        Settings::for_quality(SunShadowQuality::High, 256).distance,
        0.0
    );
    assert_eq!(
        Settings::for_quality(SunShadowQuality::High, 2048).resolution,
        2048
    );
}

#[test]
fn projections_are_finite_at_noon_horizon_and_zenith_and_disable_at_night() {
    let settings = Settings::for_quality(SunShadowQuality::Medium, 4096);
    for phase in [
        0,
        crate::daylight::CYCLE_MS / 4,
        crate::daylight::CYCLE_MS / 2,
        crate::daylight::CYCLE_MS * 3 / 4,
    ] {
        let atmosphere = Atmosphere::at(phase);
        let p = Projection::new(camera(Vec3::new(-17.0, 45.0, 64.0)), atmosphere, settings);
        assert!(p.matrix.is_finite());
        assert_eq!(
            p.enabled,
            atmosphere.sun.y > 0.01 && atmosphere.strength > 0.0
        );
        let center = p.matrix.project_point3(p.eye);
        assert!(center.x.abs() < 0.01 && center.y.abs() < 0.01);
        assert!((0.45..0.55).contains(&center.z));
    }
    let mut atmosphere = Atmosphere::at(crate::daylight::INITIAL_MS);
    atmosphere.sun = Vec3::Y;
    assert!(
        Projection::new(camera(Vec3::ZERO), atmosphere, settings)
            .matrix
            .is_finite()
    );
    assert!(
        !Projection::new(
            camera(Vec3::ZERO),
            atmosphere,
            Settings::for_quality(SunShadowQuality::Off, 4096)
        )
        .enabled
    );
}

#[test]
fn sub_texel_camera_motion_keeps_world_shadow_projection_stable() {
    let settings = Settings::for_quality(SunShadowQuality::Medium, 4096);
    let atmosphere = Atmosphere::at(crate::daylight::INITIAL_MS);
    let a = Projection::new(camera(Vec3::ZERO), atmosphere, settings);
    let b = Projection::new(camera(Vec3::splat(0.0001)), atmosphere, settings);
    assert_eq!(a.matrix, b.matrix);
    let world = Vec3::new(5.0, 2.0, -8.0);
    assert_eq!(
        a.matrix.project_point3(world),
        b.matrix.project_point3(world)
    );
    // Fade center remains the true camera position, not the snapped grid.
    assert_ne!(a.eye, b.eye);
}

#[test]
fn offscreen_nearby_occluders_remain_in_shadow_frustum() {
    let settings = Settings::for_quality(SunShadowQuality::Medium, 4096);
    let p = Projection::new(
        camera(Vec3::new(0.0, 33.0, 0.0)),
        Atmosphere::at(crate::daylight::INITIAL_MS),
        settings,
    );
    assert!(p.contains_chunk(crate::world::ChunkKey { x: 0, y: 2, z: -1 }, 0.0));
    assert!(p.contains_chunk(crate::world::ChunkKey { x: -1, y: 2, z: 0 }, 0.0));
    assert!(!p.contains_chunk(
        crate::world::ChunkKey {
            x: 100,
            y: 2,
            z: 100
        },
        0.0
    ));
}

#[test]
fn grazing_sun_shadow_strength_fades_continuously() {
    let settings = Settings::for_quality(SunShadowQuality::Medium, 4096);
    let mut atmosphere = Atmosphere::at(crate::daylight::INITIAL_MS);
    let mut weights = Vec::new();
    for y in [0.0, 0.009, 0.01, 0.0101, 0.0249, 0.0251, 0.05, 0.1, 0.2] {
        atmosphere.sun = Vec3::new((1.0f32 - y * y).sqrt(), y, 0.0);
        weights.push(Projection::new(camera(Vec3::ZERO), atmosphere, settings).data()[18]);
    }
    assert_eq!(&weights[..3], &[0.0; 3]);
    assert!(weights[3] < 0.00001);
    assert!((weights[5] - weights[4]).abs() < 0.003);
    assert!(weights.windows(2).all(|pair| pair[1] >= pair[0]));
    assert_eq!(&weights[7..], &[1.0; 2]);
}
