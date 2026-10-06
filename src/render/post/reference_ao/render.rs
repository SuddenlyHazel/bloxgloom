use super::*;
use glam::{DMat4, Vec3};
use wgpu::util::DeviceExt;
const SIDE: u32 = 64;
fn target(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("reference AO acceptance"),
            size: wgpu::Extent3d {
                width: SIDE,
                height: SIDE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
        .create_view(&Default::default())
}
fn read(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    view: &wgpu::TextureView,
    bytes: u32,
) -> Vec<u8> {
    let row = (SIDE * bytes).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(row * SIDE),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        view.texture().as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(SIDE),
            },
        },
        view.texture().size(),
    );
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let data = buffer.slice(..).get_mapped_range().unwrap();
    (0..SIDE as usize)
        .flat_map(|y| {
            data[y * row as usize..][..(SIDE * bytes) as usize]
                .iter()
                .copied()
        })
        .collect()
}
fn half(bits: u16) -> f64 {
    let e = (bits >> 10) & 31;
    let m = f64::from(bits & 1023) / 1024.0;
    if e == 0 {
        m * 2.0f64.powi(-14)
    } else {
        (1.0 + m) * 2.0f64.powi(i32::from(e) - 15)
    }
}
pub(super) fn verify() {
    let adapter =
        pollster::block_on(wgpu::Instance::default().request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let projection =
        glam::camera::rh::proj::directx::perspective(std::f32::consts::FRAC_PI_2, 1.0, 0.1, 100.0);
    let mut ao = ReferenceAo::new(&device, SIDE, SIDE);
    ao.gpu.noise = crate::render::sky::ReferenceNoise::fixture(&device, [0, 0, 192, 255]);
    let module=device.create_shader_module(wgpu::ShaderModuleDescriptor{label:Some("real depth discontinuities"),source:wgpu::ShaderSource::Wgsl(r#"
@group(0) @binding(0) var<storage,read> values:array<f32>;
@vertex fn vertex(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {let p=array<vec2f,3>(vec2f(-1,-1),vec2f(3,-1),vec2f(-1,3));return vec4f(p[i],0,1);}
struct Out {@location(0) color:vec4f,@builtin(frag_depth) depth:f32};
@fragment fn fragment(@builtin(position) p:vec4f)->Out {return Out(vec4f(2.0,1.0,0.5,0.75),values[u32(p.y)*64u+u32(p.x)]);}
"#.into())});
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: None,
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vertex"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fragment"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: super::super::super::HDR_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    for (case, frame) in [(0, 0), (1, 0), (1, 7), (2, 11)] {
        let depths: Vec<f32> = (0..SIDE * SIDE)
            .map(|i| {
                let x = i % SIDE;
                let y = i / SIDE;
                if case == 2 && x > 50 {
                    return 1.0;
                }
                let distance = if case > 0 && (24..40).contains(&x) && (18..46).contains(&y) {
                    2.65
                } else {
                    2.8
                };
                let p = projection * glam::Vec4::new(0.0, 0.0, -distance, 1.0);
                p.z / p.w
            })
            .collect();
        let oracle = oracle::Image {
            size: SIDE as usize,
            depth: depths.iter().map(|z| f64::from(*z)).collect(),
            inverse: DMat4::from_cols_array(&projection.to_cols_array().map(f64::from)).inverse(),
            frame,
        };
        let expected: Vec<f64> = (0..SIDE * SIDE)
            .map(|i| oracle.sample((i % SIDE) as usize, (i / SIDE) as usize))
            .collect();
        let scene = target(&device, super::super::super::HDR_FORMAT);
        let depth = target(&device, wgpu::TextureFormat::Depth32Float);
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&depths),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &scene,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
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
        let mut data = crate::render::daylight::Atmosphere::at(crate::daylight::INITIAL_MS)
            .camera_data(projection, Vec3::ZERO);
        data[31] = -1.0;
        ao.configure(data, std::f32::consts::FRAC_PI_2);
        ao.sample = frame;
        ao.resolve(&device, &queue, &mut encoder, &scene, &depth, true);
        queue.submit([encoder.finish()]);
        let visible = read(&device, &queue, &ao.visibility, 4);
        let color = read(&device, &queue, &scene, 8);
        let mut occluded = 0;
        for i in 0..(SIDE * SIDE) as usize {
            let actual = f64::from(visible[i * 4]) / 255.0;
            assert!(
                (actual - expected[i]).abs() <= 1.01 / 255.0,
                "AO case{case} frame{frame} pixel{i}: {actual} != {}",
                expected[i]
            );
            let composite = oracle.reconstruct(i % SIDE as usize, i / SIDE as usize, &expected);
            for (c, energy) in [2.0, 1.0, 0.5].into_iter().enumerate() {
                let result = half(u16::from_le_bytes([
                    color[i * 8 + c * 2],
                    color[i * 8 + c * 2 + 1],
                ]));
                assert!(
                    (result - energy * composite).abs() < 0.012,
                    "composite case{case} pixel{i}: {result} != {}",
                    energy * composite
                );
            }
            assert_eq!(
                u16::from_le_bytes([color[i * 8 + 6], color[i * 8 + 7]]),
                0x3a00,
                "alpha must remain unchanged"
            );
            if composite < 0.98 {
                occluded += 1;
            }
        }
        if case == 0 {
            assert_eq!(occluded, 0, "flat wall must not self occlude");
        } else {
            assert!(
                occluded > 20,
                "actual blocker must occlude adjacent surface"
            );
        }
    }
    ao.resize(&device, 17, 13);
    assert_eq!(ao.visibility.texture().width(), 17);
    assert_eq!(ao.sample, 0);
    assert!(ao.camera.is_none());
}
