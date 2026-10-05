#[test]
fn water_shader_validates_with_production_lighting_fog_and_reactivity() {
    let source = super::daylight::surface_shader(include_str!("../water.wgsl"));
    let module = wgpu::naga::front::wgsl::parse_str(&source).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}
