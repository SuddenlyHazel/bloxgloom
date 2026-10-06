use super::*;

#[test]
fn gpu_primitive_cosmetics_match_native_part_palette_and_tint() {
    let (device, queue) = device();
    let mut gpu = DynamicGpu::new(&device);
    let base = [0.2, 0.3, 0.4];
    let vertices = (0..6)
        .flat_map(|part| {
            [[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [0.0, 1.0, 0.0]].map(|p| {
                let mut v = vertex(p, [0.0; 2]);
                v.part = part;
                v
            })
        })
        .collect();
    let triangles = (0..6)
        .map(|part| ([part * 3, part * 3 + 1, part * 3 + 2], 0, part))
        .collect();
    let asset = DynamicAsset::build(vertices, triangles, vec![Material::flat(base)], vec![]);
    let mut instance = DynamicInstance::rigid(asset, Mat4::IDENTITY, 1.0);
    instance.deformation = Deformation::Primitive;
    instance.cosmetics = [3, 7, 5, 0];
    instance.tint = Vec3::new(0.6, 0.8, 1.0);
    let targets = DynamicTargets {
        instances: vec![instance],
    };
    ready(&device, &queue, &mut gpu, &targets);
    let colors = include_str!("../../intersection.wgsl")
        .split("// Alpha is tested")
        .next()
        .unwrap();
    let source = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        DEFORMATION_SHADER,
        crate::render::avatars::ray_palettes(crate::content::catalog()),
        super::super::super::MATERIAL_SHADER,
        include_str!("../../buffers.wgsl").replace("@group(2)", "@group(0)"),
        colors,
        r#"
struct Metadata {flags:u32,layer:u32};
@group(1) @binding(0) var ray_albedo:texture_2d_array<f32>;
@group(1) @binding(1) var ray_sampler:sampler;
@group(1) @binding(5) var<storage,read> ray_materials:array<Metadata>;
@group(1) @binding(2) var<storage,read_write> result:array<vec4f>;
@compute @workgroup_size(1) fn sample_appearance(@builtin(global_invocation_id) id:vec3u) {
 result[id.x]=dyn_color(id.x,vec2f(0.5));
}
"#
    );
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("actual packed primitive ray cosmetics"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 6 * 16,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 6 * 16,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let sampler = device.create_sampler(&Default::default());
    let metadata = buffer(
        &device,
        "appearance oracle unused catalog",
        &[0, 0],
        wgpu::BufferUsages::STORAGE,
    );
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&gpu.layout), Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("sample_appearance"),
        compilation_options: Default::default(),
        cache: None,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
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
            wgpu::BindGroupEntry {
                binding: 5,
                resource: metadata.as_entire_binding(),
            },
        ],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    gpu.encode(&mut encoder);
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &gpu.group, &[]);
        pass.set_bind_group(1, &group, &[]);
        pass.dispatch_workgroups(6, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, 6 * 16);
    queue.submit([encoder.finish()]);
    let words = read(&device, &readback);
    let catalog = crate::content::catalog();
    let pants = catalog.appearance_color(2, 5).unwrap();
    let expected = [
        catalog.appearance_color(0, 3).unwrap(),
        catalog.appearance_color(1, 7).unwrap(),
        pants,
        std::array::from_fn(|i| pants[i] * 0.35 + [0.10, 0.08, 0.07][i] * 0.65),
        [0.025, 0.035, 0.045],
        base,
    ];
    for (part, rgba) in words.chunks_exact(4).enumerate() {
        for (channel, actual) in rgba.iter().take(3).enumerate() {
            assert!(
                (f32::from_bits(*actual) - expected[part][channel] * [0.6, 0.8, 1.0][channel])
                    .abs()
                    < 1e-6,
                "part {part}, channel {channel}"
            );
        }
        assert_eq!(f32::from_bits(rgba[3]), 1.0);
    }
}
