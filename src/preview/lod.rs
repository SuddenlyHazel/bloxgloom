//! Production distant GPU pipeline with repeatable terrain and structure views.
use super::*;
use crate::lod::{Column, Interval, LodTile, Span, TILE_COLUMNS, TileKey};
use render::lod::{FaceColors, Gpu, Mesh};

pub(super) fn terrain_meshes(
    camera: Camera,
    horizon: u16,
) -> Result<(Vec<Mesh>, usize), Box<dyn Error>> {
    let started = Instant::now();
    let catalog = crate::content::catalog();
    let quality = match std::env::var("BLOXGLOOM_LOD_QUALITY") {
        Ok(value) => value
            .parse::<u8>()
            .ok()
            .filter(|v| *v <= 2)
            .ok_or("BLOXGLOOM_LOD_QUALITY must be 0, 1 or 2")?,
        Err(std::env::VarError::NotPresent) => 1,
        Err(error) => return Err(error.into()),
    };
    let keys = render::lod::desired_tiles(camera.position, horizon, quality, 4);
    let mut tiles = vec![];
    let mut unavailable = 0;
    for key in keys {
        match world::lod::builtin_lod_tile(key, 1, SEED, catalog) {
            Ok(tile) => tiles.push(tile),
            Err(error) => {
                unavailable += 1;
                tracing::warn!(?key, %error, "preview summary unavailable");
                if key.level == 4 {
                    return Err(format!("coarse skyline unavailable: {key:?}: {error}").into());
                }
            }
        }
    }
    let summary_ms = started.elapsed().as_secs_f64() * 1000.0;
    let summary_bytes: usize = tiles.iter().map(LodTile::encoded_bytes).sum();
    let summaries: std::collections::HashMap<_, _> =
        tiles.iter().map(|tile| (tile.key, tile)).collect();
    let mut reduction_elapsed = std::time::Duration::ZERO;
    let mut reduction_parents = 0;
    let mut reduction_unavailable = 0;
    for tile in &tiles {
        let Some(keys) = tile.key.children() else {
            continue;
        };
        let [Some(a), Some(b), Some(c), Some(d)] = keys.map(|key| summaries.get(&key).copied())
        else {
            continue;
        };
        let started = Instant::now();
        let reduced = std::hint::black_box(crate::lod::reduce_parent(
            tile.key,
            tile.revision,
            [a, b, c, d],
            catalog,
        ));
        reduction_elapsed += started.elapsed();
        reduction_parents += 1;
        reduction_unavailable += usize::from(reduced.is_err());
    }
    let reduction_ms = reduction_elapsed.as_secs_f64() * 1000.0;
    let colors = FaceColors::new(catalog);
    let started = Instant::now();
    let meshes = tiles
        .iter()
        .map(|tile| {
            let neighbors: Vec<_> = tiles
                .iter()
                .filter(|other| other.key != tile.key && touches(other.key, tile.key))
                .collect();
            render::lod::mesh(tile, &neighbors, catalog, &colors)
        })
        .collect::<Result<Vec<_>, _>>()?;
    eprintln!(
        "LOD scene: horizon={horizon} quality={quality} tiles={} unavailable={unavailable} summary={summary_ms:.2}ms summary_bytes={summary_bytes} reduction={reduction_ms:.2}ms reduction_parents={reduction_parents} reduction_unavailable={reduction_unavailable} meshing={:.2}ms mesh_bytes={} triangles={}",
        tiles.len(),
        started.elapsed().as_secs_f64() * 1000.0,
        meshes.iter().map(Mesh::byte_len).sum::<usize>(),
        meshes
            .iter()
            .map(|m| (m.indices.len() + m.water_indices.len()) / 3)
            .sum::<usize>()
    );
    Ok((meshes, summary_bytes))
}

pub(crate) fn render_lod_previews(directory: &Path, horizon: u16) -> Result<(), Box<dyn Error>> {
    if !matches!(horizon, 512 | 1024) {
        return Err("LOD preview horizon must be 512 or 1024".into());
    }
    fs::create_dir_all(directory)?;
    pollster::block_on(render_async(directory, horizon))
}

async fn render_async(directory: &Path, horizon: u16) -> Result<(), Box<dyn Error>> {
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
        .request_device(&wgpu::DeviceDescriptor {
            required_limits: render::material_device_limits(
                adapter.limits(),
                render::material_texture_layers(crate::content::catalog()) as usize,
            )?,
            ..Default::default()
        })
        .await?;
    let width = 1000;
    let height = 600;
    let (materials, _, _, _, textures) =
        render::create_voxel_pipeline(&device, &queue, render::post::HDR_FORMAT);
    let mut gpu = Gpu::new(&device, render::post::HDR_FORMAT, &materials, &textures);
    gpu.set_horizon(horizon);
    let camera = Camera {
        position: Vec3::new(8.5, 80.0, -24.0),
        yaw: 0.95,
        pitch: -0.16,
        fov_y_radians: 70.0f32.to_radians(),
    };
    let (meshes, _) = terrain_meshes(camera, horizon)?;
    for mesh in meshes {
        gpu.enqueue(mesh)
            .map_err(|_| "LOD preview GPU admission full")?;
        if gpu.upload(&device) == 0 {
            return Err("LOD preview exceeded GPU budget".into());
        }
    }
    let (sky, sky_buffer, sky_group) =
        render::create_sky_pipeline(&device, render::post::HDR_FORMAT);
    let transition_position = Vec3::new(
        100.0,
        world::terrain_height(100, -32, SEED) as f32 + 18.0,
        -32.0,
    );
    let transition_direction = (Vec3::new(
        155.0,
        world::terrain_height(155, 35, SEED) as f32 + 4.0,
        35.0,
    ) - transition_position)
        .normalize();
    let mut cases = vec![
        (
            "terrain-transition",
            Camera {
                position: transition_position,
                yaw: transition_direction.z.atan2(transition_direction.x),
                pitch: transition_direction.y.asin(),
                ..camera
            },
            crate::daylight::INITIAL_MS,
            0.0,
        ),
        ("terrain-noon", camera, crate::daylight::INITIAL_MS, 0.0),
        (
            "terrain-night",
            camera,
            crate::daylight::CYCLE_MS * 3 / 4,
            0.0,
        ),
        ("terrain-storm", camera, crate::daylight::INITIAL_MS, 1.0),
        (
            "terrain-negative",
            Camera {
                position: Vec3::new(-90.5, 80.0, -90.5),
                yaw: 0.3,
                ..camera
            },
            crate::daylight::INITIAL_MS,
            0.0,
        ),
    ];
    for (name, camera, time, storm) in cases.drain(..) {
        let mut atmosphere = render::daylight::Atmosphere::at(time);
        atmosphere.fog = storm;
        atmosphere.cloud = storm;
        gpu.prepare(
            &queue,
            camera,
            width,
            height,
            atmosphere,
            std::iter::empty(),
        );
        queue.write_buffer(
            &sky_buffer,
            0,
            bytemuck::cast_slice(&render::sky_camera_data(camera, width, height, atmosphere)),
        );
        draw_image(
            &device,
            &queue,
            &gpu,
            &sky,
            &sky_group,
            camera,
            atmosphere,
            width,
            height,
            &directory.join(format!("{name}.png")),
        )?;
    }
    gpu.clear();
    let colors = FaceColors::new(crate::content::catalog());
    let tile = structure_tile();
    gpu.enqueue(render::lod::mesh(
        &tile,
        &[],
        crate::content::catalog(),
        &colors,
    )?)
    .map_err(|_| "fixture admission")?;
    gpu.upload(&device);
    for (name, camera, time) in [
        (
            "bridge-cave-noon",
            Camera {
                position: Vec3::new(42.0, 14.0, -16.0),
                yaw: 2.0,
                pitch: -0.08,
                ..camera
            },
            crate::daylight::INITIAL_MS,
        ),
        (
            "bridge-cave-night",
            Camera {
                position: Vec3::new(42.0, 14.0, -16.0),
                yaw: 2.0,
                pitch: -0.08,
                ..camera
            },
            crate::daylight::CYCLE_MS * 3 / 4,
        ),
        (
            "bridge-cave-ready-3d",
            Camera {
                position: Vec3::new(42.0, 14.0, -16.0),
                yaw: 2.0,
                pitch: -0.08,
                ..camera
            },
            crate::daylight::INITIAL_MS,
        ),
    ] {
        let atmosphere = render::daylight::Atmosphere::at(time);
        gpu.prepare(
            &queue,
            camera,
            width,
            height,
            atmosphere,
            // Known-empty ready near geometry removes only this 3D volume.
            // The bridge at y=16 remains visible over its x/z footprint.
            std::iter::once(world::ChunkKey { x: 0, y: 0, z: 0 })
                .filter(|_| name == "bridge-cave-ready-3d"),
        );
        queue.write_buffer(
            &sky_buffer,
            0,
            bytemuck::cast_slice(&render::sky_camera_data(camera, width, height, atmosphere)),
        );
        draw_image(
            &device,
            &queue,
            &gpu,
            &sky,
            &sky_group,
            camera,
            atmosphere,
            width,
            height,
            &directory.join(format!("{name}.png")),
        )?;
    }
    gpu.clear();
    let water = water_tile();
    gpu.enqueue(render::lod::mesh(
        &water,
        &[],
        crate::content::catalog(),
        &colors,
    )?)
    .map_err(|_| "water fixture admission")?;
    gpu.upload(&device);
    let position = Vec3::new(34.0, 44.0, -16.0);
    let direction = (Vec3::new(32.0, 11.0, 32.0) - position).normalize();
    let water_camera = Camera {
        position,
        yaw: direction.z.atan2(direction.x),
        pitch: direction.y.asin(),
        ..camera
    };
    for (name, time, mask) in [
        ("water-noon", crate::daylight::INITIAL_MS, false),
        ("water-night", crate::daylight::CYCLE_MS * 3 / 4, false),
        ("water-ready-3d", crate::daylight::INITIAL_MS, true),
    ] {
        let atmosphere = render::daylight::Atmosphere::at(time);
        gpu.prepare(
            &queue,
            water_camera,
            width,
            height,
            atmosphere,
            std::iter::once(world::ChunkKey { x: 0, y: 0, z: 0 }).filter(|_| mask),
        );
        queue.write_buffer(
            &sky_buffer,
            0,
            bytemuck::cast_slice(&render::sky_camera_data(
                water_camera,
                width,
                height,
                atmosphere,
            )),
        );
        draw_image(
            &device,
            &queue,
            &gpu,
            &sky,
            &sky_group,
            water_camera,
            atmosphere,
            width,
            height,
            &directory.join(format!("{name}.png")),
        )?;
    }
    Ok(())
}

fn water_tile() -> LodTile {
    let mut tile = structure_tile();
    tile.key.level = 1;
    for z in 0..32 {
        for x in 0..32 {
            let wet = (4..28).contains(&x) && (4..28).contains(&z);
            let bed = if wet {
                if (x / 4 + z / 4) % 2 == 0 { 7 } else { 10 }
            } else {
                12
            };
            let column = &mut tile.columns[x + 32 * z];
            column.spans = vec![Span {
                bottom: 0,
                top: bed,
                state: if (x / 4 + z / 4) % 2 == 0 {
                    world::GRAVEL
                } else {
                    world::SAND
                },
                sky: 12,
                glow: 0,
            }];
            if wet {
                column.spans.push(Span {
                    bottom: bed,
                    top: 12,
                    state: world::WATER,
                    sky: 15,
                    glow: 0,
                });
            }
        }
    }
    tile
}

fn structure_tile() -> LodTile {
    let mut columns = vec![
        Column {
            coverage: vec![Interval { bottom: 0, top: 32 }],
            spans: vec![]
        };
        TILE_COLUMNS
    ];
    for z in 0..32 {
        for x in 0..32 {
            let c = &mut columns[x + 32 * z];
            c.spans.push(Span {
                bottom: 0,
                top: 4,
                state: world::STONE,
                sky: if (4..14).contains(&x) && (21..31).contains(&z) {
                    0
                } else {
                    15
                },
                glow: 0,
            });
            if (5..27).contains(&x) && (12..18).contains(&z) {
                c.spans.push(Span {
                    bottom: 16,
                    top: 18,
                    state: world::WOOD,
                    sky: 15,
                    glow: 0,
                });
            }
            if (4..14).contains(&x) && (21..31).contains(&z) {
                if x == 4 || x == 13 || z == 30 {
                    c.spans.push(Span {
                        bottom: 4,
                        top: 12,
                        state: world::STONE,
                        sky: 0,
                        glow: 0,
                    });
                } else {
                    c.spans.push(Span {
                        bottom: 10,
                        top: 12,
                        state: world::STONE,
                        sky: 15,
                        glow: 0,
                    });
                }
            }
        }
    }
    LodTile {
        key: TileKey {
            level: 0,
            x: 0,
            z: 0,
        },
        revision: 1,
        columns,
        geometric_error: 0,
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "Offscreen fixture shares production sky and LOD resources"
)]
fn draw_image(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    gpu: &Gpu,
    sky: &wgpu::RenderPipeline,
    sky_group: &wgpu::BindGroup,
    camera: Camera,
    atmosphere: render::daylight::Atmosphere,
    width: u32,
    height: u32,
    path: &Path,
) -> Result<(), Box<dyn Error>> {
    let mut post = render::post::PostProcess::new(device, width, height, FORMAT);
    post.reflections.configure(camera.position, atmosphere);
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("LOD preview image"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = color.create_view(&Default::default());
    let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("LOD preview depth"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: render::DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let depth = depth_texture.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("LOD preview"),
            color_attachments: &render::scene_ao::attachments(
                &post.scene,
                &post.ambient.indirect,
                &post.reflections.normal,
                &post.reflections.response,
                render::SKY_COLOR,
            ),
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
        pass.set_pipeline(sky);
        pass.set_bind_group(0, sky_group, &[]);
        pass.draw(0..3, 0..1);
        gpu.draw(&mut pass);
    }
    post.resolve_ambient(
        device,
        queue,
        &mut encoder,
        &depth,
        render::view_projection(camera, width, height),
    );
    gpu.draw_water_pass(
        &mut encoder,
        &post.scene,
        &post.ambient.indirect,
        &post.reflections.normal,
        &post.reflections.response,
        &depth,
    );
    post.resolve_reflections(
        device,
        queue,
        &mut encoder,
        &depth,
        render::view_projection(camera, width, height),
    );
    post.encode(device, queue, &mut encoder, &view);
    queue.submit(Some(encoder.finish()));
    super::capture::save_texture(device, queue, &color, width, height, path)
}

fn touches(a: TileKey, b: TileKey) -> bool {
    let (Some(a), Some(b)) = (a.bounds(), b.bounds()) else {
        return false;
    };
    ((a[2] == b[0] || b[2] == a[0]) && a[1] < b[3] && b[1] < a[3])
        || ((a[3] == b[1] || b[3] == a[1]) && a[0] < b[2] && b[0] < a[2])
}
