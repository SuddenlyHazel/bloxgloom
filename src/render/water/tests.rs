#[test]
fn water_shader_validates_with_production_lighting_fog_and_reactivity() {
    let source = super::shader(include_str!("../water.wgsl"));
    let module = wgpu::naga::front::wgsl::parse_str(&source).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}

#[test]
fn gpu_water_sun_visibility_preserves_indirect_and_sealed_cave_response() {
    use wgpu::util::DeviceExt;
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let source = super::shader(
        r#"
struct Camera { view_projection: mat4x4f, sun: vec4f, horizon: vec4f, eye: vec4f, fog_range: vec4f, parallax: vec4f, sun_radiance: vec4f, sky_zenith: vec4f, ambient_lower: vec4f, ambient_upper: vec4f };
@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var<storage,read_write> colors: array<vec4f>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id:vec3u) {
    let visibility = f32(id.x%2u);
    let sky = select(1.0,0.0,id.x>=2u);
    let result = bg_water_surface(vec4f(0.1,0.25,0.3,0.5),vec3f(0.0,1.0,0.0),sky,0.0,
        vec3f(0.0),vec3f(0.0,-10.0,0.0),true,0.0,visibility);
    colors[id.x] = result.color;
}
"#,
    );
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("water visibility regression"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let mut atmosphere = crate::render::daylight::Atmosphere::at(crate::daylight::INITIAL_MS);
    atmosphere.sun = glam::Vec3::Y;
    let camera = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(
            &atmosphere.camera_data(glam::Mat4::IDENTITY, glam::Vec3::new(0.0, 10.0, 0.0)),
        ),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let group = |index, buffer: &wgpu::Buffer| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(index),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        })
    };
    let camera_group = group(0, &camera);
    let output_group = group(1, &output);
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &camera_group, &[]);
        pass.set_bind_group(1, &output_group, &[]);
        pass.dispatch_workgroups(4, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, 64);
    queue.submit([encoder.finish()]);
    let slice = readback.slice(..);
    slice.map_async(wgpu::MapMode::Read, |r| r.unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let bytes = slice.get_mapped_range().unwrap();
    let colors: &[[f32; 4]] = bytemuck::cast_slice(&bytes);
    for (channel, shadow) in colors[0][..3].iter().enumerate() {
        assert!(*shadow > 0.0, "water shadow removed sky fill");
        assert!(
            colors[1][channel] > colors[0][channel],
            "sun visibility did not gate direct water light"
        );
        assert!(
            (colors[2][channel] - colors[3][channel]).abs() < 0.00001,
            "sun visibility illuminated sealed water"
        );
        assert!(
            colors[2][channel] < 0.01,
            "sealed water gained exterior reflection"
        );
    }
}
