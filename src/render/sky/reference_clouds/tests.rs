#[test]
fn source_cloud_dither_uses_explicit_sample_and_separate_uniform_lanes() {
    let camera = crate::render::Camera {
        position: glam::Vec3::ZERO,
        yaw: 0.,
        pitch: 0.,
        fov_y_radians: 1.,
    };
    let atmosphere = crate::render::daylight::Atmosphere::at(crate::daylight::INITIAL_MS);
    let a = crate::render::sky_camera_data(camera, 640, 400, atmosphere);
    let b = crate::render::sky::sky_camera_data_at_sample(camera, 640, 400, atmosphere, 17);
    assert_eq!(&a[..38], &b[..38]);
    assert_eq!(b[38], 17.);
    assert_eq!(b[36], atmosphere.reference_shadow_fade);
    assert_eq!(b[37], 1.);
    assert_eq!(a[39], b[39]);
}
#[test]
fn source_cloud_shader_validates_with_runtime_input_enabled() {
    let source = super::super::clouds::CloudPass::shader().replace(
        "const BG_REFERENCE_CLOUD_NOISE: bool = false;",
        "const BG_REFERENCE_CLOUD_NOISE: bool = true;",
    );
    let module = wgpu::naga::front::wgsl::parse_str(&source).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}

#[path = "gpu.rs"]
mod gpu;
#[path = "oracle.rs"]
mod oracle;

#[test]
fn missing_local_noise_is_a_reportable_input_error() {
    let path = std::env::temp_dir().join(format!("missing-bsl-noise-{}", std::process::id()));
    assert!(super::Noise::read(&path).is_err());
}

#[path = "render_tests.rs"]
mod render;
