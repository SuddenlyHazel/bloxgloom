#[test]
fn atmospheric_sky_shader_validates_without_a_gpu() {
    for source in [super::shader_source(), super::clouds::CloudPass::shader()] {
        let module =
            wgpu::naga::front::wgsl::parse_str(&source).expect("valid atmospheric sky WGSL");
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("valid atmospheric sky shader module");
    }
}

#[test]
fn weather_fog_shaders_validate_without_a_gpu() {
    let terrain = format!(
        "{}\n{}\n{}\n{}\n{}\n{}\n{}\nfn bg_vertex(input: BgVertex, layer: u32) -> BgVertex {{ return input; }}\nfn bg_surface(input: BgSurface, layer: u32) -> BgSurface {{ return input; }}\n{}",
        crate::render::custom::TYPES,
        include_str!("../material/relief.wgsl"),
        include_str!("../material/pbr.wgsl"),
        include_str!("../material/parallax.wgsl"),
        include_str!("../material/companions.wgsl"),
        include_str!("../material/foliage.wgsl"),
        include_str!("../material/foliage_optics.wgsl"),
        include_str!("../pipeline.wgsl")
    );
    let character = crate::render::avatars::character_shader(crate::content::catalog());
    let terrain = crate::render::daylight::shader(&terrain);
    for source in [&terrain, &character] {
        let module = wgpu::naga::front::wgsl::parse_str(source).expect("valid weather fog WGSL");
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("valid weather fog shader module");
    }
}

#[path = "transport_tests.rs"]
mod transport;

#[path = "render_tests.rs"]
mod render;

#[test]
fn sky_uniform_keeps_cloud_coverage_separate_from_precipitation_and_lunar_phase() {
    let atmosphere = crate::render::weather::Presentation::new(0.8, 0.25, [0.0; 2], 1.0, 0.0, 0.0)
        .atmosphere(crate::render::daylight::Atmosphere::at(
            crate::daylight::CYCLE_MS * 4,
        ));
    let camera = crate::render::Camera {
        position: glam::Vec3::new(1., 2., 3.),
        yaw: 0.,
        pitch: 0.,
        fov_y_radians: 1.,
    };
    let data = super::sky_camera_data(camera, 641, 359, atmosphere);
    assert_eq!(std::mem::size_of_val(&data), 160);
    assert_eq!(data[3], 0.8);
    assert_eq!(&data[32..36], &[0.25, 0.5, 4.0, 0.0]);
    assert_eq!(&data[28..31], &[1., 2., 3.]);
}

#[test]
fn primary_transport_flag_packs_into_unused_sky_sun_component() {
    let mut atmosphere = crate::render::daylight::Atmosphere::at(crate::daylight::INITIAL_MS);
    atmosphere.scene_transport = true;
    let camera = crate::render::Camera {
        position: glam::Vec3::ZERO,
        yaw: 0.,
        pitch: 0.,
        fov_y_radians: 1.,
    };
    let data = super::sky_camera_data(camera, 640, 360, atmosphere);
    assert_eq!(data[15], 1.0);
    assert_eq!(&data[12..15], &atmosphere.sun.to_array());
    assert_eq!(&data[32..36], &[0.0, 1.0, 0.0, 0.0]);
}
