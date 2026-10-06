use super::*;
mod appearance;
mod material;
fn vertex(p: [f32; 3], uv: [f32; 2]) -> Vertex {
    Vertex {
        position: p,
        normal: [0.0, 0.0, -1.0],
        uv,
        joints: [0, 1, 0, 0],
        weights: [0.25, 0.75, 0.0, 0.0],
        part: 0,
    }
}
fn device() -> (wgpu::Device, wgpu::Queue) {
    let adapter =
        pollster::block_on(wgpu::Instance::default().request_adapter(&Default::default())).unwrap();
    pollster::block_on(adapter.request_device(&Default::default())).unwrap()
}
fn read(device: &wgpu::Device, buffer: &wgpu::Buffer) -> Vec<u32> {
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(20)),
        })
        .unwrap();
    rx.recv().unwrap().unwrap();
    let mapped = buffer.slice(..).get_mapped_range().unwrap();
    let result = bytemuck::cast_slice::<u8, u32>(&mapped).to_vec();
    drop(mapped);
    buffer.unmap();
    result
}
fn ready(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    gpu: &mut DynamicGpu,
    targets: &DynamicTargets,
) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !gpu.set(device, queue, targets) {
        assert!(
            std::time::Instant::now() < deadline,
            "native asset upload must finish"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
fn geometry(device: &wgpu::Device, queue: &wgpu::Queue, gpu: &DynamicGpu) -> Vec<u32> {
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: gpu.geometry.size(),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    gpu.encode(&mut encoder);
    encoder.copy_buffer_to_buffer(&gpu.geometry, 0, &readback, 0, gpu.geometry.size());
    queue.submit(Some(encoder.finish()));
    read(device, &readback)
}
#[test]
fn gpu_current_skinning_refits_blas_tlas_and_removal_without_static_scene_rebuild() {
    let (device, queue) = device();
    let mut gpu = DynamicGpu::new(&device);
    let vertices = vec![
        vertex([-1.0, -1.0, 0.0], [0.0; 2]),
        vertex([-1.0, 1.0, 0.0], [0.0, 1.0]),
        vertex([1.0, -1.0, 0.0], [1.0, 0.0]),
    ];
    let triangles = (0..19).map(|_| ([0, 1, 2], 0, 0)).collect();
    let asset = DynamicAsset::build(
        vertices,
        triangles,
        vec![Material::flat([0.6, 0.2, 0.1])],
        Vec::new(),
    );
    let mut instance = DynamicInstance::rigid(asset, Mat4::IDENTITY, 0.0);
    instance.deformation = Deformation::Authored;
    instance.joints = vec![
        Mat4::IDENTITY,
        Mat4::from_scale_rotation_translation(
            Vec3::new(2.0, 0.5, 1.0),
            glam::Quat::from_rotation_y(0.4),
            Vec3::new(2.0, 1.0, 0.5),
        ),
    ];
    instance.parts = vec![([1.0, 1.0, 1.0, 0.0], true)];
    let mut second = instance.clone();
    second.world = Mat4::from_translation(Vec3::X * 10.0);
    let mut targets = DynamicTargets {
        instances: vec![instance, second],
    };
    ready(&device, &queue, &mut gpu, &targets);
    let first = geometry(&device, &queue, &gpu);
    assert!(gpu.lighting_changed());
    assert!(gpu.set(&device, &queue, &targets));
    assert!(!gpu.lighting_changed());
    assert_eq!(
        geometry(&device, &queue, &gpu),
        first,
        "unchanged packets retain initialized exact geometry without refitting"
    );
    let transform = targets.instances[0].joints[0] * 0.25 + targets.instances[0].joints[1] * 0.75;
    let point = transform.transform_point3(Vec3::new(-1.0, -1.0, 0.0));
    let actual = Vec3::new(
        f32::from_bits(first[8]),
        f32::from_bits(first[9]),
        f32::from_bits(first[10]),
    );
    assert!(actual.abs_diff_eq(point, 1e-5), "{actual:?} vs {point:?}");
    let normal = transform
        .inverse()
        .transpose()
        .transform_vector3(Vec3::NEG_Z)
        .normalize();
    let actual_normal = Vec3::new(
        f32::from_bits(first[20]),
        f32::from_bits(first[21]),
        f32::from_bits(first[22]),
    );
    assert!(actual_normal.abs_diff_eq(normal, 1e-5));
    let root = first[4] as usize;
    let high = Vec3::new(
        f32::from_bits(first[root + 4]),
        f32::from_bits(first[root + 5]),
        f32::from_bits(first[root + 6]),
    );
    assert!(
        high.x > 11.0,
        "TLAS must include second exact instance: {high:?}"
    );
    let source = gpu.source.clone();
    let bytes = gpu.source.size();
    targets.instances[0].world = Mat4::from_translation(Vec3::Z * 7.0);
    targets.instances[1].parts[0].1 = false;
    ready(&device, &queue, &mut gpu, &targets);
    // A static scene rebuild can suppress trace submission for several frames.
    // Repeating the same current pose must retain its pending GPU deformation.
    assert!(gpu.set(&device, &queue, &targets));
    assert!(gpu.set(&device, &queue, &targets));
    assert!(gpu.lighting_changed());
    let moved = geometry(&device, &queue, &gpu);
    assert_eq!(source, gpu.source);
    assert_eq!(bytes, gpu.source.size());
    assert!((f32::from_bits(moved[10]) - point.z - 7.0).abs() < 1e-5);
    let root = moved[4] as usize;
    assert!(
        f32::from_bits(moved[root + 4]) < 10.0,
        "hidden second actor must leave TLAS bounds"
    );
    assert!(gpu.set(&device, &queue, &targets));
    assert!(!gpu.lighting_changed());
    assert!(gpu.set(&device, &queue, &DynamicTargets::default()));
    let empty = geometry(&device, &queue, &gpu);
    assert_eq!(&empty[..4], &[0, 0, 0, 0]);
}

#[test]
fn gpu_native_alpha_tints_backfaces_static_ties_and_next_frame_motion() {
    let (device, queue) = device();
    let mut gpu = DynamicGpu::new(&device);
    let vertices = vec![
        vertex([-1.0, -1.0, 0.0], [0.0, 0.0]),
        vertex([-1.0, 1.0, 0.0], [0.0, 1.0]),
        vertex([1.0, -1.0, 0.0], [1.0, 0.0]),
        vertex([1.0, 1.0, 0.0], [1.0, 1.0]),
    ];
    let material = Material {
        kind: 1,
        base: [1.0; 4],
        image: Some(0),
        wrap: [0; 2],
        alpha_cutoff: 0.5,
        double_sided: false,
        surface: 0,
        group: 0,
        catalog_layer: 0,
    };
    let asset = DynamicAsset::build(
        vertices,
        vec![([0, 1, 2], 0, 0), ([2, 1, 3], 0, 0)],
        vec![material],
        vec![Image {
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 0, 0, 255, 0, 255].into(),
        }],
    );
    let mut target = DynamicInstance::rigid(asset, Mat4::IDENTITY, 1.0);
    target.parts = vec![([0.25, 0.5, 0.75, 1.0], true)];
    let mut targets = DynamicTargets {
        instances: vec![target],
    };
    ready(&device, &queue, &mut gpu, &targets);
    let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let texture = device.create_texture_with_data(
        &queue,
        &wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        &[255; 4],
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let sampler = device.create_sampler(&Default::default());
    let metadata = buffer(
        &device,
        "test catalog metadata",
        &[0, 0],
        wgpu::BufferUsages::STORAGE,
    );
    let materials = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &material_layout,
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
                binding: 5,
                resource: metadata.as_entire_binding(),
            },
        ],
    });
    let fragment = include_str!("../intersection.wgsl")
        .split("fn dynamic_ray_surface")
        .next()
        .unwrap();
    let source = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        DEFORMATION_SHADER,
        crate::render::avatars::ray_palettes(crate::content::catalog()),
        super::super::MATERIAL_SHADER,
        include_str!("../buffers.wgsl"),
        r#"
struct RayHit {distance:f32,triangle:u32,uv:vec2f,normal:vec3f};
struct Metadata {flags:u32,layer:u32};
@group(1) @binding(0) var ray_albedo:texture_2d_array<f32>;
@group(1) @binding(1) var ray_sampler:sampler;
@group(1) @binding(5) var<storage,read> ray_materials:array<Metadata>;
@vertex fn vs_main(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
 let p=array<vec2f,3>(vec2f(-1.0,-1.0),vec2f(3.0,-1.0),vec2f(-1.0,3.0));return vec4f(p[i],0.0,1.0);
}
@fragment fn fs_main(@builtin(position) pixel:vec4f)->@location(0) vec4f {
 let i=u32(pixel.x);let x=select(-0.5,0.5,i>0u);
 var origin=vec3f(x,0.0,-3.0);var direction=vec3f(0.0,0.0,1.0);
 if i==3u {origin.z=3.0;direction.z=-1.0;}
 var current=RayHit(100.0,0xffffffffu,vec2f(0.0),vec3f(0.0));
 if i==2u {current=RayHit(3.0,1u,vec2f(0.0),vec3f(0.0));}
 if i>=5u {origin=vec3f(0.0);direction=vec3f(0.0,0.0,1.0);}
 var hit=dynamic_ray_cast(origin,direction,100.0,current);
 if i==4u||i==6u {hit=dynamic_ray_cast_primary(origin,direction,100.0,current);}
 if hit.triangle==0xffffffffu {return vec4f(-1.0,-1.0,-1.0,100.0);}
 if (hit.triangle&0x80000000u)==0u {return vec4f(9.0,9.0,9.0,hit.distance);}
 return vec4f(dyn_color(hit.triangle&0x7fffffffu,hit.uv).rgb,hit.distance);
}
"#,
        fragment
    );
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("actual dynamic native target intersections"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let empty = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&empty), Some(&material_layout), Some(&gpu.layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba32Float,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    let output = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 7,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 256,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let render = |gpu: &DynamicGpu| {
        let view = output.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        gpu.encode(&mut encoder);
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                timestamp_writes: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(1, &materials, &[]);
            pass.set_bind_group(2, &gpu.group, &[]);
            pass.draw(0..3, 0..1);
        }
        encoder.copy_texture_to_buffer(
            output.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(1),
                },
            },
            output.size(),
        );
        queue.submit(Some(encoder.finish()));
        let words = read(&device, &readback);
        words[..28]
            .iter()
            .map(|v| f32::from_bits(*v))
            .collect::<Vec<_>>()
    };
    let first = render(&gpu);
    assert_eq!(&first[..4], &[-1.0, -1.0, -1.0, 100.0]);
    assert_eq!(&first[4..7], &[0.25, 0.5, 0.75]);
    assert!((first[7] - 3.0).abs() < 1e-5);
    assert_eq!(&first[8..12], &[9.0, 9.0, 9.0, 3.0]);
    assert_eq!(&first[12..16], &[-1.0, -1.0, -1.0, 100.0]);
    targets.instances[0].world = Mat4::from_translation(Vec3::Z * 2.0);
    targets.instances[0].skip_primary = true;
    ready(&device, &queue, &mut gpu, &targets);
    let moved = render(&gpu);
    assert!((moved[7] - 5.0).abs() < 1e-5);
    assert_eq!(
        &moved[16..20],
        &[-1.0, -1.0, -1.0, 100.0],
        "primary excludes owner; secondary still hits exact world body"
    );
    let mut head_vertices = Vec::new();
    let mut head_triangles = Vec::new();
    for axis in 0..3 {
        for side in [-1.0, 1.0] {
            let base = head_vertices.len() as u32;
            let u = (axis + 1) % 3;
            let v = (axis + 2) % 3;
            for (du, dv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let mut p = [0.0; 3];
                p[axis] = side;
                p[u] = du;
                p[v] = dv;
                head_vertices.push(vertex(p, [0.0; 2]));
            }
            head_triangles.extend([
                ([base, base + 1, base + 2], 0, 0),
                ([base, base + 2, base + 3], 0, 0),
            ]);
        }
    }
    let mut material = Material::flat([0.2, 0.3, 0.4]);
    material.double_sided = true;
    let head = DynamicAsset::build(head_vertices, head_triangles, vec![material], Vec::new());
    let mut owner_head = DynamicInstance::rigid(head, Mat4::IDENTITY, 1.0);
    owner_head.skip_primary = true;
    targets.instances.push(owner_head);
    ready(&device, &queue, &mut gpu, &targets);
    let inside = render(&gpu);
    assert!(
        (inside[23] - 1.0).abs() < 1e-5,
        "secondary ray from inside owner head retains world body"
    );
    assert_eq!(
        &inside[24..28],
        &[-1.0, -1.0, -1.0, 100.0],
        "camera primary from inside head ignores only owner body"
    );
    assert!(gpu.set(&device, &queue, &DynamicTargets::default()));
    let empty = render(&gpu);
    assert_eq!(&empty[4..8], &[-1.0, -1.0, -1.0, 100.0]);
}
