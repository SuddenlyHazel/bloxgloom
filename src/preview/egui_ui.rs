//! Headless screenshots of the exact egui proof document used in live play.

use super::*;

pub fn render_egui_previews(directory: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(directory)?;
    pollster::block_on(render(directory))
}

async fn render(directory: &Path) -> Result<(), Box<dyn Error>> {
    // Use the same world scene as the native UI preview so the overlay is
    // judged in the setting where it actually appears.
    let backdrops = [(1280, 720), (640, 360)]
        .into_iter()
        .map(|(width, height)| PreviewOutput {
            path: directory.join(format!("egui-scene-{width}x{height}.png")),
            width,
            height,
            scale: 1.0,
            screen: UiScreen::Inventory,
            orientation: None,
        })
        .collect();
    render_previews(backdrops, (0, 0), PreviewScene::Surface).await?;
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
    let mut renderer = egui_wgpu::Renderer::new(&device, FORMAT, Default::default());
    for (width, height) in [(1280, 720), (640, 360)] {
        let color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("egui preview color"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let backdrop = read_rgba_png(&directory.join(format!("egui-scene-{width}x{height}.png")))?;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &color,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &backdrop,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let view = color.create_view(&Default::default());
        let context = render::egui_proof::themed_context();
        let frame = preview_frame(UiScreen::Container, None, 1.0);
        let mut search = String::new();
        let mut filter = render::egui_proof::SlotFilter::All;
        let mut intents = Vec::new();
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width as f32, height as f32),
                )),
                ..Default::default()
            },
            |ui| {
                render::egui_proof::draw(
                    ui,
                    &frame,
                    crate::content::catalog(),
                    &mut search,
                    &mut filter,
                    &mut intents,
                );
            },
        );
        for (id, deltas) in output.textures_delta.set.drain() {
            for delta in &deltas {
                renderer.update_texture(&device, &queue, id, delta);
            }
        }
        let paint = context.tessellate(output.shapes, output.pixels_per_point);
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [width, height],
            pixels_per_point: output.pixels_per_point,
        };
        let bytes_per_row = width * 4;
        let padded_bytes_per_row = bytes_per_row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("egui preview readback"),
            size: u64::from(padded_bytes_per_row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("egui preview"),
        });
        renderer.update_buffers(&device, &queue, &mut encoder, &paint, &screen);
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui preview pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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
            renderer.render(&mut pass.forget_lifetime(), &paint, &screen);
        }
        for id in output.textures_delta.free.drain() {
            renderer.free_texture(&id);
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
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
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
        let mut pixels = Vec::with_capacity((bytes_per_row * height) as usize);
        for row in mapped.chunks_exact(padded_bytes_per_row as usize) {
            pixels.extend_from_slice(&row[..bytes_per_row as usize]);
        }
        drop(mapped);
        readback.unmap();
        write_png(
            &directory.join(format!("egui-container-{width}x{height}.png")),
            width,
            height,
            &pixels,
        )?;
    }
    Ok(())
}

fn read_rgba_png(path: &Path) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut reader = png::Decoder::new(std::io::BufReader::new(File::open(path)?)).read_info()?;
    let mut pixels = vec![
        0;
        reader
            .output_buffer_size()
            .ok_or("missing PNG buffer size")?
    ];
    let info = reader.next_frame(&mut pixels)?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return Err("egui preview background must be 8-bit RGBA".into());
    }
    pixels.truncate(info.buffer_size());
    Ok(pixels)
}
