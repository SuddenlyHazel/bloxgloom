//! Headless GPU renders of the world and each interface screen.
use std::{
    error::Error,
    fs::{self, File},
    path::{Path, PathBuf},
    sync::mpsc,
};

use glam::Vec3;
use wgpu::util::DeviceExt;

use crate::{
    render::{self, Camera},
    ui::{self, SettingId, UiControl, UiFrame, UiScreen, UiSettings},
    world::{self, ChunkKey},
};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const SEED: u64 = 0xB10C_6100;

pub fn render_preview(path: &Path) -> Result<(), Box<dyn Error>> {
    pollster::block_on(render_previews(vec![PreviewOutput {
        path: path.to_owned(),
        width: 1000,
        height: 600,
        screen: UiScreen::Playing,
    }]))
}

/// Write every screen at 1280x720 and 640x360 for headless visual inspection.
pub fn render_ui_previews(directory: &Path) -> Result<(), Box<dyn Error>> {
    let mut outputs = Vec::with_capacity(8);
    for (width, height, suffix) in [(1280, 720, "1280x720"), (640, 360, "640x360")] {
        for (screen, name) in [
            (UiScreen::Playing, "playing"),
            (UiScreen::Inventory, "inventory"),
            (UiScreen::Pause, "pause"),
            (UiScreen::Settings, "settings"),
        ] {
            outputs.push(PreviewOutput {
                path: directory.join(format!("{name}-{suffix}.png")),
                width,
                height,
                screen,
            });
        }
    }
    fs::create_dir_all(directory)?;
    pollster::block_on(render_previews(outputs))
}

struct PreviewOutput {
    path: PathBuf,
    width: u32,
    height: u32,
    screen: UiScreen,
}

async fn render_previews(outputs: Vec<PreviewOutput>) -> Result<(), Box<dyn Error>> {
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
    let (target_pipeline, target_camera_buffer, target_camera_group, target_vertices) =
        render::create_target_pipeline(&device, FORMAT);
    let mut ui_renderer = ui::UiRenderer::new(&device, &queue, FORMAT);
    measure_ui_prepare(&mut ui_renderer, &queue);
    let camera_xz = (40, 16);
    let target_xz = (8, -16);
    let target_height = surface_height(target_xz.0, target_xz.1);
    let camera_position = Vec3::new(
        camera_xz.0 as f32 + 0.5,
        surface_height(camera_xz.0, camera_xz.1) as f32 + 18.0,
        camera_xz.1 as f32 + 0.5,
    );
    let target = Vec3::new(
        target_xz.0 as f32 + 0.5,
        target_height as f32 + 0.5,
        target_xz.1 as f32 + 0.5,
    );
    let direction = (target - camera_position).normalize();
    let camera_template = Camera {
        position: camera_position,
        yaw: direction.z.atan2(direction.x),
        pitch: direction.y.asin(),
        fov_y_radians: 70f32.to_radians(),
    };

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

    for output in outputs {
        let color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("preview color"),
            size: wgpu::Extent3d {
                width: output.width,
                height: output.height,
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
                width: output.width,
                height: output.height,
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
        let camera = Camera {
            fov_y_radians: 70.0f32.to_radians(),
            ..camera_template
        };
        let matrix = render::view_projection(camera, output.width, output.height);
        queue.write_buffer(
            &camera_buffer,
            0,
            bytemuck::cast_slice(&matrix.to_cols_array()),
        );
        let has_target = output.screen == UiScreen::Playing;
        if has_target {
            queue.write_buffer(
                &target_camera_buffer,
                0,
                bytemuck::cast_slice(&matrix.to_cols_array()),
            );
            queue.write_buffer(
                &target_vertices,
                0,
                bytemuck::cast_slice(&render::target_outline_vertices([
                    target_xz.0,
                    target_height,
                    target_xz.1,
                ])),
            );
        }
        let ui_frame = preview_frame(
            output.screen,
            has_target.then_some([target_xz.0, target_height, target_xz.1]),
        );
        ui_renderer.prepare(&queue, output.width, output.height, &ui_frame);
        let bytes_per_row = output.width * 4;
        let padded_bytes_per_row = bytes_per_row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("preview readback"),
            size: u64::from(padded_bytes_per_row) * u64::from(output.height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("preview commands"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("preview world"),
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
        if has_target {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("preview target outline"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&target_pipeline);
            pass.set_bind_group(0, &target_camera_group, &[]);
            pass.set_vertex_buffer(0, target_vertices.slice(..));
            pass.draw(0..24, 0..1);
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("preview UI"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
            ui_renderer.encode(&mut pass);
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
                    rows_per_image: Some(output.height),
                },
            },
            wgpu::Extent3d {
                width: output.width,
                height: output.height,
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
        let mut pixels = Vec::with_capacity((bytes_per_row * output.height) as usize);
        for row in mapped.chunks_exact(padded_bytes_per_row as usize) {
            pixels.extend_from_slice(&row[..bytes_per_row as usize]);
        }
        drop(mapped);
        readback.unmap();
        write_png(&output.path, output.width, output.height, &pixels)?;
    }
    Ok(())
}

fn preview_frame(screen: UiScreen, target: Option<[i32; 3]>) -> UiFrame<'static> {
    UiFrame {
        screen,
        selected_slot: 1,
        hotbar: [1, 2, 3, 1, 2, 3, 1, 2, 3],
        target,
        status: (screen == UiScreen::Playing).then_some("CREATIVE MODE  /  E OPENS INVENTORY"),
        debug: None,
        catalog_selection: 2,
        settings: UiSettings {
            sensitivity: 0.002,
            fov_degrees: 70.0,
            view_distance: 3,
            scale: 1.0,
            fullscreen: false,
        },
        hovered: match screen {
            UiScreen::Playing => None,
            UiScreen::Inventory => Some(UiControl::CatalogBlock(2)),
            UiScreen::Pause => Some(UiControl::Resume),
            UiScreen::Settings => Some(UiControl::Increase(SettingId::FieldOfView)),
        },
    }
}

fn measure_ui_prepare(ui_renderer: &mut ui::UiRenderer, queue: &wgpu::Queue) {
    let frame = UiFrame {
        screen: UiScreen::Settings,
        selected_slot: 4,
        hotbar: [1, 2, 3, 1, 2, 3, 1, 2, 3],
        target: None,
        status: None,
        debug: Some(ui::UiDebug {
            position: [123.4, 65.0, -87.6],
            fps: 60.0,
            frame_ms: 16.6,
            visible_chunks: 80,
            cached_chunks: 120,
            latency_ms: Some(24),
        }),
        catalog_selection: 2,
        settings: UiSettings::default(),
        hovered: Some(UiControl::Increase(SettingId::FieldOfView)),
    };
    for _ in 0..40 {
        ui_renderer.prepare(queue, 1280, 720, &frame);
        queue.submit(std::iter::empty());
    }
    let mut cached_samples = Vec::with_capacity(300);
    for _ in 0..300 {
        let start = std::time::Instant::now();
        ui_renderer.prepare(queue, 1280, 720, &frame);
        cached_samples.push(start.elapsed().as_secs_f64() * 1_000.0);
        queue.submit(std::iter::empty());
    }
    let mut rebuilt_samples = Vec::with_capacity(300);
    for index in 0..300 {
        let mut changed = frame;
        changed.hovered = Some(if index % 2 == 0 {
            UiControl::Increase(SettingId::FieldOfView)
        } else {
            UiControl::Decrease(SettingId::FieldOfView)
        });
        let start = std::time::Instant::now();
        ui_renderer.prepare(queue, 1280, 720, &changed);
        rebuilt_samples.push(start.elapsed().as_secs_f64() * 1_000.0);
        queue.submit(std::iter::empty());
    }
    cached_samples.sort_by(f64::total_cmp);
    rebuilt_samples.sort_by(f64::total_cmp);
    let cached_median = cached_samples[cached_samples.len() / 2];
    let cached_p95 = cached_samples[(cached_samples.len() - 1) * 95 / 100];
    let rebuilt_median = rebuilt_samples[rebuilt_samples.len() / 2];
    let rebuilt_p95 = rebuilt_samples[(rebuilt_samples.len() - 1) * 95 / 100];
    eprintln!(
        "headless UI prepare at 1280x720, Settings + debug (300 frames): cached median {:.3} us, p95 {:.3} us; rebuilt median {rebuilt_median:.3} ms, p95 {rebuilt_p95:.3} ms",
        cached_median * 1_000.0,
        cached_p95 * 1_000.0,
    );
}

fn write_png(path: &Path, width: u32, height: u32, pixels: &[u8]) -> Result<(), Box<dyn Error>> {
    let file = File::create(path)?;
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
    encoder.write_header()?.write_image_data(pixels)?;
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
