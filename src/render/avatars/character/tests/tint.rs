//! Check the production WGSL tint functions against the byte-exact CPU contract.
use wgpu::util::DeviceExt;
#[test]
fn gpu_native_hair_color_preserves_linear_shading_alpha_and_fixed_accessories() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let source = include_str!("../../character.wgsl");
    let functions = &source
        [source.find("fn srgb_to_linear").unwrap()..source.find("fn character_albedo").unwrap()];
    let source = format!(
        "{functions}\n@group(0) @binding(0) var<storage,read_write> result:array<vec4f>; @compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id:vec3u) {{ result[id.x]=vec4f(srgb_to_linear(vec3f(f32(id.x))/255.0),1.0); result[id.x+256u]=shade_hair(vec4f(0.1,0.3,0.7,0.45),vec3f(f32(id.x)),false); result[id.x+512u]=shade_hair(vec4f(0.1,0.3,0.7,0.45),vec3f(f32(id.x)),true); }}"
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
        contents: &[0; 768 * 16],
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 768 * 16,
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
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, 768 * 16);
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
    for byte in 0..256 {
        let rgb = byte as f32 / 255.0;
        let linear = if rgb <= 0.04045 {
            rgb / 12.92
        } else {
            ((rgb + 0.055) / 1.055).powf(2.4)
        };
        for (channel, neutral) in [0.1, 0.3, 0.7].into_iter().enumerate() {
            assert!((floats[(byte + 256) * 4 + channel] - neutral * linear).abs() < 0.00001);
            assert!(
                (floats[(byte + 512) * 4 + channel] - neutral).abs() < 0.00001,
                "fixed accessory was recolored"
            );
        }
        assert!((floats[(byte + 256) * 4 + 3] - 0.45).abs() < 0.00001);
        assert!((floats[(byte + 512) * 4 + 3] - 0.45).abs() < 0.00001);
    }
    for shade in 0..256 {
        for channel in 0..3 {
            let srgb = shade as f32 / 255.0;
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
