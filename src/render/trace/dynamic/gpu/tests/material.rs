use super::*;

#[test]
fn gpu_pickup_labpbr_categories_use_exact_base_texels_with_continuous_ra() {
    let (device, queue) = device();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("actual pickup companion sampling"),
        source: wgpu::ShaderSource::Wgsl(
            format!(
                "{}\n{}\n{}",
                include_str!("../../../../material/pbr.wgsl"),
                include_str!("../../material_channels.wgsl"),
                r#"
@group(0) @binding(0) var ray_specular:texture_2d_array<f32>;
@group(0) @binding(1) var ray_sampler:sampler;
@group(0) @binding(2) var<storage,read_write> result:array<vec4f>;
@compute @workgroup_size(1) fn sample_pickup(@builtin(global_invocation_id) id:vec3u) {
 let i=id.x;let uv=vec2f(select(0.49,0.51,(i%2u)==1u),0.5);
 let value=dyn_catalog_channels(uv,select(0,1,i>=4u),i<2u||i>=4u);
 let pbr=bg_decode_pbr(value,vec3f(0.5),true,true);
 result[i*2u]=value;result[i*2u+1u]=vec4f(f32(pbr.preset_id),pbr.porosity,pbr.subsurface,pbr.metal);
}
"#
            )
            .into(),
        ),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: Some("sample_pickup"),
        compilation_options: Default::default(),
        cache: None,
    });
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 2,
            height: 1,
            depth_or_array_layers: 2,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
        &[
            64, 230, 64, 20, 192, 237, 254, 240, 64, 100, 64, 20, 192, 100, 65, 240,
        ],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(8),
            rows_per_image: Some(1),
        },
        texture.size(),
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let size = 6 * 2 * 16;
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: output.as_entire_binding(),
            },
        ],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(6, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, size);
    queue.submit([encoder.finish()]);
    let words = read(&device, &readback);
    let values: Vec<[f32; 4]> = words
        .chunks_exact(4)
        .map(|v| {
            [
                f32::from_bits(v[0]),
                f32::from_bits(v[1]),
                f32::from_bits(v[2]),
                f32::from_bits(v[3]),
            ]
        })
        .collect();
    for (case, rows) in values.chunks_exact(2).enumerate() {
        let right = case % 2 == 1;
        let blend = if right { 0.52 } else { 0.48 };
        assert!((rows[0][0] - (64.0 + 128.0 * blend) / 255.0).abs() < 0.002);
        assert!((rows[0][3] - (20.0 + 220.0 * blend) / 255.0).abs() < 0.002);
        if case < 2 {
            assert!((rows[0][1] * 255.0 - if right { 237.0 } else { 230.0 }).abs() < 0.001);
            assert!((rows[0][2] * 255.0 - if right { 254.0 } else { 64.0 }).abs() < 0.001);
            assert_eq!(rows[1], [if right { 237.0 } else { 230.0 }, 0.0, 0.0, 1.0]);
        } else if case < 4 {
            assert!((rows[0][1] * 255.0 - (230.0 + 7.0 * blend)).abs() < 0.3);
            assert!((rows[0][2] * 255.0 - (64.0 + 190.0 * blend)).abs() < 0.8);
        } else {
            let expected = [0.0, if right { 0.0 } else { 1.0 }, 0.0, 0.0];
            for (actual, expected) in rows[1].iter().zip(expected) {
                assert!((*actual - expected).abs() < 1e-6);
            }
        }
    }
}
