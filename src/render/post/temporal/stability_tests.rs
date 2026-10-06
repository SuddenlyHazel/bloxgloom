//! Actual jitter/resolve/upscale sequences with independent pixel-area truth.
use super::super::*;
use crate::render::post::{HDR_FORMAT, PostProcess};
use wgpu::util::DeviceExt;

const WIDTH: u32 = 64;
const HEIGHT: u32 = 40;

fn shape(x: f32, y: f32, pan: f32) -> f32 {
    if y >= 36.0 {
        return if x < 32.0 { 0.0 } else { 2.0 };
    }
    f32::from(((x + 0.57 * y + pan) / 7.0).fract() > 0.5)
}

#[test]
fn gpu_sampling_mode_survives_resize_and_invalidates_history_on_budget_changes() {
    let adapter =
        pollster::block_on(wgpu::Instance::default().request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    if !supported(&device) {
        return;
    }
    let mut post =
        PostProcess::new_with_reference(&device, 7, 5, wgpu::TextureFormat::Rgba8UnormSrgb, false);
    post.configure_reduced_resolution(true);
    post.enable_temporal(&device, true);
    assert_eq!(post.prepare_temporal(&queue, super::camera()).1, Vec2::ZERO);
    post.temporal.as_mut().unwrap().valid = true;
    post.resize(&device, 9, 7);
    assert!(!post.temporal.as_ref().unwrap().valid);
    assert_eq!(post.prepare_temporal(&queue, super::camera()).1, Vec2::ZERO);
    post.temporal.as_mut().unwrap().valid = true;
    post.configure_reduced_resolution(false);
    assert!(!post.temporal.as_ref().unwrap().valid);
    assert_eq!(post.prepare_temporal(&queue, super::camera()).1, jitter(0));
    post.configure_reduced_resolution(true);
    post.enable_temporal(&device, false);
    post.enable_temporal(&device, true);
    assert_eq!(post.prepare_temporal(&queue, super::camera()).1, Vec2::ZERO);
    let mut reference =
        PostProcess::new_with_reference(&device, 7, 5, wgpu::TextureFormat::Rgba8UnormSrgb, true);
    reference.configure_reduced_resolution(true);
    reference.enable_temporal(&device, true);
    assert_eq!(
        reference.prepare_temporal(&queue, super::camera()).1,
        jitter(0)
    );
}

#[test]
fn gpu_reduced_resolution_stops_stationary_reactive_crawl_and_filters_moving_edges() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    if !supported(&device) {
        return;
    }
    let texture = |format, usage, width, height| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("reduced-resolution stability fixture"),
            size: wgpu::Extent3d {
                width,
                height,
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
    let usage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
    let depth = texture(wgpu::TextureFormat::Depth32Float, usage, WIDTH, HEIGHT);
    let depth = depth.create_view(&Default::default());
    let reactive = texture(HDR_FORMAT, usage, WIDTH, HEIGHT);
    let reactive = reactive.create_view(&Default::default());
    let output = texture(
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        WIDTH * 3,
        HEIGHT * 3,
    );
    let output_view = output.create_view(&Default::default());
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("independent raster sample positions"),
        contents: &[0; 16],
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("analytic reactive edges with actual raster jitter"),
        source: wgpu::ShaderSource::Wgsl(
            r#"
@group(0) @binding(0) var<uniform> sample:vec4f;
@vertex fn vs(@builtin(vertex_index) id:u32)->@builtin(position) vec4f {
 let uv=vec2f(f32((id<<1u)&2u),f32(id&2u));return vec4f(uv*2.0-1.0,0.0,1.0);
}
struct Output {@location(0) color:vec4f,@location(1) reactive:vec4f,@builtin(frag_depth) depth:f32};
@fragment fn fs(@builtin(position) p:vec4f)->Output {
 let stable=p.xy-sample.xy;
 var c=select(0.0,1.0,fract((stable.x+0.57*stable.y+sample.z)/7.0)>0.5);
 if stable.y>=36.0 {c=select(0.0,2.0,stable.x>=32.0);}
 // The actual wind and water tags. Neither may borrow stale history.
 return Output(vec4f(vec3f(c),1.0),vec4f(0.0,0.0,0.0,select(-1.0,-2.0,p.x>=32.0)),0.5);
}
"#
            .into(),
        ),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("analytic current-only wind/water coverage"),
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
            targets: &[0, 1].map(|_| {
                Some(wgpu::ColorTargetState {
                    format: HDR_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })
            }),
        }),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        primitive: Default::default(),
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform.as_entire_binding(),
        }],
    });
    let artifacts = std::env::var_os("BLOXGLOOM_SHIMMER_ARTIFACTS").map(std::path::PathBuf::from);
    let mut sequences = Vec::new();
    for (label, stable, spatial, moving) in [
        ("old-stationary", false, false, false),
        ("fixed-stationary", true, true, false),
        ("stable-unfiltered-moving", true, false, true),
        ("fixed-moving", true, true, true),
    ] {
        let mut post = PostProcess::new_with_reference(
            &device,
            WIDTH,
            HEIGHT,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            false,
        );
        post.enable_temporal(&device, true);
        post.temporal.as_mut().unwrap().reduced_resolution(stable);
        let mut frames = Vec::new();
        for frame in 0..8 {
            let pan = if moving { frame as f32 * 0.125 } else { 0.0 };
            let (_, offset) = post.prepare_temporal(&queue, super::camera());
            queue.write_buffer(
                &uniform,
                0,
                bytemuck::cast_slice(&[offset.x, offset.y, pan, 0.0]),
            );
            if !spatial {
                queue.write_buffer(&post.temporal.as_ref().unwrap().settings, 160, &[0; 16]);
            }
            let mut encoder = device.create_command_encoder(&Default::default());
            let attachment = |view| {
                Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })
            };
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[attachment(&post.scene), attachment(&reactive)],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &depth,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    ..Default::default()
                });
                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, &group, &[]);
                pass.draw(0..3, 0..1);
            }
            post.draw_motion(&queue, &mut encoder, &depth, None);
            post.temporal.as_mut().unwrap().resolve(
                &device,
                &mut encoder,
                &post.scene,
                &depth,
                Some(&reactive),
            );
            post.configure(&queue, true, 1.0, 0.0);
            post.encode(&device, &queue, &mut encoder, &output_view);
            let read = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: u64::from(WIDTH * HEIGHT * 8),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            encoder.copy_texture_to_buffer(
                post.scene.texture().as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &read,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(WIDTH * 8),
                        rows_per_image: Some(HEIGHT),
                    },
                },
                post.scene.texture().size(),
            );
            queue.submit(Some(encoder.finish()));
            post.submitted();
            let (tx, rx) = std::sync::mpsc::channel();
            read.map_async(wgpu::MapMode::Read, .., move |r| tx.send(r).unwrap());
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            rx.recv().unwrap().unwrap();
            let mapped = read.get_mapped_range(..).unwrap();
            let values = mapped
                .chunks_exact(8)
                .map(|p| {
                    let bits = u16::from_le_bytes([p[0], p[1]]);
                    if bits & 0x7c00 == 0 {
                        f32::from(bits & 1023) / 16_777_216.0
                    } else {
                        f32::from_bits(
                            ((u32::from(bits) >> 10) + 112) << 23
                                | ((u32::from(bits) & 1023) << 13),
                        )
                    }
                })
                .collect::<Vec<_>>();
            assert!(
                values
                    .iter()
                    .all(|v| v.is_finite() && *v >= 0.0 && *v <= 2.0)
            );
            assert_eq!(values[(HEIGHT as usize - 2) * WIDTH as usize + 8], 0.0);
            assert_eq!(values[(HEIGHT as usize - 2) * WIDTH as usize + 48], 2.0);
            frames.push(values);
            drop(mapped);
            read.unmap();
            if let Some(directory) = &artifacts {
                crate::preview::capture::save_texture(
                    &device,
                    &queue,
                    &output,
                    WIDTH * 3,
                    HEIGHT * 3,
                    &directory.join(format!("{label}-{frame:02}.png")),
                )
                .unwrap();
            }
        }
        sequences.push(frames);
    }
    let changed = sequences[0]
        .windows(2)
        .map(|f| {
            f[0].iter()
                .zip(&f[1])
                .filter(|(a, b)| (*a - *b).abs() > 0.05)
                .count()
        })
        .sum::<usize>();
    let mut predicted_changes = 0;
    for frame in 0..8 {
        let offset = jitter(frame);
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let sample_x = x as f32 + 0.5 - offset.x;
                let sample_y = y as f32 + 0.5 - offset.y;
                let phase = ((sample_x + 0.57 * sample_y) / 7.0).fract();
                // CPU/GPU contraction can classify a sample exactly on a
                // discontinuous analytic stripe edge differently. Every other
                // sample must match the independent point-coverage oracle.
                if sample_y < 36.0 && phase.min(1.0 - phase).min((phase - 0.5).abs()) < 0.0001 {
                    continue;
                }
                let expected = shape(sample_x, sample_y, 0.0);
                let index = (y * WIDTH + x) as usize;
                assert_eq!(
                    sequences[0][frame as usize][index], expected,
                    "old point coverage at {x},{y}, frame {frame}"
                );
                if frame > 0 && expected != sequences[0][frame as usize - 1][index] {
                    predicted_changes += 1;
                }
            }
        }
    }
    assert!(
        predicted_changes > WIDTH as usize * HEIGHT as usize / 4,
        "independently predicted edges must crawl"
    );
    assert!(
        sequences[1].windows(2).all(|f| f[0] == f[1]),
        "stable current-only surfaces must remain identical"
    );
    let error = |frames: &[Vec<f32>]| {
        let mut sum = 0.0f64;
        for (frame, values) in frames.iter().enumerate() {
            for y in 2..34 {
                for x in 2..WIDTH - 2 {
                    let mut truth = 0.0;
                    for sy in 0..32 {
                        for sx in 0..32 {
                            truth += shape(
                                x as f32 + (sx as f32 + 0.5) / 32.0,
                                y as f32 + (sy as f32 + 0.5) / 32.0,
                                frame as f32 * 0.125,
                            );
                        }
                    }
                    let difference = f64::from(values[(y * WIDTH + x) as usize] - truth / 1024.0);
                    sum += difference * difference;
                }
            }
        }
        sum
    };
    let before = error(&sequences[2]);
    let after = error(&sequences[3]);
    assert!(
        after < before,
        "spatial edge AA must approach independently integrated coverage: {before} -> {after}"
    );
    eprintln!(
        "stationary reactive changed pixels: {changed} -> 0; moving edge squared error: {before:.6} -> {after:.6}"
    );
}
