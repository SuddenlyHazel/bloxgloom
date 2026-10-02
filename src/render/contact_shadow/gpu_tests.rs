//! Offscreen regression for the actual HDR blend/depth/day-night shader.
use super::*;
use crate::render::{DEPTH_FORMAT, daylight, post};
use wgpu::util::DeviceExt;

const SIDE: u32 = 32;

#[test]
fn gpu_shadow_only_darkens_lit_visible_ground_and_fades_at_night() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let render =
        |light, strength, depth, color| render(&device, &queue, light, strength, depth, color);
    let day = render([1.0, 0.0], 1.0, 1.0, wgpu::Color::WHITE);
    let night = render([1.0, 0.0], 0.035, 1.0, wgpu::Color::WHITE);
    let dark = render([0.0, 0.0], 1.0, 1.0, wgpu::Color::WHITE);
    let hidden = render([1.0, 0.0], 1.0, 0.0, wgpu::Color::WHITE);
    let black = render([1.0, 0.0], 1.0, 1.0, wgpu::Color::BLACK);
    let torch = render([0.0, 0.8], 0.035, 1.0, wgpu::Color::WHITE);
    // Half-float bit order is monotonic for the nonnegative [0,1] colors here.
    // 0x3c00 is exactly 1.0, avoiding extra half-float test dependencies.
    let center = ((SIDE / 2 * SIDE + SIDE / 2) * 4) as usize;
    assert!(day[center] < 0x3b00, "visible floor must receive contact");
    assert!(night[center] > day[center] + 100, "night contact must fade");
    assert!(
        torch[center] < 0x3b00,
        "local glow retains contact at night"
    );
    assert!(
        day.chunks_exact(4)
            .all(|p| p[..3].iter().all(|&c| c <= 0x3c00))
    );
    assert!(
        day.chunks_exact(4).all(|p| p[3] == 0x3c00),
        "alpha stays unchanged"
    );
    for unchanged in [&dark, &hidden] {
        assert!(
            unchanged.iter().all(|&c| c == 0x3c00),
            "darkness and nearer depth reject contact"
        );
    }
    assert!(
        black.chunks_exact(4).all(|p| p[..3] == [0; 3]),
        "sealed black cannot gain light"
    );
    assert_eq!(day[0], 0x3c00, "outside the soft radius remains untouched");
}

fn render(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    light: [f32; 2],
    strength: f32,
    depth_value: f32,
    clear: wgpu::Color,
) -> Vec<u16> {
    let eye = Vec3::new(0.5, 4.0, 0.5);
    let matrix = glam::camera::rh::proj::directx::orthographic(-1.0, 1.0, -1.0, 1.0, 0.1, 10.0)
        * glam::camera::rh::view::look_at_mat4(eye, Vec3::new(0.5, 1.0, 0.5), Vec3::Z);
    let mut atmosphere = daylight::Atmosphere::at(crate::daylight::INITIAL_MS);
    atmosphere.strength = strength;
    let camera = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("contact shadow regression camera"),
        contents: bytemuck::cast_slice(&atmosphere.camera_data(matrix, eye)),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let mut renderer = Renderer::new(device, &camera);
    renderer.set(
        queue,
        &[Patch {
            bounds: [-0.08, -0.08, 1.08, 1.08],
            center: [0.5, 0.5, 1.003, 0.58],
            light: [0.26, light[0], light[1], 0.0],
        }],
    );
    let size = wgpu::Extent3d {
        width: SIDE,
        height: SIDE,
        depth_or_array_layers: 1,
    };
    let texture = |format, usage| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    };
    let color = texture(
        post::HDR_FORMAT,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    let depth = texture(DEPTH_FORMAT, wgpu::TextureUsages::RENDER_ATTACHMENT);
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(SIDE * SIDE * 8),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color.create_view(&Default::default()),
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth.create_view(&Default::default()),
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(depth_value),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        assert_eq!(renderer.draw(&mut pass), 2);
    }
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &color,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(SIDE * 8),
                rows_per_image: Some(SIDE),
            },
        },
        size,
    );
    queue.submit(Some(encoder.finish()));
    let (tx, rx) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(10)),
        })
        .unwrap();
    rx.recv_timeout(std::time::Duration::from_secs(10))
        .unwrap()
        .unwrap();
    let data = readback.slice(..).get_mapped_range().unwrap();
    let samples = data
        .chunks_exact(2)
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
        .collect();
    drop(data);
    readback.unmap();
    samples
}
