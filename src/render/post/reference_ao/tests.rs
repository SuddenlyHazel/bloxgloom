use super::*;
#[path = "oracle.rs"]
mod oracle;
#[path = "render.rs"]
mod render;

#[test]
fn source_default_ao_shader_validates() {
    let source = shader();
    let module = wgpu::naga::front::wgsl::parse_str(&source).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}

#[test]
fn gpu_source_default_ao_matches_independent_depth_and_composite_equations() {
    render::verify();
}

#[test]
fn gpu_reference_ao_fog_order_preserves_scattering_and_full_color_energy() {
    let adapter =
        pollster::block_on(wgpu::Instance::default().request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let source = format!(
        "{}\n{}",
        shader(),
        r#"
@group(1) @binding(0) var<storage,read_write> output:array<vec4f>;
@compute @workgroup_size(1) fn verify(@builtin(global_invocation_id) id:vec3u) {
    let ao=array<f32,4>(0.0,0.25,0.75,1.0)[id.x];
    let fog=array<f32,4>(0.0,0.25,0.8,1.0)[id.y];
    let surface=vec3f(2.0,0.5,0.125);let air=vec3f(0.1,0.3,0.8);
    let current=mix(surface,air,fog);
    let blend=bg_ao_fog_order(ao,air*fog);
    output[id.y*4u+id.x]=vec4f(current*blend.a+blend.rgb,0.75);
}
"#
    );
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("source AO before fog algebra"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &module,
        entry_point: Some("verify"),
        compilation_options: Default::default(),
        cache: None,
    });
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 256,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 256,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(1),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: output.as_entire_binding(),
        }],
    });
    let empty = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &empty, &[]);
        pass.set_bind_group(1, &group, &[]);
        pass.dispatch_workgroups(4, 4, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &read, 0, 256);
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    read.slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let data = read.slice(..).get_mapped_range().unwrap();
    let rows: &[[f32; 4]] = bytemuck::cast_slice(&data);
    for (y, fog) in [0.0, 0.25, 0.8, 1.0].into_iter().enumerate() {
        for (x, ao) in [0.0, 0.25, 0.75, 1.0].into_iter().enumerate() {
            for (c, (surface, air)) in [(2.0, 0.1), (0.5, 0.3), (0.125, 0.8)]
                .into_iter()
                .enumerate()
            {
                let expected = surface * ao * (1.0 - fog) + air * fog;
                assert!(
                    (f64::from(rows[y * 4 + x][c]) - expected).abs() < 2e-7,
                    "full source color AO and fog order row{} channel{c}",
                    y * 4 + x
                );
            }
        }
    }
}
