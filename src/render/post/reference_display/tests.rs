use super::*;
#[test]
fn reference_display_production_shaders_validate() {
    for source in [
        format!("{}\n{STAGES}\n{FXAA}", lens::SHADER),
        TAA.to_owned(),
    ] {
        let module = wgpu::naga::front::wgsl::parse_str(&source).unwrap();
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
    }
}

pub(super) fn output(
    device: &wgpu::Device,
    w: u32,
    h: u32,
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}
pub(super) fn clear(
    encoder: &mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    color: wgpu::Color,
) {
    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: None,
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(color),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    });
}
pub(super) fn read(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    mut encoder: wgpu::CommandEncoder,
    texture: &wgpu::Texture,
) -> Vec<u8> {
    let size = texture.size();
    let row = (size.width * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(row) * u64::from(size.height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(size.height),
            },
        },
        size,
    );
    queue.submit(Some(encoder.finish()));
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let data = buffer.slice(..).get_mapped_range().unwrap();
    (0..size.height as usize)
        .flat_map(|y| {
            data[y * row as usize..][..size.width as usize * 4]
                .iter()
                .copied()
        })
        .collect()
}
fn write(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    w: u32,
    h: u32,
    bytes: &[u8],
) -> wgpu::TextureView {
    let texture = output(device, w, h, wgpu::TextureFormat::Rgba8Unorm);
    queue.write_texture(
        texture.as_image_copy(),
        bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(w * 4),
            rows_per_image: Some(h),
        },
        texture.size(),
    );
    texture.create_view(&Default::default())
}
fn flags(display: &ReferenceDisplay, queue: &wgpu::Queue, data: [f32; 4]) {
    queue.write_buffer(&display.options, 0, bytemuck::cast_slice(&data));
}

#[test]
fn gpu_reference_display_gamma_grain_fxaa_and_transfer_match_source_defaults() {
    pollster::block_on(async {
        let adapter = wgpu::Instance::default()
            .request_adapter(&Default::default())
            .await
            .unwrap();
        let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
        let (w, h) = (32, 16);
        let mut encoded_outputs = Vec::new();
        for format in [
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        ] {
            let mut display = ReferenceDisplay::new(&device, w, h, format);
            display.noise = crate::render::sky::ReferenceNoise::fixture(&device, [0, 0, 192, 255]);
            let out = output(&device, w, h, format);
            let view = out.create_view(&Default::default());
            let reactive = write(&device, &queue, w, h, &vec![0; w as usize * h as usize * 4]);
            let mut results = Vec::new();
            for (level, effects) in [
                (0.0, true),
                (0.004, true),
                (0.5, true),
                (2.0, true),
                (0.5, false),
            ] {
                let mut encoder = device.create_command_encoder(&Default::default());
                clear(
                    &mut encoder,
                    &display.linear,
                    wgpu::Color {
                        r: level,
                        g: level,
                        b: level,
                        a: 1.0,
                    },
                );
                display.encode(
                    &device,
                    &queue,
                    &mut encoder,
                    &view,
                    None,
                    &reactive,
                    effects,
                );
                let bytes = read(&device, &queue, encoder, &out);
                let actual = bytes[4 * (8 * w as usize + 16)];
                // Independent composite5 source gamma/grain, RGB8 intermediate.
                // Fallback/master-off deliberately retains engine sRGB bypass.
                let expected = if effects {
                    (level.max(0.0).powf(1.0 / 2.2) + (192.0 / 255.0 - 0.5) / 256.0).clamp(0.0, 1.0)
                } else {
                    1.055 * level.powf(1.0 / 2.4) - 0.055
                };
                let expected = (expected * 255.0).round() as u8;
                assert!(
                    actual.abs_diff(expected) <= 1,
                    "source gamma/grain level{level}: {actual} vs {expected}"
                );
                results.push(actual);
            }
            encoded_outputs.push(results);
        }
        for (linear, srgb) in encoded_outputs[0].iter().zip(&encoded_outputs[1]) {
            assert!(
                linear.abs_diff(*srgb) <= 1,
                "presentation must encode exactly once"
            );
        }
        let display = ReferenceDisplay::new(&device, w, h, wgpu::TextureFormat::Rgba8Unorm);
        let pattern: Vec<_> = (0..w * h)
            .flat_map(|i| {
                let level = if i % w < 16 { 51u8 } else { 204 };
                [level, level, level, 255]
            })
            .collect();
        queue.write_texture(
            display.gamma.texture().as_image_copy(),
            &pattern,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            display.gamma.texture().size(),
        );
        // Independent analytic reduction of the source FXAA311 vertical edge:
        // subpixel t=1/3, cubic smoothing 7/27, squared*.25 →.01680384px.
        // .2+.6*offset→54/255; the bright side symmetrically becomes201/255.
        flags(&display, &queue, [1.0, 1.0, 0.0, 0.0]);
        let mut encoder = device.create_command_encoder(&Default::default());
        draw(
            &mut encoder,
            &display.fxaa_pipeline,
            &display.group(&device, &display.gamma),
            &[&display.fxaa],
        );
        let bytes = read(&device, &queue, encoder, display.fxaa.texture());
        // Hardware bilinear fractional weights can quantize the tiny offset
        // before RGB8 storage; retain only a one-code tolerance.
        assert!(bytes[(8 * 32 + 15) * 4].abs_diff(54) <= 1);
        assert!(bytes[(8 * 32 + 16) * 4].abs_diff(201) <= 1);
        assert!(bytes[(8 * 32 + 15) * 4] > 51);
        assert!(bytes[(8 * 32 + 16) * 4] < 204);
        assert_eq!(bytes[(8 * 32 + 3) * 4], 51);
    });
}

fn camera() -> crate::render::Camera {
    crate::render::Camera {
        position: glam::Vec3::new(0.0, 2.0, 0.0),
        yaw: 0.0,
        pitch: 0.0,
        fov_y_radians: 1.0,
    }
}
fn depth(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    w: u32,
    h: u32,
    z: f32,
) -> wgpu::TextureView {
    let texture = output(device, w, h, wgpu::TextureFormat::Depth32Float);
    let view = texture.create_view(&Default::default());
    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: None,
        color_attachments: &[],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: &view,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(z),
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        ..Default::default()
    });
    drop(_pass);
    view
}
fn sample(bytes: &[u8], w: usize, h: usize, uv: [f64; 2]) -> f64 {
    let p = [uv[0] * w as f64 - 0.5, uv[1] * h as f64 - 0.5];
    let base = [p[0].floor(), p[1].floor()];
    let frac = [p[0] - base[0], p[1] - base[1]];
    let mut result = 0.0;
    for y in 0..2 {
        for x in 0..2 {
            let px = (base[0] as isize + x).clamp(0, w as isize - 1) as usize;
            let py = (base[1] as isize + y).clamp(0, h as isize - 1) as usize;
            result += f64::from(bytes[(py * w + px) * 4]) / 255.0
                * if x == 0 { 1.0 - frac[0] } else { frac[0] }
                * if y == 0 { 1.0 - frac[1] } else { frac[1] };
        }
    }
    result
}
fn catmull(bytes: &[u8], w: usize, h: usize, uv: [f64; 2]) -> f64 {
    let p = [uv[0] * w as f64, uv[1] * h as f64];
    let center = p.map(|v| (v - 0.5).floor() + 0.5);
    let f = [p[0] - center[0], p[1] - center[1]];
    let weights = f.map(|f| {
        [
            -0.7 * f.powi(3) + 1.4 * f * f - 0.7 * f,
            1.3 * f.powi(3) - 2.3 * f * f + 1.0,
            -1.3 * f.powi(3) + 1.6 * f * f + 0.7 * f,
            0.7 * f.powi(3) - 0.7 * f * f,
        ]
    });
    let middle = [weights[0][1] + weights[0][2], weights[1][1] + weights[1][2]];
    let tc = [
        (center[0] + weights[0][2] / middle[0]) / w as f64,
        (center[1] + weights[1][2] / middle[1]) / h as f64,
    ];
    let pairs = [
        (
            [tc[0], (center[1] - 1.0) / h as f64],
            middle[0] * weights[1][0],
        ),
        (
            [(center[0] - 1.0) / w as f64, tc[1]],
            weights[0][0] * middle[1],
        ),
        (tc, middle[0] * middle[1]),
        (
            [(center[0] + 2.0) / w as f64, tc[1]],
            weights[0][3] * middle[1],
        ),
        (
            [tc[0], (center[1] + 2.0) / h as f64],
            middle[0] * weights[1][3],
        ),
    ];
    pairs
        .iter()
        .map(|(uv, weight)| sample(bytes, w, h, *uv) * weight)
        .sum::<f64>()
        / pairs.iter().map(|(_, weight)| weight).sum::<f64>()
}

#[test]
fn gpu_reference_display_catmull_history_classification_and_submitted_lifecycle() {
    pollster::block_on(async {
        let adapter = wgpu::Instance::default()
            .request_adapter(&Default::default())
            .await
            .unwrap();
        let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
        let (w, h) = (32, 16);
        let mut display = ReferenceDisplay::new(&device, w, h, wgpu::TextureFormat::Rgba8Unorm);
        let mut temporal = Temporal::new(&device, w, h);
        temporal.prepare(&queue, camera(), w, h);
        assert!(temporal.reference_frame().is_some());
        temporal.submitted();
        assert!(
            temporal.previous.is_none(),
            "prepare alone cannot commit camera history"
        );
        let history: Vec<_> = (0..w * h)
            .flat_map(|i| {
                let c = 64 + ((i % w * 7 + i / w * 11) % 29) as u8 * 4;
                [c, c, c, 255]
            })
            .collect();
        let current: Vec<_> = (0..w * h)
            .flat_map(|i| {
                let c = if (i % w + i / w) % 2 == 0 { 51 } else { 204 };
                [c, c, c, 255]
            })
            .collect();
        let source = write(&device, &queue, w, h, &current);
        let history_view = write(&device, &queue, w, h, &history);
        let reactive = write(&device, &queue, w, h, &vec![0; w as usize * h as usize * 4]);
        let history_depth =
            output(&device, w, h, wgpu::TextureFormat::R32Float).create_view(&Default::default());
        let mut matrix = [0.0f32; 40];
        let stable = crate::render::view_projection(camera(), w, h);
        matrix[..16].copy_from_slice(&stable.inverse().to_cols_array());
        matrix[16..32].copy_from_slice(&stable.to_cols_array());
        matrix[32..36].copy_from_slice(&[0.9, 1.0, 0.01, 0.0]);
        matrix[36..].copy_from_slice(&[0.1, 1000.0, 0.0, 0.0]);
        queue.write_buffer(&temporal.settings, 0, bytemuck::cast_slice(&matrix));
        let mut outputs = Vec::new();
        for (saved, z, marker, valid) in [
            (0.2, 0.5, 1.0, 1.0),
            (-1.0, 0.5, 1.0, 1.0),
            (0.2, 0.5, -1.0, 1.0),
            (0.2, 0.5, 1.0, 0.0),
            (-1.0, 1.0, 0.0, 1.0),
            (0.2, 1.0, 0.0, 1.0),
        ] {
            matrix[33] = valid;
            queue.write_buffer(&temporal.settings, 0, bytemuck::cast_slice(&matrix));
            let mut encoder = device.create_command_encoder(&Default::default());
            let depth = depth(&device, &mut encoder, w, h, z);
            clear(
                &mut encoder,
                &history_depth,
                wgpu::Color {
                    r: saved,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                },
            );
            clear(
                &mut encoder,
                &temporal.motion,
                wgpu::Color {
                    r: 0.35,
                    g: 0.2,
                    b: 0.2,
                    a: marker,
                },
            );
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &display.temporal_layout,
                entries: &[
                    texture_entry(0, &source),
                    texture_entry(1, &depth),
                    texture_entry(2, &history_view),
                    texture_entry(3, &history_depth),
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::Sampler(&display.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: temporal.settings.as_entire_binding(),
                    },
                    texture_entry(6, &temporal.motion),
                    texture_entry(7, &reactive),
                ],
            });
            draw(
                &mut encoder,
                &display.temporal_pipeline,
                &group,
                &[&display.colors[1], &display.depths[1]],
            );
            let bytes = read(&device, &queue, encoder, display.colors[1].texture());
            outputs.push(bytes[(8 * 32 + 16) * 4]);
        }
        let uv = [16.85 / 32.0, 8.7 / 16.0];
        let old = catmull(&history, 32, 16, uv).clamp(0.2, 0.8);
        let weight = 0.7 + 0.2 * (-0.35_f64.hypot(0.2)).exp();
        let expected = ((0.2 * (1.0 - weight) + old * weight) * 255.0).round() as u8;
        assert!(
            outputs[0].abs_diff(expected) <= 1,
            "actual source Catmull/motion blend {} vs{expected}",
            outputs[0]
        );
        assert_eq!(outputs[1], 51, "foreground cannot retain sky");
        assert_eq!(outputs[2], 51, "reactive content rejects history");
        // Missing-history source four-diagonal .1667px bilinear blur on checker.
        let uv = [16.5 / 32.0, 8.5 / 16.0];
        let missing = [[-1.0, -1.0], [1.0, 1.0], [1.0, -1.0], [-1.0, 1.0]]
            .iter()
            .map(|d| {
                sample(
                    &current,
                    32,
                    16,
                    [uv[0] + d[0] * 0.1667 / 32.0, uv[1] + d[1] * 0.1667 / 16.0],
                )
            })
            .sum::<f64>()
            * 0.25;
        assert!(outputs[3].abs_diff((missing * 255.0).round() as u8) <= 1);
        let sky_old = catmull(&history, 32, 16, [16.5 / 32.0, 8.5 / 16.0]).clamp(0.2, 0.8);
        let sky_expected = ((0.2 * 0.1 + sky_old * 0.9) * 255.0).round() as u8;
        assert!(
            outputs[4].abs_diff(sky_expected) <= 1,
            "stationary directional sky history {} vs {sky_expected}",
            outputs[4]
        );
        assert_eq!(outputs[5], 51, "sky cannot retain foreground");
        // Actual display encode marks a resolve, but only submitted advances.
        temporal.reference_reset();
        temporal.prepare(&queue, camera(), w, h);
        let mut encoder = device.create_command_encoder(&Default::default());
        display.depth = Some(depth(&device, &mut encoder, w, h, 0.5));
        clear(
            &mut encoder,
            &display.linear,
            wgpu::Color {
                r: 0.5,
                g: 0.5,
                b: 0.5,
                a: 1.0,
            },
        );
        clear(&mut encoder, &temporal.motion, wgpu::Color::TRANSPARENT);
        let out = output(&device, w, h, wgpu::TextureFormat::Rgba8Unorm);
        display.encode(
            &device,
            &queue,
            &mut encoder,
            &out.create_view(&Default::default()),
            Some(&mut temporal),
            &reactive,
            true,
        );
        read(&device, &queue, encoder, &out);
        assert_eq!(display.index, 0);
        assert!(temporal.previous.is_none());
        display.submitted();
        temporal.submitted();
        assert_eq!(display.index, 1);
        assert!(temporal.previous.is_some());
        let mut cut = camera();
        cut.position.x += 10.0;
        temporal.prepare(&queue, cut, w, h);
        display.resize(&device, 7, 5);
        assert_eq!(display.index, 0);
        assert!(display.depth.is_none());
    });
}
