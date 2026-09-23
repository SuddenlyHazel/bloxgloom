//! Headless GPU render of the procedural world for visual inspection.
use std::{error::Error, fs::File, path::Path, sync::mpsc};

use glam::Vec3;
use wgpu::util::DeviceExt;

use crate::{
    render::{self, Camera},
    world::{self, ChunkKey},
};

const WIDTH: u32 = 1000;
const HEIGHT: u32 = 600;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const SEED: u64 = 0xB10C_6100;

pub fn render_preview(path: &Path) -> Result<(), Box<dyn Error>> {
    pollster::block_on(render_preview_async(path))
}

async fn render_preview_async(path: &Path) -> Result<(), Box<dyn Error>> {
    let instance = wgpu::Instance::default();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        })
        .await?;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default())
        .await?;
    let (pipeline, camera_buffer, camera_group) = render::create_voxel_pipeline(&device, FORMAT);

    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("preview color"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("preview depth"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: render::DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let color_view = color.create_view(&Default::default());
    let depth_view = depth.create_view(&Default::default());
    let camera_xz = (40, 16);
    let target_xz = (8, -16);
    let camera_position = Vec3::new(
        camera_xz.0 as f32 + 0.5,
        surface_height(camera_xz.0, camera_xz.1) as f32 + 18.0,
        camera_xz.1 as f32 + 0.5,
    );
    let target = Vec3::new(
        target_xz.0 as f32 + 0.5,
        surface_height(target_xz.0, target_xz.1) as f32 + 3.0,
        target_xz.1 as f32 + 0.5,
    );
    let direction = (target - camera_position).normalize();
    let camera = Camera {
        position: camera_position,
        yaw: direction.z.atan2(direction.x),
        pitch: direction.y.asin(),
        fov_y_radians: 70f32.to_radians(),
    };
    let matrix = render::view_projection(camera, WIDTH, HEIGHT);
    queue.write_buffer(
        &camera_buffer,
        0,
        bytemuck::cast_slice(&matrix.to_cols_array()),
    );

    let mut gpu_meshes = Vec::new();
    for z in -2..=2 {
        for x in -2..=2 {
            for y in 0..=4 {
                let chunk = world::generate_chunk(ChunkKey { x, y, z }, SEED);
                let mesh = render::mesh_chunk(&chunk);
                if mesh.indices.is_empty() {
                    continue;
                }
                let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("preview vertices"),
                    contents: bytemuck::cast_slice(&mesh.vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("preview indices"),
                    contents: bytemuck::cast_slice(&mesh.indices),
                    usage: wgpu::BufferUsages::INDEX,
                });
                gpu_meshes.push((vertices, indices, mesh.indices.len() as u32));
            }
        }
    }

    let bytes_per_row = WIDTH * 4;
    let padded_bytes_per_row = bytes_per_row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("preview readback"),
        size: u64::from(padded_bytes_per_row) * u64::from(HEIGHT),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("preview commands"),
    });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("preview pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.04,
                        g: 0.06,
                        b: 0.09,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
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
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &camera_group, &[]);
        for (vertices, indices, count) in &gpu_meshes {
            pass.set_vertex_buffer(0, vertices.slice(..));
            pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..*count, 0, 0..1);
        }
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
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(HEIGHT),
            },
        },
        wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
    );
    let submission = queue.submit(Some(encoder.finish()));
    let (sender, receiver) = mpsc::channel();
    readback.map_async(wgpu::MapMode::Read, .., move |result| {
        let _ = sender.send(result);
    });
    device.poll(wgpu::PollType::Wait {
        submission_index: Some(submission),
        timeout: Some(std::time::Duration::from_secs(30)),
    })?;
    receiver.recv()??;
    let mapped = readback.get_mapped_range(..)?;
    let mut pixels = Vec::with_capacity((bytes_per_row * HEIGHT) as usize);
    for row in mapped.chunks_exact(padded_bytes_per_row as usize) {
        pixels.extend_from_slice(&row[..bytes_per_row as usize]);
    }
    drop(mapped);
    readback.unmap();

    let file = File::create(path)?;
    let mut encoder = png::Encoder::new(file, WIDTH, HEIGHT);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
    encoder.write_header()?.write_image_data(&pixels)?;
    Ok(())
}

fn surface_height(x: i32, z: i32) -> i32 {
    let local_x = x.rem_euclid(world::CHUNK_SIZE as i32) as usize;
    let local_z = z.rem_euclid(world::CHUNK_SIZE as i32) as usize;
    for chunk_y in (0..=4).rev() {
        let chunk = world::generate_chunk(
            ChunkKey {
                x: x.div_euclid(world::CHUNK_SIZE as i32),
                y: chunk_y,
                z: z.div_euclid(world::CHUNK_SIZE as i32),
            },
            SEED,
        );
        for local_y in (0..world::CHUNK_SIZE).rev() {
            if chunk
                .block([local_x, local_y, local_z])
                .unwrap_or(world::AIR)
                != world::AIR
            {
                return chunk_y * world::CHUNK_SIZE as i32 + local_y as i32;
            }
        }
    }
    0
}
