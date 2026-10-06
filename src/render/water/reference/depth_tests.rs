//! Draw the production source-water fragment with the depth state used by both
//! near and LOD water. Verify private nearest depth and immutable opaque depth.
use wgpu::util::DeviceExt;

#[test]
fn gpu_reference_water_front_depth_is_nearest_and_opaque_depth_stays_immutable() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let camera = include_str!("../../water.wgsl")
        .split("@group")
        .next()
        .unwrap();
    let source=crate::render::daylight::surface_shader(&format!(r#"
{camera}
@group(0) @binding(0) var<uniform> camera:Camera;
@group(0) @binding(1) var<uniform> layer:vec4f;
{}
@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4f{{let p=array<vec2f,3>(vec2f(-1.0,-1.0),vec2f(3.0,-1.0),vec2f(-1.0,3.0));return vec4f(p[i],layer.x,1.0);}}
struct WaterTestOutput{{@location(0) color:vec4f,@location(1) translucent:vec4f}};
@fragment fn fs(@builtin(position) pixel:vec4f)->WaterTestOutput{{let result=bg_reference_water_surface(vec3f(0.0,1.0,0.0),1.0,0.0,vec3f(0.0,1.0,0.0),vec3f(0.0,-1.0,-2.0),true,1.0,pixel.xy);return WaterTestOutput(result.color,result.indirect);}}
"#,super::shader(false))).replace("const BG_FOG_REFERENCE: bool = false;","const BG_FOG_REFERENCE: bool = true;");
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("actual source-water front depth regression"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let atmosphere = crate::render::daylight::Atmosphere::at(crate::daylight::INITIAL_MS);
    let eye = glam::Vec3::new(0.0, 2.0, 2.0);
    let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&atmosphere.camera_data(glam::Mat4::IDENTITY, eye)),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let layer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 16,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut composition = super::composition::Composition::new(&device);
    for reference in [false, true] {
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("production shared near/LOD water depth state"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[
                    Some(wgpu::ColorTargetState {
                        format: crate::render::post::HDR_FORMAT,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                    Some(wgpu::ColorTargetState {
                        format: crate::render::post::HDR_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                ],
            }),
            primitive: Default::default(),
            depth_stencil: Some(crate::render::water::depth_state(reference)),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let camera_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: layer.as_entire_binding(),
                },
            ],
        });
        for depths in [[0.6f32], [0.3f32]]
            .into_iter()
            .map(Vec::from)
            .chain([vec![0.35, 0.3], vec![0.3, 0.35]])
        {
            let scene = super::tests::hdr(&device, 2, 1);
            let opaque = super::tests::depth(&device, 2, 1);
            let metadata = super::tests::hdr(&device, 2, 1);
            let mut encoder = device.create_command_encoder(&Default::default());
            {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[
                        Some(wgpu::RenderPassColorAttachment {
                            view: &scene,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Store,
                            },
                        }),
                        Some(wgpu::RenderPassColorAttachment {
                            view: &metadata,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                                store: wgpu::StoreOp::Store,
                            },
                        }),
                    ],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &opaque,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(0.4),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    ..Default::default()
                });
            }
            let target = composition.begin(&device, &mut encoder, &scene, &opaque);
            queue.submit([encoder.finish()]);
            let mut inputs = super::Inputs::fallback(&device);
            inputs.opaque_depth = opaque.clone();
            inputs.reflection = composition.reflection.as_ref().unwrap().clone();
            inputs.prepare(
                &queue,
                crate::render::water::Frame {
                    atmosphere,
                    eye_in_water: false,
                    sample: 0,
                    camera: crate::render::Camera {
                        position: eye,
                        yaw: 0.0,
                        pitch: 0.0,
                        fov_y_radians: 1.0,
                    },
                    size: [2, 1],
                    view_projection: glam::Mat4::IDENTITY,
                },
            );
            let entries = inputs.entries(1);
            let layout = pipeline.get_bind_group_layout(1);
            let water_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &layout,
                entries: &entries,
            });
            for depth in &depths {
                queue.write_buffer(&layer, 0, bytemuck::cast_slice(&[*depth, 0.0, 0.0, 0.0]));
                let mut encoder = device.create_command_encoder(&Default::default());
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        color_attachments: &[
                            Some(wgpu::RenderPassColorAttachment {
                                view: &target,
                                depth_slice: None,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Load,
                                    store: wgpu::StoreOp::Store,
                                },
                            }),
                            Some(wgpu::RenderPassColorAttachment {
                                view: &metadata,
                                depth_slice: None,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Load,
                                    store: wgpu::StoreOp::Store,
                                },
                            }),
                        ],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: composition.front_depth.as_ref().unwrap(),
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            }),
                            stencil_ops: None,
                        }),
                        ..Default::default()
                    });
                    pass.set_pipeline(&pipeline);
                    pass.set_bind_group(0, &camera_group, &[]);
                    pass.set_bind_group(1, &water_group, &[]);
                    pass.draw(0..3, 0..1);
                }
                queue.submit([encoder.finish()]);
            }
            let mut encoder = device.create_command_encoder(&Default::default());
            let read = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 768,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            for (offset, view) in [
                (0, &opaque),
                (256, composition.front_depth.as_ref().unwrap()),
            ] {
                let mut copy = view.texture().as_image_copy();
                copy.aspect = wgpu::TextureAspect::DepthOnly;
                encoder.copy_texture_to_buffer(
                    copy,
                    wgpu::TexelCopyBufferInfo {
                        buffer: &read,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset,
                            bytes_per_row: Some(256),
                            rows_per_image: Some(1),
                        },
                    },
                    view.texture().size(),
                );
            }
            encoder.copy_texture_to_buffer(
                metadata.texture().as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &read,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 512,
                        bytes_per_row: Some(256),
                        rows_per_image: Some(1),
                    },
                },
                metadata.texture().size(),
            );
            queue.submit([encoder.finish()]);
            let slice = read.slice(..);
            slice.map_async(wgpu::MapMode::Read, |r| r.unwrap());
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            {
                let bytes = slice.get_mapped_range().unwrap();
                let values: &[f32] = bytemuck::cast_slice(&bytes);
                let expected = if reference {
                    depths.iter().copied().fold(0.4, f32::min)
                } else {
                    0.4
                };
                let alpha = 0.7f64;
                for (channel, encoded) in [64.0f64, 160.0, 255.0].into_iter().enumerate() {
                    let base = (encoded / 255.0 * 0.35).powi(2) * 0.1225;
                    let expected =
                        (1.0 - alpha.sqrt() + base.sqrt() * alpha.sqrt()) * (1.0 - alpha.powi(64));
                    let at = 512 + channel * 2;
                    let actual = super::tests::half(u16::from_le_bytes([bytes[at], bytes[at + 1]]));
                    if depths.iter().any(|depth| *depth < 0.4) {
                        assert!(
                            (f64::from(actual) - expected).abs() < 0.001,
                            "source vlAlbedo channel{channel}: {actual}!={expected}"
                        );
                        let tag = super::tests::half(u16::from_le_bytes([bytes[518], bytes[519]]));
                        assert_eq!(tag, -2.0);
                    } else {
                        assert_eq!(actual, 0.0, "occluded water cannot write translucency");
                    }
                }
                for x in 0..2 {
                    assert!((values[x] - 0.4).abs() < 1e-6, "opaque depth was mutated");
                    assert!(
                        (values[64 + x] - expected).abs() < 1e-6,
                        "reference={reference} depths={depths:?}: front={} expected={expected}",
                        values[64 + x]
                    );
                }
            }
            read.unmap();
        }
    }
}
