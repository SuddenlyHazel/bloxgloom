//! Independent adapter/storage gate before the production sampler emits raw data.
use wgpu::util::DeviceExt;

const SIZE: u32 = 8;
const COMMON: &str = include_str!("storage/common.wgsl");
const OFF: &str = include_str!("storage/off.wgsl");
const ON: &str = include_str!("storage/on.wgsl");
const LOAD: &str = include_str!("storage/load.wgsl");
const SENTINEL: [f32; 4] = [42.0, -42.0, 0.5, -1.0];

fn texture(
    device: &wgpu::Device,
    width: u32,
    layers: u32,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("frozen first-water storage gate"),
        size: wgpu::Extent3d {
            width,
            height: SIZE,
            depth_or_array_layers: layers,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    })
}

fn pipeline(
    device: &wgpu::Device,
    source: &str,
    layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
    count: usize,
) -> wgpu::RenderPipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("frozen first-water storage gate"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    let targets = vec![
        Some(wgpu::ColorTargetState {
            format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        });
        count
    ];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &targets,
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn read(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    let size = texture.size();
    let pixel_bytes = texture.format().block_copy_size(None).unwrap();
    let row_bytes = size.width * pixel_bytes;
    let stride = row_bytes.div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("storage gate readback"),
        size: u64::from(stride * size.height * size.depth_or_array_layers),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(size.height),
            },
        },
        size,
    );
    let submitted = queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submitted),
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .unwrap();
    rx.recv_timeout(std::time::Duration::from_secs(30))
        .unwrap()
        .unwrap();
    let mapped = buffer.slice(..).get_mapped_range().unwrap();
    let bytes = mapped
        .chunks_exact(stride as usize)
        .flat_map(|row| row[..row_bytes as usize].iter().copied())
        .collect();
    drop(mapped);
    buffer.unmap();
    bytes
}

fn expected(frame: u32, x: u32, y: u32, layer: u32) -> [f32; 4] {
    let invalid = x == 7 || (frame != 0 && y & 1 != 0);
    match layer {
        0 => [
            if x & 1 != 0 { -0.0009765625 } else { 14721.34 },
            y as f32 * 0.125,
            -((x + 1) as f32) * 0.0625,
            if frame == 0 { 1.0 } else { -0.75 },
        ],
        1 => [
            (x + y * 8 + frame * 64) as f32 * 0.5,
            1.0 / 1_048_576.0,
            -16384.0,
            if invalid { -1.0 } else { 0.125 },
        ],
        2 if invalid => [0.0, 0.0, -1.0, -1.0],
        2 => [
            x as f32 * 0.125 - 0.5,
            y as f32 * 0.125 - 0.5,
            if x & 1 != 0 { 16383.0 } else { 0.0 },
            (y + frame * 8) as f32,
        ],
        _ => unreachable!(),
    }
}

fn assert_packet(bytes: &[u8], pixel: usize, expected: [f32; 4]) {
    let actual: [u32; 4] = std::array::from_fn(|channel| {
        let offset = (pixel * 4 + channel) * 4;
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    });
    assert_eq!(
        actual,
        expected.map(f32::to_bits),
        "float32 storage packet pixel{pixel}"
    );
}

#[test]
fn gpu_first_water_storage_gate_preserves_float32_packets_tiles_and_load_barriers() {
    assert!(!OFF.contains("texture_storage") && !OFF.contains("textureStore"));
    let adapter =
        pollster::block_on(wgpu::Instance::default().request_adapter(&Default::default()))
            .expect("storage gate adapter");
    assert!(
        adapter
            .get_downlevel_capabilities()
            .flags
            .contains(wgpu::DownlevelFlags::FRAGMENT_WRITABLE_STORAGE),
        "actual gate requires fragment storage support; unsupported adapters must retain fallback"
    );
    let format_features = adapter.get_texture_format_features(wgpu::TextureFormat::Rgba32Float);
    let raw_usage = wgpu::TextureUsages::STORAGE_BINDING
        | wgpu::TextureUsages::TEXTURE_BINDING
        | wgpu::TextureUsages::COPY_SRC
        | wgpu::TextureUsages::COPY_DST;
    assert!(format_features.allowed_usages.contains(raw_usage));
    assert!(
        format_features
            .flags
            .contains(wgpu::TextureFormatFeatureFlags::STORAGE_WRITE_ONLY)
    );
    let limits = crate::render::material::resources::required_limits(adapter.limits(), 1).unwrap();
    assert!(limits.max_storage_textures_per_shader_stage >= 1);
    assert!(limits.max_texture_array_layers >= 3);
    assert_eq!(limits.max_color_attachment_bytes_per_sample, 32);
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_limits: limits,
        ..Default::default()
    }))
    .unwrap();
    assert!(
        device.features().is_empty(),
        "gate requests no optional feature"
    );
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&[0u32; 4]),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let raw = texture(
        &device,
        SIZE,
        3,
        wgpu::TextureFormat::Rgba32Float,
        raw_usage,
    );
    let raw_view = raw.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    queue.write_texture(
        raw.as_image_copy(),
        bytemuck::cast_slice(&vec![SENTINEL; (SIZE * SIZE * 3) as usize]),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(SIZE * 16),
            rows_per_image: Some(SIZE),
        },
        raw.size(),
    );
    let uniform_entry = wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };
    let storage_entry = wgpu::BindGroupLayoutEntry {
        binding: 1,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::StorageTexture {
            access: wgpu::StorageTextureAccess::WriteOnly,
            format: wgpu::TextureFormat::Rgba32Float,
            view_dimension: wgpu::TextureViewDimension::D2Array,
        },
        count: None,
    };
    let layouts = [vec![uniform_entry], vec![uniform_entry, storage_entry]].map(|entries| {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &entries,
        })
    });
    let groups: [wgpu::BindGroup; 2] = std::array::from_fn(|mode| {
        let mut entries = vec![wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform.as_entire_binding(),
        }];
        if mode == 1 {
            entries.push(wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&raw_view),
            });
        }
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layouts[mode],
            entries: &entries,
        })
    });
    let pipelines = [OFF, ON].map(|body| {
        pipeline(
            &device,
            &format!("{COMMON}\n{body}"),
            &layouts[usize::from(body == ON)],
            wgpu::TextureFormat::Rgba16Float,
            4,
        )
    });
    let outputs: [[wgpu::Texture; 4]; 2] = std::array::from_fn(|_| {
        std::array::from_fn(|_| {
            texture(
                &device,
                SIZE,
                1,
                wgpu::TextureFormat::Rgba16Float,
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            )
        })
    });
    let views = outputs.each_ref().map(|targets| {
        targets
            .each_ref()
            .map(|target| target.create_view(&Default::default()))
    });
    for frame in 0..2u32 {
        queue.write_buffer(&uniform, 0, bytemuck::cast_slice(&[frame, 0, 0, 0]));
        for (tile, (x, width)) in [(0, 3), (3, 3), (6, 2)].into_iter().enumerate() {
            for mode in 0..2 {
                let mut encoder = device.create_command_encoder(&Default::default());
                let attachments = views[mode].each_ref().map(|view| {
                    Some(wgpu::RenderPassColorAttachment {
                        view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: if tile == 0 {
                                wgpu::LoadOp::Clear(wgpu::Color::BLACK)
                            } else {
                                wgpu::LoadOp::Load
                            },
                            store: wgpu::StoreOp::Store,
                        },
                    })
                });
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        color_attachments: &attachments,
                        ..Default::default()
                    });
                    pass.set_pipeline(&pipelines[mode]);
                    pass.set_bind_group(0, &groups[mode], &[]);
                    pass.set_scissor_rect(x, 0, width, SIZE);
                    pass.draw(0..3, 0..1);
                }
                queue.submit([encoder.finish()]);
            }
            if frame == 0 && tile == 0 {
                let bytes = read(&device, &queue, &raw);
                for layer in 0..3 {
                    for y in 0..SIZE {
                        for x in 0..SIZE {
                            assert_packet(
                                &bytes,
                                ((layer * SIZE + y) * SIZE + x) as usize,
                                if x < 3 {
                                    expected(frame, x, y, layer)
                                } else {
                                    SENTINEL
                                },
                            );
                        }
                    }
                }
            }
        }
        for (attachment, (off, on)) in outputs[0].iter().zip(&outputs[1]).enumerate() {
            assert_eq!(
                read(&device, &queue, off),
                read(&device, &queue, on),
                "off/on MRT attachment{attachment}"
            );
        }
        let bytes = read(&device, &queue, &raw);
        for layer in 0..3 {
            for y in 0..SIZE {
                for x in 0..SIZE {
                    assert_packet(
                        &bytes,
                        ((layer * SIZE + y) * SIZE + x) as usize,
                        expected(frame, x, y, layer),
                    );
                }
            }
        }
        // A distinct pass samples the same array after prior tile submissions.
        let load_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let load_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &load_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&raw_view),
            }],
        });
        let load_pipeline = pipeline(
            &device,
            LOAD,
            &load_layout,
            wgpu::TextureFormat::Rgba32Float,
            1,
        );
        let loaded = texture(
            &device,
            SIZE * 3,
            1,
            wgpu::TextureFormat::Rgba32Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        );
        let view = loaded.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&load_pipeline);
            pass.set_bind_group(0, &load_group, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
        let loaded = read(&device, &queue, &loaded);
        for y in 0..SIZE {
            for layer in 0..3 {
                for x in 0..SIZE {
                    assert_packet(
                        &loaded,
                        (y * SIZE * 3 + layer * SIZE + x) as usize,
                        expected(frame, x, y, layer),
                    );
                }
            }
        }
    }
}
