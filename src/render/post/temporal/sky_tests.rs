use super::super::*;
#[path = "sky_tests/gpu.rs"]
mod gpu;
#[path = "sky_tests/oracle.rs"]
mod oracle;
#[test]
fn reference_temporal_shader_validates() {
    let source = Temporal::shader();
    let module = wgpu::naga::front::wgsl::parse_str(&source).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}
#[test]
fn directional_sky_reprojection_matches_camera_basis_and_cancels_raster_jitter() {
    let previous = super::camera();
    for translation in [Vec3::ZERO, Vec3::new(0.5, 1., -0.8)] {
        for yaw in [0., 0.04, -0.07] {
            let current = crate::render::Camera {
                position: translation,
                yaw,
                ..previous
            };
            let stable = crate::render::view_projection(current, 24, 16);
            let old = crate::render::view_projection(previous, 24, 16);
            for frame in 0..8 {
                let offset = jitter(frame);
                let inverse = jitter_matrix(stable, offset, 24, 16).inverse();
                for uv in [
                    Vec2::new(0.3, 0.4),
                    Vec2::new(0.5, 0.5),
                    Vec2::new(0.7, 0.6),
                ] {
                    let clip = (uv + offset / Vec2::new(24., 16.)) * Vec2::new(2., -2.)
                        + Vec2::new(-1., 1.);
                    let near = inverse * glam::Vec4::new(clip.x, clip.y, 0., 1.);
                    let far = inverse * glam::Vec4::new(clip.x, clip.y, 1., 1.);
                    let ray = (far.truncate() / far.w - near.truncate() / near.w).normalize();
                    let projected = old * ray.extend(0.);
                    let actual = projected.truncate().truncate() / projected.w
                        * Vec2::new(0.5, -0.5)
                        + Vec2::splat(0.5);
                    let expected = oracle::previous_uv(current, previous, uv);
                    assert!(
                        actual.distance(expected) < 2e-5,
                        "{actual:?} vs {expected:?}, frame{frame}"
                    );
                }
            }
        }
    }
}
