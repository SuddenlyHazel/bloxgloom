//! Headless screenshots of the exact egui proof document used in live play.

use super::*;
mod audio;
mod chat;

pub fn render_egui_previews(directory: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(directory)?;
    pollster::block_on(render(directory, None))
}

pub fn render_package_egui_previews(directory: &Path, root: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(directory)?;
    pollster::block_on(render(directory, Some(root)))
}

async fn render(directory: &Path, root: Option<&Path>) -> Result<(), Box<dyn Error>> {
    // Use the same world scene as the native UI preview so the overlay is
    // judged in the setting where it actually appears.
    let backdrops = [(1280, 720), (640, 360)]
        .into_iter()
        .map(|(width, height)| PreviewOutput {
            path: directory.join(format!("egui-scene-{width}x{height}.png")),
            width,
            height,
            scale: 1.0,
            screen: UiScreen::Playing,
            orientation: None,
        })
        .collect();
    render_previews(backdrops, (0, 0), PreviewScene::SurfaceBare).await?;
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
    let mut character_preview =
        render::CharacterPreview::new(&device, &queue, crate::content::catalog());
    let character_texture = renderer.register_native_texture(
        &device,
        &character_preview.sampled,
        wgpu::FilterMode::Nearest,
    );
    let default_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/packages");
    let package_snapshot = crate::server::PackageSnapshot::discover(root.unwrap_or(&default_root))?;
    let package_resources = Arc::clone(
        package_snapshot
            .client_bundle()
            .ui()
            .ok_or("missing package UI")?,
    );
    let mut package_session = crate::ui::authored::Session::new(Arc::clone(&package_resources));
    let mut updated_session = crate::ui::authored::Session::new(Arc::clone(&package_resources));
    if root.is_some() {
        for session in [&mut package_session, &mut updated_session] {
            if let Some((_, key)) = session.declared_bindings().first() {
                session.binding_key(*key, &Default::default(), Default::default(), false, false);
                session.wait_for_presentation()?;
            }
        }
        if let Some(index) = updated_session.node_index("recipe:browser/search") {
            // Representative post-craft data for offline visual QA. Live play
            // supplies these snapshots exclusively from accepted server packets.
            let catalog = crate::content::Catalog::builtins();
            let mut inventory = crate::inventory::Inventory {
                revision: 2,
                slots: std::array::from_fn(|_| {
                    Some(crate::inventory::Stack::new(crate::items::STICK, 128))
                }),
            };
            let stone = catalog
                .item_by_key("bloxgloom:stone")
                .ok_or("missing stone")?;
            let gravel = catalog
                .item_by_key("bloxgloom:gravel")
                .ok_or("missing gravel")?;
            inventory.slots[0] = Some(crate::inventory::Stack::new(stone, 5));
            inventory.slots[1] = Some(crate::inventory::Stack::new(gravel, 128));
            inventory.slots[2] =
                crate::inventory::Stack::with_components(stone, 4, 1, vec![0, 255]);
            updated_session.observe(
                "replica:inventory",
                String::new(),
                Arc::new(crate::client::presentation::Observations {
                    inventory: Some(crate::client::presentation::InventoryView::from_inventory(
                        &inventory, &catalog,
                    )?),
                    world: Some(crate::client::presentation::WorldView {
                        elapsed_ms: crate::daylight::INITIAL_MS,
                        cycle_ms: crate::daylight::CYCLE_MS,
                    }),
                    actions: vec![crate::client::presentation::ActionView {
                        spawned: Default::default(),
                        id: (1u128 << 64) | 1,
                        key: Some("recipe:craft".into()),
                        accepted: true,
                        reason: String::new(),
                    }],
                    ..Default::default()
                }),
            );
            updated_session.apply_egui(crate::ui::authored::EguiIntent::Input(
                index,
                "crush".into(),
            ));
            updated_session.wait_for_presentation()?;
        }
    } else {
        updated_session.apply_egui(crate::ui::authored::EguiIntent::Input(
            4,
            "café garden".into(),
        ));
        updated_session.wait_for_presentation()?;
    }
    let screens = [
        (UiScreen::Playing, "playing"),
        (UiScreen::Playing, "playing-debug"),
        (UiScreen::Playing, "chat-open"),
        (UiScreen::Playing, "chat-open-debug"),
        (UiScreen::Playing, "chat-closed"),
        (UiScreen::Container, "container"),
        (UiScreen::Pause, "pause"),
        (UiScreen::Settings, "settings"),
        (UiScreen::Graphics, "graphics"),
        (UiScreen::Audio, "audio"),
        (UiScreen::Audio, "audio-mixer"),
        (UiScreen::Audio, "audio-wind"),
        (UiScreen::Audio, "audio-cicadas"),
        (UiScreen::Audio, "audio-thunder"),
        (UiScreen::Audio, "audio-spatial"),
        (UiScreen::Audio, "audio-weather"),
        (UiScreen::Character, "character"),
        (UiScreen::Admin, "admin"),
        (UiScreen::Admin, "admin-flying"),
        (UiScreen::Admin, "admin-walking"),
        (UiScreen::Admin, "admin-flight-pending"),
        (UiScreen::Package, "package"),
        (UiScreen::Package, "package-updated"),
        (UiScreen::Joining, "joining"),
        (UiScreen::Joining, "join-cached"),
        (UiScreen::Joining, "join-verifying"),
        (UiScreen::JoinFailed, "join-failed"),
    ];
    for (width, height, screen_kind, label) in
        [(1280, 720), (640, 360)]
            .into_iter()
            .flat_map(|(width, height)| {
                screens
                    .into_iter()
                    .map(move |(screen, label)| (width, height, screen, label))
            })
    {
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
        let context = render::game_ui::themed_context();
        let atlas =
            (screen_kind == UiScreen::Package).then(|| package_resources.install_egui(&context));
        let package = if label == "package-updated" {
            &updated_session
        } else {
            &package_session
        };
        let preview = preview_frame(screen_kind, None, 1.0);
        let chat_preview = chat::session(label.starts_with("chat-open"));
        let frame = UiFrame {
            chat: label.starts_with("chat-").then_some(&chat_preview),
            debug: if label.ends_with("-debug") {
                Some(crate::ui::UiDebug {
                    position: [-0.1, 80.5, -16.1],
                    fps: 60.0,
                    frame_ms: 16.6,
                    visible_chunks: 80,
                    cached_chunks: 120,
                    latency_ms: Some(24),
                })
            } else {
                preview.debug
            },
            admin_enabled: screen_kind != UiScreen::Admin || label != "admin",
            flying: label != "admin-walking",
            flying_pending: label == "admin-flight-pending",
            character: (screen_kind == UiScreen::Character).then_some(crate::ui::CharacterPanel {
                packaged: None,
                cosmetics: [0; 4],
                recipe: Some(crate::appearance::CharacterRecipe {
                    hair: 2,
                    eyes: 0,
                    mouth: 0,
                    iris: Some([36, 220, 95]),
                    ..Default::default()
                }),
                can_apply: true,
                pending: false,
                status: "Unapplied changes",
                clip: 1,
                time: 0.2,
                preview: Some(character_texture),
            }),
            package_ui: (screen_kind == UiScreen::Package).then_some(package),
            join_address: matches!(screen_kind, UiScreen::Joining | UiScreen::JoinFailed)
                .then_some("127.0.0.1:25565"),
            join_progress: (screen_kind == UiScreen::Joining).then_some(crate::ui::JoinProgress {
                received: if label == "join-verifying" {
                    800 * 1024
                } else if label == "join-cached" {
                    0
                } else {
                    384 * 1024
                },
                total: 800 * 1024,
                cached: label == "join-cached",
            }),
            status: match screen_kind {
                UiScreen::Joining => Some(match label {
                    "join-cached" => "Reusing verified package cache",
                    "join-verifying" => "Verifying downloaded package",
                    _ => "Downloading package content",
                }),
                UiScreen::JoinFailed => Some("Connection failed during package verification"),
                _ => preview.status,
            },
            ..preview
        };
        let mut search = String::new();
        let mut filter = render::game_ui::SlotFilter::All;
        let mut intents = Vec::new();
        let mut draw = |events| {
            context.run_ui(
                egui::RawInput {
                    events,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width as f32, height as f32),
                    )),
                    ..Default::default()
                },
                |ui| {
                    render::game_ui::draw_screen(
                        ui,
                        &frame,
                        crate::content::catalog(),
                        atlas.as_ref().map(egui::TextureHandle::id),
                        &mut search,
                        &mut filter,
                        &mut intents,
                    );
                },
            )
        };
        let mut output = if label.starts_with("audio-") {
            audio::prepare(label, egui::vec2(width as f32, height as f32), &mut draw)?
        } else {
            draw(Vec::new())
        };
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
        if let Some(panel) = frame.character {
            character_preview.encode(&queue, &mut encoder, panel);
        }
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
            &directory.join(format!("egui-{label}-{width}x{height}.png")),
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
