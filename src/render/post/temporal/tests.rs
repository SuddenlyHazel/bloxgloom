use super::*;

fn camera() -> crate::render::Camera {
    crate::render::Camera {
        position: Vec3::ZERO,
        yaw: 0.0,
        pitch: 0.0,
        fov_y_radians: 1.0,
    }
}

#[test]
fn jitter_is_bounded_and_repeats_without_depth_changes() {
    for frame in 0..8 {
        let offset = jitter(frame);
        assert!(offset.abs().max_element() <= 0.5);
        assert_eq!(offset, jitter(frame + 8));
        let point = glam::Vec4::new(0.3, 0.2, 0.8, 2.0);
        let shifted = jitter_matrix(Mat4::IDENTITY, offset, 800, 600) * point;
        assert_eq!(shifted.z, point.z);
        assert_eq!(shifted.w, point.w);
        let stable = crate::render::view_projection(camera(), 800, 600);
        let raster = jitter_matrix(stable, offset, 800, 600);
        let clip = glam::Vec4::new(0.2, -0.1, 0.8, 1.0);
        let previous = stable * (raster.inverse() * clip);
        let uv = previous.truncate().truncate() / previous.w * Vec2::new(0.5, -0.5)
            + Vec2::splat(0.5)
            + offset / Vec2::new(800.0, 600.0);
        assert!(
            uv.distance(Vec2::new(0.6, 0.55)) < 1e-5,
            "jitter alone must not create history motion"
        );
        assert!((shifted.x / shifted.w - point.x / point.w - offset.x / 400.0).abs() < 1e-6);
    }
}

#[test]
fn camera_cuts_projection_changes_and_teleports_reject_history() {
    let old = camera();
    let mut new = old;
    new.position.x = 0.1;
    assert!(continuous(old, new));
    new.position.x = 10.0;
    assert!(!continuous(old, new));
    new = old;
    new.yaw += 1.0;
    assert!(!continuous(old, new));
    new = old;
    new.fov_y_radians += 0.01;
    assert!(!continuous(old, new));
}

#[test]
fn gpu_temporal_history_rejects_disocclusion_and_offscreen_motion() {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .unwrap();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();
        if !supported(&device) {
            let mut post =
                super::super::PostProcess::new(&device, 5, 5, wgpu::TextureFormat::Rgba8UnormSrgb);
            post.enable_temporal(&device, true);
            assert!(
                post.temporal.is_none(),
                "unsupported backend must degrade safely"
            );
            return;
        }
        let mut taa = Temporal::new(&device, 5, 5);
        let texture = |format, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("temporal fixture"),
                size: wgpu::Extent3d {
                    width: 5,
                    height: 5,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let scene = texture(
            super::super::HDR_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
        );
        let scene_view = scene.create_view(&Default::default());
        let depth = texture(
            wgpu::TextureFormat::Depth32Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let depth_view = depth.create_view(&Default::default());
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("temporal fixture shader"),
            source: wgpu::ShaderSource::Wgsl(
                r#"
@vertex fn vs(@builtin(vertex_index) id: u32) -> @builtin(position) vec4f {
    let uv = vec2f(f32((id << 1u) & 2u), f32(id & 2u));
    return vec4f(uv * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs(@builtin(position) p: vec4f) -> @location(0) vec4f {
    var value = 0.5;
    if p.x < 2.0 { value = 0.0; }
    if p.x > 3.0 { value = 1.0; }
    return vec4f(vec3f(value), 1.0);
}
"#
                .into(),
            ),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("temporal fixture"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: super::super::HDR_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        // Seed history at 0.75; current center is 0.5 with a [0,1] neighborhood.
        // A valid sample must blend; depth changes and offscreen motion must not.
        let camera_cases = [
            (0.5, Mat4::IDENTITY, true, true, false),
            // One-pixel camera motion still reprojects valid history.
            (
                0.5,
                Mat4::from_translation(Vec3::new(0.4, 0.0, 0.0)),
                true,
                true,
                false,
            ),
            (0.2, Mat4::IDENTITY, true, false, false),
            (
                0.5,
                Mat4::from_translation(Vec3::new(3.0, 0.0, 0.0)),
                true,
                false,
                false,
            ),
            (0.5, Mat4::IDENTITY, false, false, false),
            // Same-depth moving/changed content cannot leave bright history in a
            // newly dark neighborhood, even without object velocity information.
            (0.5, Mat4::IDENTITY, true, false, true),
        ];
        let cases = camera_cases
            .into_iter()
            .map(|(d, p, v, b, black)| (d, p, v, b, black, [0.0; 4]))
            .chain([
                // A valid object velocity overrides incorrect camera-only reprojection.
                (
                    0.5,
                    Mat4::from_translation(Vec3::new(3.0, 0.0, 0.0)),
                    true,
                    true,
                    false,
                    [0.0, 0.0, 0.1, 1.0],
                ),
                (0.5, Mat4::IDENTITY, true, true, false, [1.0, 0.0, 0.1, 1.0]),
                // New/changed objects cannot reuse same-color, same-depth history.
                (
                    0.5,
                    Mat4::IDENTITY,
                    true,
                    false,
                    false,
                    [0.0, 0.0, 0.1, -1.0],
                ),
                // Expected depth is the moving surface's old depth, not current depth.
                (
                    0.5,
                    Mat4::IDENTITY,
                    true,
                    false,
                    false,
                    [0.0, 0.0, 0.3, 1.0],
                ),
                (
                    0.5,
                    Mat4::IDENTITY,
                    true,
                    false,
                    false,
                    [6.0, 0.0, 0.1, 1.0],
                ),
            ])
            .map(|(d, p, v, b, black, object)| (d, p, v, b, black, object, 1.0))
            .chain(
                [-1.0, -2.0]
                    .map(|reactive| (0.5, Mat4::IDENTITY, true, false, false, [0.0; 4], reactive)),
            );
        let indirect = texture(
            super::super::HDR_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let indirect = indirect.create_view(&Default::default());

        for (history_depth, previous, valid, expected_blend, uniform_black, object, reactive) in
            cases
        {
            let mut uniforms = Vec::new();
            uniforms.extend(Mat4::IDENTITY.to_cols_array());
            uniforms.extend(previous.to_cols_array());
            uniforms.extend([0.9, if valid { 1.0 } else { 0.0 }, 0.01, 0.0]);
            uniforms.extend([
                crate::render::visibility::CAMERA_NEAR,
                crate::render::visibility::CAMERA_FAR,
                0.0,
                0.0,
            ]);
            queue.write_buffer(&taa.settings, 0, bytemuck::cast_slice(&uniforms));
            taa.pending = Some((Mat4::IDENTITY, camera()));
            let readback = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("temporal readback"),
                size: 256,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let mut encoder = device.create_command_encoder(&Default::default());
            let attachment = |view, value| {
                Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: value,
                            g: value,
                            b: value,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })
            };
            {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[
                        attachment(&taa.colors[taa.index], 0.75),
                        attachment(
                            &taa.depths[taa.index],
                            0.05 / (1.0 - history_depth * (1.0 - 0.05 / 4096.0)),
                        ),
                    ],
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
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[attachment(&scene_view, 0.0)],
                    ..Default::default()
                });
                if !uniform_black {
                    pass.set_pipeline(&pipeline);
                    pass.draw(0..3, 0..1);
                }
            }
            {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &taa.motion,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: object[0],
                                g: object[1],
                                b: object[2],
                                a: object[3],
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
            }
            {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &indirect,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.0,
                                g: 0.0,
                                b: 0.0,
                                a: reactive,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
            }
            taa.resolve(
                &device,
                &mut encoder,
                &scene_view,
                &depth_view,
                Some(&indirect),
            );
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &scene,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: 2, y: 2, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &readback,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(256),
                        rows_per_image: Some(1),
                    },
                },
                wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
            queue.submit(Some(encoder.finish()));
            taa.submitted();
            let (tx, rx) = std::sync::mpsc::channel();
            readback
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            rx.recv().unwrap().unwrap();
            let bytes = readback.slice(..).get_mapped_range().unwrap();
            let half = u16::from_le_bytes([bytes[0], bytes[1]]);
            // Fixture output is positive normal f16, avoiding a conversion dependency.
            let value = f32::from_bits(
                ((u32::from(half) >> 10) + 112) << 23 | ((u32::from(half) & 1023) << 13),
            );
            if uniform_black {
                assert_eq!(half, 0, "moving/changed bright history leaked into black");
            } else if expected_blend {
                assert!(
                    value > 0.6 && value < 0.7,
                    "history failed to blend: {value}"
                );
            } else {
                assert!(
                    (value - 0.5).abs() < 0.001,
                    "rejected history leaked: {value}"
                );
            }
        }
        assert_eq!(taa.frame, 13);
        let (_, first_offset) = taa.prepare(&queue, camera(), 5, 5);
        let stable = taa.pending.unwrap().0;
        let (_, repeated_offset) = taa.prepare(&queue, camera(), 5, 5);
        assert_eq!(
            first_offset, repeated_offset,
            "unsubmitted frame advanced jitter"
        );
        assert_eq!(
            stable,
            crate::render::view_projection(camera(), 5, 5),
            "history retained raster jitter"
        );
        let mut post =
            super::super::PostProcess::new(&device, 5, 5, wgpu::TextureFormat::Rgba8UnormSrgb);
        post.enable_temporal(&device, true);
        post.temporal.as_mut().unwrap().valid = true;
        post.temporal.as_mut().unwrap().frame = 5;
        post.resize(&device, 7, 3);
        let reset = post.temporal.as_ref().unwrap();
        assert!(!reset.valid);
        assert_eq!(reset.frame, 0);
        assert_eq!(reset.colors[0].texture().size().width, 7);
        assert_eq!(reset.colors[0].texture().size().height, 3);
    });
}

#[path = "sky_tests.rs"]
mod sky;
