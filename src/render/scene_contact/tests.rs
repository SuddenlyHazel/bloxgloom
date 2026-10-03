//! Execute the actual contact and lighting WGSL, not a CPU approximation.
use wgpu::util::DeviceExt;

fn source() -> String {
    let structs = include_str!("../sun_shadow/shader.wgsl")
        .lines()
        .take(2)
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "{structs}\nvar<private> bg_shadow: BgShadow;\n{}\n{}\n{}\n{}",
        include_str!("../daylight/test_camera.wgsl"),
        include_str!("../scene_contact.wgsl"),
        include_str!("../daylight.wgsl"),
        r#"
@group(0) @binding(0) var<storage, read_write> result: array<vec4f>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3u) {
    bg_shadow.center = vec4f(0.0, 1.0, 0.0, 0.0);
    bg_shadow.contact_params = vec4f(1.0, 1.0, 0.0, 0.0);
    bg_shadow.contacts[0] = BgSceneContact(vec4f(-1.0, -1.0, 1.0, 1.0), vec4f(0.0, 0.0, 1.0, 0.6), vec4f(0.26, 1.0, 0.0, 0.0));
    var point = vec3f(0.0, 1.0, 0.0);
    var normal = vec3f(0.0, 1.0, 0.0);
    if id.x == 1u { point.x = 2.0; }
    if id.x == 2u { point.y = 0.0; }
    if id.x == 3u { normal = vec3f(1.0, 0.0, 0.0); }
    if id.x == 4u { bg_shadow.contact_params.x = 2.0; bg_shadow.contacts[1] = bg_shadow.contacts[0]; }
    if id.x == 5u { bg_shadow.center.x = 30.0; }
    if id.x == 6u { bg_shadow.contact_params.y = 0.0; }
    let visibility = bg_scene_contact(point, normal);
    if id.x < 7u { result[id.x] = vec4f(visibility); return; }
    var sky = 1.0;
    var glow = 0.0;
    var sun = vec4f(0.0, 1.0, 0.0, 1.0);
    if id.x == 8u || id.x == 9u { sky = 0.0; }
    if id.x == 9u || id.x == 10u { glow = 1.0; }
    if id.x == 11u { sun.y = -1.0; }
    let light = bg_surface_light(normal, sun, sky, glow*glow*vec3f(1.0,0.57,0.23), vec3f(0.0), vec3f(0.0), 1.0);
    result[id.x] = vec4f(bg_contact_light(light, normal, sun, sky, visibility), 1.0);
}
"#
    )
}

#[test]
fn production_contact_shader_validates() {
    let module = wgpu::naga::front::wgsl::parse_str(&source()).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}

#[test]
#[ignore = "requires a GPU adapter"]
fn gpu_contacts_are_bounded_and_preserve_direct_glow_and_cave_floor() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(source().into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let output = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &[0; 12 * 16],
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 12 * 16,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: output.as_entire_binding(),
        }],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(12, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, 12 * 16);
    queue.submit([encoder.finish()]);
    let slice = readback.slice(..);
    slice.map_async(wgpu::MapMode::Read, |result| result.unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let bytes = slice.get_mapped_range().unwrap();
    let rows: &[[f32; 4]] = bytemuck::cast_slice(&bytes);
    assert!((rows[0][0] - 0.74).abs() < 1e-6);
    for row in [1, 2, 3, 5, 6] {
        assert_eq!(rows[row][0], 1.0);
    }
    assert_eq!(rows[0], rows[4], "overlapping contacts stacked");
    let vector = |i: usize| glam::Vec3::from_slice(&rows[i][..3]);
    let floor = glam::Vec3::new(0.012, 0.015, 0.022);
    let glow = glam::Vec3::new(1.0, 0.57, 0.23);
    assert!(vector(8).distance(floor) < 1e-6);
    assert!((vector(9) - floor).distance(glow) < 1e-6);
    assert!((vector(10) - vector(7)).distance(glow) < 1e-6);
    assert!((vector(7) - vector(11)).distance(glam::Vec3::new(0.72, 0.67, 0.56)) < 1e-6);
}
