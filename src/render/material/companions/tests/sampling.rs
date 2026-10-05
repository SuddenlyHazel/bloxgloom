//! Production material sampling must never interpolate categorical LabPBR bytes.
use wgpu::util::DeviceExt;

const CASES: u32 = 13;
fn source() -> String {
    let helper = include_str!("../../companions.wgsl")
        .split("// Legacy RGB")
        .next()
        .unwrap();
    format!(
        "{}\n{helper}\n{}",
        include_str!("../../pbr.wgsl"),
        r#"
@group(0) @binding(0) var<storage, read_write> result: array<vec4f>;
@group(0) @binding(1) var material_specular: texture_2d_array<f32>;
@group(0) @binding(2) var material_sampler: sampler;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3u) {
    let positions = array<vec2f,4>(vec2f(0.25,0.25),vec2f(0.75,0.25),vec2f(0.25,0.75),vec2f(0.75,0.75));
    var uv = positions[id.x % 4u];
    if id.x >= 8u { uv -= vec2f(1.0); }
    let lod = min(f32(id.x / 4u)*0.5,1.0);
    let filtered = textureSampleLevel(material_specular,material_sampler,uv,0,lod);
    let lab = id.x < 12u;
    let channels = bg_material_channels(filtered,uv,0,lab);
    let p = bg_decode_pbr(channels,vec3f(0.5),lab,true);
    result[id.x*2u] = channels;
    result[id.x*2u+1u] = vec4f(p.roughness,p.metal,p.subsurface,p.emission);
}
"#
    )
}

#[test]
fn material_sampling_shader_validates_without_a_gpu() {
    let module = wgpu::naga::front::wgsl::parse_str(&source()).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}

#[test]
fn gpu_material_categories_survive_minification_fractional_lod_and_repeat() {
    let instance = wgpu::Instance::default();
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
        eprintln!("material sampling test skipped: no adapter");
        return;
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("categorical material sampling regression"),
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
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        },
        mip_level_count: 2,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    for (level, pixels) in [
        &[
            0, 231, 255, 254, 255, 10, 19, 0, 255, 10, 19, 0, 0, 231, 255, 254,
        ][..],
        &[128, 120, 137, 127][..],
    ]
    .into_iter()
    .enumerate()
    {
        let size = 2 >> level;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: level as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size * 4),
                rows_per_image: Some(size),
            },
            wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
        );
    }
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        ..Default::default()
    });
    let output = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &[0; CASES as usize * 32],
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(CASES * 32),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: output.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(CASES, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, u64::from(CASES * 32));
    queue.submit([encoder.finish()]);
    let slice = readback.slice(..);
    slice.map_async(wgpu::MapMode::Read, |result| result.unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let bytes = slice.get_mapped_range().unwrap();
    let rows: &[[f32; 4]] = bytemuck::cast_slice(&bytes);
    for case in 0..12 {
        let metal = matches!(case % 4, 0 | 3);
        let channels = rows[case * 2];
        assert!((channels[1] * 255.0 - if metal { 231.0 } else { 10.0 }).abs() < 0.001);
        assert!((channels[2] * 255.0 - if metal { 255.0 } else { 19.0 }).abs() < 0.001);
        assert_eq!(rows[case * 2 + 1][1], f32::from(metal));
        assert_eq!(
            rows[case * 2 + 1][2],
            0.0,
            "porosity cannot become SSS; metals have no SSS"
        );
        if case >= 8 {
            assert!(
                (channels[0] - 128.0 / 255.0).abs() < 0.001,
                "smoothness stays filtered"
            );
            assert!(
                (channels[3] - 127.0 / 255.0).abs() < 0.001,
                "emission stays filtered"
            );
        }
    }
    assert!(
        (rows[24][1] * 255.0 - 120.0).abs() < 0.001,
        "legacy PBR stays filtered"
    );
}
