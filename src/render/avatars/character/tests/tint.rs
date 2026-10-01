//! Check the production WGSL tint functions against the byte-exact CPU contract.
use wgpu::util::DeviceExt;
#[test]
fn gpu_iris_tint_matches_srgb_byte_math_including_half_ties() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let source = include_str!("../../character.wgsl");
    let functions =
        &source[source.find("fn srgb_to_linear").unwrap()..source.find("@fragment").unwrap()];
    let source = format!(
        "{functions}\n@group(0) @binding(0) var<storage,read_write> result:array<vec4f>; @compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id:vec3u) {{ result[id.x]=vec4f(tint_iris(vec3f(1.0,127.0,255.0),f32(id.x)),1.0); }}"
    );
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
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
    let output = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &[0; 256 * 16],
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 256 * 16,
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
        pass.dispatch_workgroups(256, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, 256 * 16);
    queue.submit(Some(encoder.finish()));
    let (tx, rx) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(10)),
        })
        .unwrap();
    rx.recv().unwrap().unwrap();
    let mapped = readback.slice(..).get_mapped_range().unwrap();
    let floats: &[f32] = bytemuck::cast_slice(&mapped);
    for shade in 0..256 {
        for (channel, base) in [1.0f32, 127.0, 255.0].into_iter().enumerate() {
            let v = if shade <= 128 {
                base * shade as f32 / 128.0
            } else {
                base + (255.0 - base) * (shade - 128) as f32 / 127.0
            };
            let srgb = (v + 0.5).floor() / 255.0;
            let expected = if srgb <= 0.04045 {
                srgb / 12.92
            } else {
                ((srgb + 0.055) / 1.055).powf(2.4)
            };
            assert!(
                (floats[shade * 4 + channel] - expected).abs() < 0.00001,
                "shade{shade}, channel{channel}"
            );
        }
    }
}
