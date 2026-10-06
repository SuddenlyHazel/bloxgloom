#[test]
fn gpu_source_handlight_matches_distance_soft_addition_and_hand_equations() {
    use wgpu::util::DeviceExt;
    let adapter =
        pollster::block_on(wgpu::Instance::default().request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let source = format!(
        "{}\n{}",
        super::super::HANDLIGHT_SHADER,
        r#"
@group(0) @binding(0) var<storage,read_write> out:array<vec4f>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id:vec3u) {
 let i=id.x;let held=f32(i%4u)*5.0;let raw=f32((i/4u)%4u)/4.0;
 let relative=vec3f(f32(i/16u)*2.0-2.0,1.0,-0.5);let eye=vec3f(0.2,-0.4,0.7);
 out[i]=vec4f(bg_bsl_handlight(vec2f(raw,0.35),relative,eye,held),bg_bsl_handlight_hand(vec2f(raw,0.35),held));
}"#
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
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &[0; 64 * 16],
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 64 * 16,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(64, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&buffer, 0, &read, 0, 64 * 16);
    queue.submit([encoder.finish()]);
    read.slice(..)
        .map_async(wgpu::MapMode::Read, |r| r.unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let bytes = read.slice(..).get_mapped_range().unwrap();
    let rows: &[[f32; 4]] = bytemuck::cast_slice(&bytes);
    for (i, row) in rows.iter().enumerate() {
        let held = (i % 4) as f64 * 5.0;
        let raw = ((i / 4) % 4) as f64 / 4.0;
        let position = [
            (i / 16) as f64 * 2.0 - 2.0 + 0.2,
            1.0 - 0.4 + 0.5,
            -0.5 + 0.7,
        ];
        let distance = position.into_iter().map(|v| v * v).sum::<f64>().sqrt();
        let hand = ((held - 2.0 * distance) / 15.0).min(0.9333);
        let expected = if held == 0.0 {
            raw
        } else {
            ((raw * 32.0).exp2() + (hand * 32.0).exp2()).log2() / 32.0
        };
        for (actual, expected) in
            row.iter()
                .zip([expected, 0.35, raw.max((held / 15.0).min(0.9333)), 0.35])
        {
            assert!(
                (f64::from(*actual) - expected).abs() < 1e-6,
                "case{i} actual{row:?}"
            );
        }
    }
}

#[test]
fn authoritative_hand_payload_preserves_reference_sh_flag_and_enhanced_coefficients() {
    let mut original = [0.0f32; 80];
    for (i, v) in original.iter_mut().enumerate() {
        *v = i as f32;
    }
    let mut enhanced = original;
    super::configure_for(&mut enhanced, 15, [1.0, 2.0, 3.0], false);
    assert_eq!(enhanced, original);
    let mut reference = [0.0f32; 80];
    super::configure_for(&mut reference, 255, [1.0, f32::NAN, 3.0], true);
    assert_eq!(reference[59], 0.0, "SH remains disabled");
    assert_eq!(&reference[76..], [1.0, 0.0, 3.0, 15.0]);
    super::configure_for(&mut reference, 0, [0.0; 3], true);
    assert_eq!(&reference[76..], [0.0; 4]);
}

#[test]
fn gpu_actual_hand_camera_payload_matches_near_and_origin_relative_lod_receivers() {
    use wgpu::util::DeviceExt;
    let adapter =
        pollster::block_on(wgpu::Instance::default().request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let source = format!(
        "{}\n{}\n{}\n{}",
        super::super::HANDLIGHT_SHADER,
        super::super::LIGHTING_SHADER,
        include_str!("../camera.wgsl"),
        r#"
struct Camera {matrix:mat4x4f,sun:vec4f,horizon:vec4f,eye:vec4f,fog_range:vec4f,parallax:vec4f,sun_radiance:vec4f,sky_zenith:vec4f,ambient_lower:vec4f,ambient_upper:vec4f,cloud:vec4f,ambient_sh:array<vec4f,6>};
@group(0) @binding(0) var<uniform> camera:Camera;
@group(0) @binding(1) var<storage,read_write> output:array<vec4f>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id:vec3u) {
 let delta=vec3f(f32(id.x)*2.0-2.0,1.0,3.0);
 let near=bg_bsl_reference_lightmap(vec2f(0.2,0.5),camera.eye.xyz+delta);
 let lod=bg_bsl_reference_relative_lightmap(vec2f(0.2,0.5),delta);
 output[id.x]=vec4f(near.x,lod.x,near.y,camera.ambient_sh[0].w);
}"#
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
    for origin in [
        [0.0, 0.0, 0.0],
        [-2015.0, 30.0, 512.0],
        [32768.0, -64.0, -32768.0],
    ] {
        let mut packet = [0.0f32; 80];
        packet[24..27].copy_from_slice(&origin);
        super::configure_for(&mut packet, 15, [1.0, -2.0, 3.0], true);
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&packet),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let output = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: &[0; 4 * 16],
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        });
        let read = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 4 * 16,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: output.as_entire_binding(),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(4, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &read, 0, 4 * 16);
        queue.submit([encoder.finish()]);
        read.slice(..)
            .map_async(wgpu::MapMode::Read, |r| r.unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let bytes = read.slice(..).get_mapped_range().unwrap();
        let rows: &[[f32; 4]] = bytemuck::cast_slice(&bytes);
        for (i, row) in rows.iter().enumerate() {
            let p = [i as f64 * 2.0 - 1.0, -0.5, 6.0];
            let distance = p.into_iter().map(|v| v * v).sum::<f64>().sqrt();
            let hand = ((15.0 - 2.0 * distance) / 15.0).min(0.9333);
            let expected = ((0.2f64 * 32.0).exp2() + (hand * 32.0).exp2()).log2() / 32.0;
            assert!((f64::from(row[0]) - expected).abs() < 2e-6);
            assert_eq!(row[0], row[1], "near/LOD mismatch at{origin:?}");
            assert_eq!(row[2], 0.5);
            assert_eq!(row[3], 0.0, "heldlight must not enable SH");
        }
    }
}
