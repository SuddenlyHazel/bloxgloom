#[test]
fn atmospheric_sky_shader_validates_without_a_gpu() {
    let module =
        wgpu::naga::front::wgsl::parse_str(super::SKY_SHADER).expect("valid atmospheric sky WGSL");
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .expect("valid atmospheric sky shader module");
}

#[test]
fn weather_fog_shaders_validate_without_a_gpu() {
    let terrain = format!(
        "{}\nfn bg_vertex(input: BgVertex, layer: u32) -> BgVertex {{ return input; }}\nfn bg_surface(input: BgSurface, layer: u32) -> BgSurface {{ return input; }}\n{}",
        crate::render::custom::TYPES,
        include_str!("../pipeline.wgsl")
    );
    for source in [&terrain, include_str!("../avatars/character.wgsl")] {
        let module = wgpu::naga::front::wgsl::parse_str(&crate::render::fog::shader(source))
            .expect("valid weather fog WGSL");
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("valid weather fog shader module");
    }
}
