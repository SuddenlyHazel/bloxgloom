use glam::{Mat4, Vec3, Vec4};
use wgpu::util::DeviceExt;

const PBR: &str = include_str!("../../material/pbr.wgsl");
const HELPERS: &str = include_str!("../denoise.wgsl");
const FILTER: &str = include_str!("../filter.wgsl");
const COMPOSITE: &str = include_str!("../composite.wgsl");

#[test]
fn denoising_shader_modules_validate() {
    for source in [
        format!("{PBR}\n{HELPERS}\n{FILTER}\n{COMPOSITE}"),
        format!("{HELPERS}\n{HISTORY_FIXTURE}"),
    ] {
        let module = wgpu::naga::front::wgsl::parse_str(&source).expect("denoising WGSL parses");
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("denoising WGSL validates");
    }
}

const HISTORY_FIXTURE: &str = r#"
struct Output { @builtin(position) position:vec4f };
@vertex fn vs_main(@builtin(vertex_index) i:u32)->Output {
 let xy=vec2f(f32((i<<1u)&2u),f32(i&2u));return Output(vec4f(xy*2.0-1.0,0.0,1.0));
}
@fragment fn fs_main(@builtin(position) p:vec4f)->@location(0) vec4f {
 let n=vec3f(0.0,0.0,1.0);let g=vec4f(0.0,0.0,0.8,8.0);
 let position=vec3f(3.0,4.0,12.0);let expected=length(position);
 var accepted=false;
 switch u32(p.x) {
  case 0u:{accepted=ray_history_compatible(13.0,expected,g,n,0.8,false,0.0);}
  // Clip W is 12, but the stored radial distance is 13 at this off-axis point.
  case 1u:{accepted=ray_history_compatible(12.0,expected,g,n,0.8,false,0.0);}
  case 2u:{accepted=ray_history_compatible(12.0,length(position-vec3f(3.0,4.0,0.0)),g,n,0.8,false,0.0);}
  case 3u:{accepted=ray_history_compatible(13.0,expected,g,-n,0.8,false,0.0);}
  case 4u:{accepted=ray_history_compatible(13.0,expected,g,n,0.4,false,0.0);}
  case 5u:{accepted=ray_history_compatible(13.0,expected,g,n,0.8,true,0.0);}
  case 6u:{accepted=ray_history_compatible(13.0,expected,vec4f(0.0,0.0,-0.8,3.0),n,0.8,true,0.0);}
  case 7u:{accepted=ray_history_compatible(13.0,expected,vec4f(0.0,0.0,0.8,0.0),n,0.8,false,0.0);}
  case 8u:{accepted=ray_history_compatible(13.0,expected,vec4f(0.0,0.0,2.0,6.0),n,2.0,false,0.0);}
  default:{accepted=ray_history_compatible(13.0,expected,g,n,2.0,false,0.0);}
 }
 return vec4f(select(0.0,1.0,accepted),0.0,0.0,1.0);
}
"#;

pub(super) fn device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    pollster::block_on(adapter.request_device(&Default::default())).unwrap()
}

fn texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    width: u32,
    height: u32,
    data: &[[f32; 4]],
) -> wgpu::Texture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("GI denoise fixture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
        bytemuck::cast_slice(data),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 16),
            rows_per_image: Some(height),
        },
        texture.size(),
    );
    texture
}

#[allow(clippy::too_many_arguments)]
fn render_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &str,
    entry: &str,
    width: u32,
    height: u32,
    groups: &[&wgpu::BindGroup],
    layouts: &[Option<&wgpu::BindGroupLayout>],
) -> wgpu::Texture {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("GI denoise acceptance"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: layouts,
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some(entry),
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
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = output.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&pipeline);
        for (index, group) in groups.iter().enumerate() {
            pass.set_bind_group(index as u32, *group, &[]);
        }
        pass.draw(0..3, 0..1);
    }
    queue.submit([encoder.finish()]);
    output
}

pub(super) fn draw(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &str,
    width: u32,
    height: u32,
    groups: &[&wgpu::BindGroup],
    layouts: &[Option<&wgpu::BindGroupLayout>],
) -> Vec<[f32; 4]> {
    let output = render_texture(
        device, queue, source, "fs_main", width, height, groups, layouts,
    );
    let row = (width * 16).div_ceil(256) * 256;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(row * height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        output.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(height),
            },
        },
        output.size(),
    );
    queue.submit([encoder.finish()]);
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, |result| result.unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let bytes = readback.slice(..).get_mapped_range().unwrap();
    bytes
        .chunks_exact(row as usize)
        .flat_map(|line| {
            bytemuck::cast_slice::<u8, [f32; 4]>(&line[..width as usize * 16]).to_vec()
        })
        .collect()
}

#[test]
fn gpu_temporal_rejection_uses_radial_depth_and_true_geometry() {
    let (device, queue) = device();
    let pixels = draw(
        &device,
        &queue,
        &format!("{HELPERS}\n{HISTORY_FIXTURE}"),
        10,
        1,
        &[],
        &[],
    );
    let expected = [1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0];
    for (pixel, expected) in pixels.iter().zip(expected) {
        assert_eq!(pixel[0], expected);
    }
}

#[test]
fn gpu_spatial_denoise_preserves_albedo_detail_and_parallel_depth_edges() {
    spatial_denoise_fixture(2);
}

#[test]
fn gpu_quarter_resolution_preserves_receiver_color_and_depth_edges() {
    spatial_denoise_fixture(4);
}

#[test]
fn gpu_eighth_resolution_preserves_receiver_color_and_depth_edges() {
    spatial_denoise_fixture(8);
}

fn spatial_denoise_fixture(scale: u32) {
    let (device, queue) = device();
    let width = 32u32;
    let height = 16u32;
    let entries = (0..8)
        .map(|binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: if binding == 2 {
                wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                }
            } else {
                wgpu::BindingType::Texture {
                    sample_type: if binding == 5 {
                        wgpu::TextureSampleType::Depth
                    } else {
                        wgpu::TextureSampleType::Float { filterable: false }
                    },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                }
            },
            count: None,
        })
        .collect::<Vec<_>>();
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &entries,
    });
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("GI primary depth"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth_view = depth.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
    }
    queue.submit([encoder.finish()]);
    for (depth_edge, ao_contrast, media_only, foreground, water_edge, signed) in [
        (false, false, false, false, false, false),
        (true, false, false, false, false, false),
        (false, true, false, false, false, false),
        (false, false, true, false, false, false),
        (false, false, true, true, false, false),
        (false, false, false, false, true, false),
        (false, false, false, false, false, true),
    ] {
        let mut uniform_data = vec![0.0f32; 72];
        let inverse = if foreground {
            let mut encoder = device.create_command_encoder(&Default::default());
            {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(0.5),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    ..Default::default()
                });
            }
            queue.submit([encoder.finish()]);
            glam::camera::rh::proj::directx::perspective(0.2, 2.0, 0.1, 1000.0).inverse()
        } else if media_only {
            Mat4::from_scale(Vec3::new(0.1, 0.1, 1.0))
        } else {
            Mat4::IDENTITY
        };
        uniform_data[..16].copy_from_slice(&inverse.to_cols_array());
        uniform_data[16..32].copy_from_slice(&Mat4::IDENTITY.to_cols_array());
        uniform_data[44..47].fill(1.0);
        uniform_data[48..52].fill(1.0);
        uniform_data[62] = scale as f32;
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&uniform_data),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let distance = |x: u32, y: u32| {
            let nx = (x as f32 + 0.5) / width as f32 * 2.0 - 1.0;
            let ny = 1.0 - (y as f32 + 0.5) / height as f32 * 2.0;
            if foreground {
                let point = inverse * Vec4::new(nx, ny, 0.5, 1.0);
                return (point.truncate() / point.w).length();
            }
            if media_only {
                return 2400.0;
            }
            let plane = if depth_edge && x >= width / 2 {
                2.0
            } else {
                1.0
            };
            (nx * nx + ny * ny + 1.0).sqrt() * plane
        };
        let basis = |x: u32, y: u32| {
            if media_only || (water_edge && x < width / 2) {
                return 1.0;
            }
            if (x + y).is_multiple_of(2) {
                0.008
            } else {
                0.032
            }
        };
        let receivers = (0..height)
            .flat_map(|y| {
                (0..width).map(move |x| {
                    if media_only {
                        return [0.0; 4];
                    }
                    [
                        if (x + y).is_multiple_of(2) { 0.8 } else { -0.8 },
                        0.0,
                        0.8,
                        distance(x, y),
                    ]
                })
            })
            .collect::<Vec<_>>();
        let materials = (0..height)
            .flat_map(|y| {
                (0..width).map(move |x| {
                    if water_edge && x < width / 2 {
                        return [0.0, 0.0, 0.0, -2.0];
                    }
                    if ao_contrast {
                        [0.032, 0.032, 0.032, basis(x, y) / 0.032]
                    } else {
                        [basis(x, y), basis(x, y), basis(x, y), 1.0]
                    }
                })
            })
            .collect::<Vec<_>>();
        let radiances = (0..height / scale)
            .flat_map(|y| {
                (0..width / scale).map(move |x| {
                    let mean = if (depth_edge || water_edge) && x * scale >= width / 2 {
                        4.0
                    } else if signed {
                        -1.0
                    } else {
                        1.0
                    };
                    let noise = if (x + y).is_multiple_of(2) { 0.5 } else { -0.5 };
                    [
                        mean + noise,
                        mean + noise,
                        mean + noise,
                        distance(x * scale + scale / 2, y * scale + scale / 2),
                    ]
                })
            })
            .collect::<Vec<_>>();
        let metadata = (0..height / scale)
            .flat_map(|y| {
                (0..width / scale).map(move |x| {
                    if media_only {
                        let nx = ((x * scale + scale / 2) as f32 + 0.5) / width as f32 * 2.0 - 1.0;
                        let ny = 1.0 - ((y * scale + scale / 2) as f32 + 0.5) / height as f32 * 2.0;
                        let far = inverse * Vec4::new(nx, ny, 1.0, 1.0);
                        let n = -(far.truncate() / far.w).normalize();
                        let xy = n.truncate() / n.abs().element_sum();
                        let oct = if n.z < 0.0 {
                            (glam::Vec2::ONE - glam::Vec2::new(xy.y, xy.x).abs()) * xy.signum()
                        } else {
                            xy
                        };
                        [oct.x, oct.y, 2.0, 6.0]
                    } else {
                        [
                            0.0,
                            0.0,
                            if water_edge && x * scale < width / 2 {
                                -0.8
                            } else {
                                0.8
                            },
                            8.0,
                        ]
                    }
                })
            })
            .collect::<Vec<_>>();
        let textures = [
            texture(&device, &queue, width / scale, height / scale, &radiances),
            texture(&device, &queue, width, height, &receivers),
            texture(&device, &queue, width / scale, height / scale, &metadata),
            texture(&device, &queue, width, height, &materials),
            texture(
                &device,
                &queue,
                width,
                height,
                &vec![[0.0; 4]; (width * height) as usize],
            ),
            texture(
                &device,
                &queue,
                width / scale,
                height / scale,
                &vec![[0.37, 0.0, 0.0, 0.0]; (width * height / (scale * scale)) as usize],
            ),
        ];
        let views = textures
            .iter()
            .map(|texture| texture.create_view(&Default::default()))
            .collect::<Vec<_>>();
        let group_for = |radiance: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(radiance),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&views[1]),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&views[2]),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(&views[3]),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::TextureView(&depth_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::TextureView(&views[4]),
                    },
                    wgpu::BindGroupEntry {
                        binding: 7,
                        resource: wgpu::BindingResource::TextureView(&views[5]),
                    },
                ],
            })
        };
        let filtering = group_for(&views[0]);
        let source = format!("{PBR}\n{HELPERS}\n{FILTER}\n{COMPOSITE}");
        let draw_filtered = || {
            // The production filter/reconstruction shader runs in two ordered
            // GPU passes, with no CPU readback of the intermediate irradiance.
            let filtered = render_texture(
                &device,
                &queue,
                &source,
                "fs_filter",
                width / scale,
                height / scale,
                &[&filtering],
                &[Some(&layout)],
            );
            let filtered_view = filtered.create_view(&Default::default());
            let group = group_for(&filtered_view);
            draw(
                &device,
                &queue,
                &source,
                width,
                height,
                &[&group],
                &[Some(&layout)],
            )
        };
        let output = draw_filtered();
        if !media_only && !depth_edge && !ao_contrast && !water_edge && !signed {
            // Change only the high-frequency, colored specular fallback. Its
            // subtraction must remain at the exact center and independent of
            // neighboring albedo/AO; primary extinction scales it once.
            let responses = (0..height)
                .flat_map(|y| {
                    (0..width).map(move |x| {
                        if (x + y).is_multiple_of(2) {
                            [0.24, 0.01, 0.06, 1.0]
                        } else {
                            [0.005, 0.18, 0.09, 1.0]
                        }
                    })
                })
                .collect::<Vec<_>>();
            queue.write_texture(
                textures[4].as_image_copy(),
                bytemuck::cast_slice(&responses),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 16),
                    rows_per_image: Some(height),
                },
                textures[4].size(),
            );
            let colored = draw_filtered();
            let clear = vec![[1.0f32, 0.0, 0.0, 0.0]; (width * height / (scale * scale)) as usize];
            queue.write_texture(
                textures[5].as_image_copy(),
                bytemuck::cast_slice(&clear),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width / scale * 16),
                    rows_per_image: Some(height / scale),
                },
                textures[5].size(),
            );
            let unattenuated = draw_filtered();
            let mut largest = 0.0f32;
            for y in 4..height - 4 {
                for x in 4..width - 4 {
                    let at = (x + y * width) as usize;
                    let scalar = (output[at][0] - colored[at][0]) / responses[at][0];
                    largest = largest.max(scalar);
                    for channel in 0..3 {
                        let removed = output[at][channel] - colored[at][channel];
                        assert!(
                            (removed / responses[at][channel] - scalar).abs() < 0.00002,
                            "specular subtraction must preserve center color"
                        );
                        assert!(
                            (removed - (output[at][channel] - unattenuated[at][channel]) * 0.37)
                                .abs()
                                < 0.00002,
                            "primary extinction scales center fallback once: ({x},{y}) c={channel}; removed={removed}, full={}, error={}",
                            output[at][channel] - unattenuated[at][channel],
                            removed - (output[at][channel] - unattenuated[at][channel]) * 0.37
                        );
                    }
                }
            }
            assert!(largest > 0.02, "specular fallback must actually be removed");
            // A raster-visible reactive leaf whose primary ray misses keeps its
            // existing specular. Negative age isolates its medium-only signal;
            // signed T distinguishes fallback retention from true traced GI.
            let retained_t =
                vec![[-0.37f32, 0.0, 0.0, 0.0]; (width * height / (scale * scale)) as usize];
            let retained_geometry = metadata
                .iter()
                .map(|g| [g[0], g[1], g[2], -1.0])
                .collect::<Vec<_>>();
            let reactive = materials
                .iter()
                .map(|m| [m[0], m[1], m[2], -m[3].abs()])
                .collect::<Vec<_>>();
            for (texture, data) in [
                (&textures[5], &retained_t),
                (&textures[2], &retained_geometry),
                (&textures[3], &reactive),
            ] {
                queue.write_texture(
                    texture.as_image_copy(),
                    bytemuck::cast_slice(data),
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(texture.width() * 16),
                        rows_per_image: Some(texture.height()),
                    },
                    texture.size(),
                );
            }
            let retained = draw_filtered();
            for y in 4..height - 4 {
                for x in 4..width - 4 {
                    let at = (x + y * width) as usize;
                    for channel in 0..3 {
                        assert!(
                            (retained[at][channel] - output[at][channel]).abs() < 0.00002,
                            "raster/ray disagreement must preserve center specular and camera-medium correction"
                        );
                    }
                }
            }
        }
        let mut max_error = 0.0f32;
        for y in 4..height - 4 {
            for x in 4..width - 4 {
                let pixel = output[(x + y * width) as usize];
                let expected = if (depth_edge || water_edge) && x >= width / 2 {
                    4.0
                } else if signed {
                    -1.0
                } else {
                    1.0
                };
                let reconstructed = pixel[0] / basis(x, y);
                max_error = max_error.max((reconstructed - expected).abs());
                assert!(pixel.iter().all(|value| value.is_finite()));
                assert_eq!(pixel[3], 0.0);
            }
        }
        assert!(
            max_error < 0.16,
            "irradiance noise/edge error={max_error}; depth_edge={depth_edge}; media_only={media_only}; foreground={foreground}; water_edge={water_edge}; signed={signed}"
        );
    }
}
