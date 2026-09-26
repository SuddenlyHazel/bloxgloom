//! Headless GPU renders of the world and each interface screen.
mod perf;

use perf::run_perf_benchmark_async;
use std::{
    collections::{HashMap, VecDeque},
    error::Error,
    fs::{self, File},
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    time::Instant,
};

use glam::Vec3;
use wgpu::util::DeviceExt;

use crate::{
    client::drops::DropAnimator,
    inventory::{SLOTS, Stack},
    items::SEEDS,
    lighting::LightField,
    protocol::DroppedItem,
    render::{self, Camera, ChunkMesh},
    ui::{self, SettingId, UiControl, UiFrame, UiScreen, UiSettings},
    world::{self, ChunkKey},
};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const SEED: u64 = 0xB10C_6100;
const PERF_WIDTH: u32 = 1280;
const PERF_HEIGHT: u32 = 720;
pub(crate) const PERF_RADIUS: u8 = 6;
pub(crate) const PERF_STEADY_FRAMES: usize = 300;
const MESHER_RESULT_CAPACITY: usize = 64;
const CLIENT_MESH_RESULT_BATCH: usize = 64;
const CLIENT_PENDING_UPLOADS: usize = 128;

pub fn render_preview(path: &Path, center_x: i32, center_z: i32) -> Result<(), Box<dyn Error>> {
    if !(i32::MIN + 128..=i32::MAX - 128).contains(&center_x)
        || !(i32::MIN + 128..=i32::MAX - 128).contains(&center_z)
    {
        return Err("preview center is too close to the i32 world-coordinate limit".into());
    }
    pollster::block_on(render_previews(
        vec![PreviewOutput {
            path: path.to_owned(),
            width: 1000,
            height: 600,
            scale: 1.0,
            screen: UiScreen::Playing,
            orientation: None,
        }],
        (center_x.div_euclid(16), center_z.div_euclid(16)),
        PreviewScene::Surface,
    ))
}

/// A close, repeatable composition for checking cutout foliage and plant silhouettes.
pub fn render_vegetation_preview(path: &Path) -> Result<(), Box<dyn Error>> {
    pollster::block_on(render_previews(
        vec![PreviewOutput {
            path: path.to_owned(),
            width: 1280,
            height: 720,
            scale: 1.0,
            screen: UiScreen::Playing,
            orientation: None,
        }],
        (0, 0),
        PreviewScene::Vegetation,
    ))
}

/// Write every screen at 1280x720 and 640x360 for headless visual inspection.
pub fn render_ui_previews(directory: &Path) -> Result<(), Box<dyn Error>> {
    let mut outputs = Vec::with_capacity(8);
    for (width, height, suffix) in [(1280, 720, "1280x720"), (640, 360, "640x360")] {
        for (screen, name) in [
            (UiScreen::Playing, "playing"),
            (UiScreen::Inventory, "inventory"),
            (UiScreen::Admin, "admin"),
            (UiScreen::Pause, "pause"),
            (UiScreen::Settings, "settings"),
            (UiScreen::Graphics, "graphics"),
        ] {
            outputs.push(PreviewOutput {
                path: directory.join(format!("{name}-{suffix}.png")),
                width,
                height,
                scale: 1.0,
                screen,
                orientation: None,
            });
        }
    }
    for (screen, name) in [
        (UiScreen::Playing, "playing"),
        (UiScreen::Inventory, "inventory"),
        (UiScreen::Admin, "admin"),
        (UiScreen::Pause, "pause"),
        (UiScreen::Settings, "settings"),
        (UiScreen::Graphics, "graphics"),
    ] {
        outputs.push(PreviewOutput {
            path: directory.join(format!("{name}-640x360-scale2.png")),
            width: 640,
            height: 360,
            scale: 2.0,
            screen,
            orientation: None,
        });
    }
    let sun = render::SUN_DIRECTION.normalize();
    for (name, orientation) in [
        ("sun-facing", (sun.z.atan2(sun.x), sun.y.asin())),
        ("sun-away", ((-sun.z).atan2(-sun.x), 0.15)),
    ] {
        outputs.push(PreviewOutput {
            path: directory.join(format!("{name}-1280x720.png")),
            width: 1280,
            height: 720,
            scale: 1.0,
            screen: UiScreen::Playing,
            orientation: Some(orientation),
        });
    }
    fs::create_dir_all(directory)?;
    pollster::block_on(render_previews(outputs, (0, 0), PreviewScene::Surface))
}

pub fn render_lighting_previews(directory: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(directory)?;
    for (name, lamp, bounced) in [
        ("cave-dark.png", false, false),
        ("cave-lamp.png", true, false),
        ("cave-bounced.png", true, true),
    ] {
        pollster::block_on(render_previews(
            vec![PreviewOutput {
                path: directory.join(name),
                width: 1280,
                height: 720,
                scale: 1.0,
                screen: UiScreen::Playing,
                orientation: None,
            }],
            (0, 0),
            PreviewScene::Cave { lamp, bounced },
        ))?;
    }
    pollster::block_on(render_previews(
        vec![PreviewOutput {
            path: directory.join("natural-cavern.png"),
            width: 1280,
            height: 720,
            scale: 1.0,
            screen: UiScreen::Playing,
            orientation: None,
        }],
        (16, 20),
        PreviewScene::NaturalCavern,
    ))?;
    Ok(())
}

pub fn render_drop_preview(path: &Path) -> Result<(), Box<dyn Error>> {
    pollster::block_on(render_previews(
        vec![PreviewOutput {
            path: path.to_owned(),
            width: 1280,
            height: 720,
            scale: 1.0,
            screen: UiScreen::Playing,
            orientation: None,
        }],
        (0, 0),
        PreviewScene::Drops(DropPhase::Hover),
    ))
}

/// Inspect the production instanced avatar shader and silhouette offscreen.
pub fn render_avatar_preview(path: &Path) -> Result<(), Box<dyn Error>> {
    pollster::block_on(render_previews(
        vec![PreviewOutput {
            path: path.to_owned(),
            width: 1280,
            height: 720,
            scale: 1.0,
            screen: UiScreen::Playing,
            orientation: None,
        }],
        (0, 0),
        PreviewScene::Avatars,
    ))
}

pub fn render_drop_animation_previews(directory: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(directory)?;
    for (name, phase) in [
        ("pop.png", DropPhase::Pop),
        ("hover.png", DropPhase::Hover),
        ("pickup.png", DropPhase::Pickup),
    ] {
        pollster::block_on(render_previews(
            vec![PreviewOutput {
                path: directory.join(name),
                width: 1280,
                height: 720,
                scale: 1.0,
                screen: UiScreen::Playing,
                orientation: None,
            }],
            (0, 0),
            PreviewScene::Drops(phase),
        ))?;
    }
    Ok(())
}

/// Render the production voxel, target-outline, and playing-HUD passes offscreen while
/// exercising the same bounded chunk upload path used by the windowed renderer.
pub fn run_perf_benchmark(
    steady_frames: usize,
    radius: u8,
    bounced: bool,
) -> Result<(), Box<dyn Error>> {
    pollster::block_on(run_perf_benchmark_async(steady_frames, radius, bounced))
}

struct PreviewOutput {
    path: PathBuf,
    width: u32,
    height: u32,
    scale: f32,
    screen: UiScreen,
    orientation: Option<(f32, f32)>,
}

#[derive(Clone, Copy)]
enum DropPhase {
    Pop,
    Hover,
    Pickup,
}

#[derive(Clone, Copy)]
enum PreviewScene {
    Surface,
    Vegetation,
    Drops(DropPhase),
    Avatars,
    Cave { lamp: bool, bounced: bool },
    NaturalCavern,
}

async fn render_previews(
    outputs: Vec<PreviewOutput>,
    center_chunk: (i32, i32),
    scene: PreviewScene,
) -> Result<(), Box<dyn Error>> {
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
    let (sky_pipeline, sky_buffer, sky_group) =
        render::create_sky_pipeline(&device, render::post::HDR_FORMAT);
    let (pipeline, cutout_pipeline, camera_buffer, camera_group, texture_group) =
        render::create_voxel_pipeline(&device, &queue, render::post::HDR_FORMAT);
    let mut avatar_renderer =
        render::AvatarRenderer::new(&device, render::post::HDR_FORMAT, &camera_buffer);
    let (target_pipeline, target_camera_buffer, target_camera_group, target_vertices) =
        render::create_target_pipeline(&device, FORMAT);
    let mut ui_renderer = ui::UiRenderer::new(&device, &queue, FORMAT);
    measure_ui_prepare(&mut ui_renderer, &queue);
    let center_x = center_chunk.0 * 16;
    let center_z = center_chunk.1 * 16;
    let camera_xz = (center_x + 40, center_z + 16);
    let target_xz = (center_x + 8, center_z - 16);
    let target_height = if matches!(scene, PreviewScene::Vegetation) {
        world::terrain_height(i64::from(target_xz.0), i64::from(target_xz.1), SEED) as i32
    } else {
        surface_height(target_xz.0, target_xz.1)
    };
    let (camera_position, target) = match scene {
        PreviewScene::Surface => (
            Vec3::new(
                camera_xz.0 as f32 + 0.5,
                surface_height(camera_xz.0, camera_xz.1) as f32 + 18.0,
                camera_xz.1 as f32 + 0.5,
            ),
            Vec3::new(
                target_xz.0 as f32 + 0.5,
                target_height as f32 + 0.5,
                target_xz.1 as f32 + 0.5,
            ),
        ),
        PreviewScene::Vegetation => {
            let target = Vec3::new(
                target_xz.0 as f32 + 0.5,
                target_height as f32 + 2.0,
                target_xz.1 as f32 + 0.5,
            );
            (target + Vec3::new(9.0, 5.0, 11.0), target)
        }
        PreviewScene::Drops(_) | PreviewScene::Avatars => {
            let target = Vec3::new(
                target_xz.0 as f32 + 0.5,
                target_height as f32 + 1.0,
                target_xz.1 as f32 + 0.5,
            );
            let offset = if matches!(scene, PreviewScene::Avatars) {
                Vec3::new(5.5, 3.1, 7.0)
            } else {
                Vec3::new(4.0, 2.6, 5.0)
            };
            (target + offset, target)
        }
        PreviewScene::Cave { .. } => (Vec3::new(40.5, 12.0, 16.5), Vec3::new(29.5, 12.0, 16.5)),
        PreviewScene::NaturalCavern => {
            (Vec3::new(264.7, 3.3, 333.3), Vec3::new(264.0, -12.0, 327.0))
        }
    };
    let direction = (target - camera_position).normalize();
    let camera_template = Camera {
        position: camera_position,
        yaw: direction.z.atan2(direction.x),
        pitch: direction.y.asin(),
        fov_y_radians: 70f32.to_radians(),
    };

    let mut chunks = HashMap::new();
    let bottom_chunk = if matches!(scene, PreviewScene::NaturalCavern) {
        -4
    } else {
        0
    };
    for z in -2..=2 {
        for x in -2..=2 {
            for y in bottom_chunk..=4 {
                let key = ChunkKey {
                    x: center_chunk.0 + x,
                    y,
                    z: center_chunk.1 + z,
                };
                chunks.insert(key, Arc::new(world::generate_chunk(key, SEED)));
            }
        }
    }
    if let PreviewScene::Cave { lamp, .. } = scene {
        for y in 8..=16 {
            for z in 7..=24 {
                for x in 27..=46 {
                    set_preview_block(&mut chunks, x, y, z, world::STONE);
                }
            }
        }
        for y in 9..=15 {
            for z in 8..=23 {
                for x in 28..=45 {
                    set_preview_block(&mut chunks, x, y, z, world::AIR);
                }
            }
        }
        if lamp {
            set_preview_block(&mut chunks, 29, 12, 16, world::GLOWSTONE);
        }
        for y in 9..=13 {
            for z in 12..=20 {
                set_preview_block(&mut chunks, 27, y, z, world::MOSS);
            }
        }
    }
    if let PreviewScene::Vegetation = scene {
        let [tree_x, tree_z] = [target_xz.0 - 4, target_xz.1 - 5];
        let tree_ground = world::terrain_height(i64::from(tree_x), i64::from(tree_z), SEED) as i32;
        for z in tree_z - 11..=tree_z + 5 {
            for x in tree_x - 6..=tree_x + 6 {
                let ground = world::terrain_height(i64::from(x), i64::from(z), SEED) as i32;
                for y in ground + 1..=ground + 18 {
                    set_preview_block(&mut chunks, x, y, z, world::AIR);
                }
            }
        }
        for y in tree_ground + 1..=tree_ground + 5 {
            set_preview_block(&mut chunks, tree_x, y, tree_z, world::WOOD);
        }
        for dy in -2i32..=2 {
            for dz in -2i32..=2 {
                for dx in -2i32..=2 {
                    if dx * dx + dz * dz + dy * dy * 2 <= 8 {
                        set_preview_block(
                            &mut chunks,
                            tree_x + dx,
                            tree_ground + 5 + dy,
                            tree_z + dz,
                            world::LEAVES,
                        );
                    }
                }
            }
        }
        for dz in -5..=5 {
            for dx in -6..=6 {
                let x = target_xz.0 + dx;
                let z = target_xz.1 + dz;
                let block = match (dx + dz * 3).rem_euclid(11) {
                    0 => world::RED_FLOWER,
                    2 => world::YELLOW_FLOWER,
                    4 => world::BLUE_FLOWER,
                    6 => world::FERN,
                    8 | 9 => world::TALL_GRASS,
                    _ => continue,
                };
                let ground = world::terrain_height(i64::from(x), i64::from(z), SEED) as i32;
                set_preview_block(&mut chunks, x, ground + 1, z, block);
            }
        }
    }
    let mut gpu_meshes = Vec::new();
    for z in -2..=2 {
        for x in -2..=2 {
            for y in bottom_chunk..=4 {
                let key = ChunkKey {
                    x: center_chunk.0 + x,
                    y,
                    z: center_chunk.1 + z,
                };
                let chunk = &chunks[&key];
                let light = LightField::build_with_bounce(
                    key,
                    &chunks,
                    SEED,
                    matches!(scene, PreviewScene::Cave { bounced: true, .. }),
                );
                let mesh = render::mesh_chunk_lit(chunk, &light, 0);
                if mesh.indices.is_empty() && mesh.cutout_indices.is_empty() {
                    continue;
                }
                let upload = |vertices: &[f32], indices: &[u32]| {
                    if indices.is_empty() {
                        return None;
                    }
                    Some((
                        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("preview vertices"),
                            contents: bytemuck::cast_slice(vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        }),
                        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("preview indices"),
                            contents: bytemuck::cast_slice(indices),
                            usage: wgpu::BufferUsages::INDEX,
                        }),
                        indices.len() as u32,
                    ))
                };
                gpu_meshes.push((
                    upload(&mesh.vertices, &mesh.indices),
                    upload(&mesh.cutout_vertices, &mesh.cutout_indices),
                ));
            }
        }
    }

    let drop_gpu_mesh = if let PreviewScene::Drops(phase) = scene {
        let items: Vec<_> = [
            crate::items::ItemId::new(world::RED_FLOWER.get()),
            crate::items::ItemId::new(world::STONE.get()),
            crate::items::ItemId::new(world::GLOWSTONE.get()),
            SEEDS,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, item)| DroppedItem {
            id: index as u64 + 1,
            item,
            count: 1,
            position: [
                target_xz.0 as f32 + index as f32 - 0.5,
                target_height as f32 + 1.2 + index as f32 * 0.2,
                target_xz.1 as f32 + 0.5,
            ],
            age_ms: if matches!(phase, DropPhase::Pop) {
                0
            } else {
                2000
            },
        })
        .collect();
        let now = Instant::now();
        let mut animator = DropAnimator::new(now);
        animator.snapshot(items.clone(), now);
        let moment = match phase {
            DropPhase::Pop => now + std::time::Duration::from_millis(250),
            DropPhase::Hover => now,
            DropPhase::Pickup => {
                animator.picked_up(vec![items[1]], now);
                now + std::time::Duration::from_millis(180)
            }
        };
        let visuals = animator.visuals(moment, camera_position - Vec3::Y * 1.6);
        let meshes = render::mesh_dropped_items(&visuals);
        let upload = |vertices: &[f32], indices: &[u32]| {
            if indices.is_empty() {
                return None;
            }
            Some((
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("preview drops vertices"),
                    contents: bytemuck::cast_slice(vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("preview drops indices"),
                    contents: bytemuck::cast_slice(indices),
                    usage: wgpu::BufferUsages::INDEX,
                }),
                indices.len() as u32,
            ))
        };
        Some((
            upload(&meshes.opaque_vertices, &meshes.opaque_indices),
            upload(&meshes.cutout_vertices, &meshes.cutout_indices),
        ))
    } else {
        None
    };
    if matches!(scene, PreviewScene::Avatars) {
        avatar_renderer.set(
            &queue,
            &[
                render::VisualAvatar {
                    id: 1,
                    position: Vec3::new(
                        target_xz.0 as f32 - 1.25,
                        target_height as f32 + 1.0,
                        target_xz.1 as f32 + 0.5,
                    ),
                    cosmetics: [0, 0, 0, 0],
                    light_levels: [15, 0, 0, 0],
                    bounce: [0; 4],
                },
                render::VisualAvatar {
                    id: 2,
                    position: Vec3::new(
                        target_xz.0 as f32 + 0.5,
                        target_height as f32 + 1.0,
                        target_xz.1 as f32 + 0.5,
                    ),
                    cosmetics: [2, 4, 2, 0],
                    light_levels: [15, 0, 0, 0],
                    bounce: [0; 4],
                },
                render::VisualAvatar {
                    id: 3,
                    position: Vec3::new(
                        target_xz.0 as f32 + 2.25,
                        target_height as f32 + 1.0,
                        target_xz.1 as f32 + 0.5,
                    ),
                    cosmetics: [4, 1, 4, 0],
                    light_levels: [15, 0, 0, 0],
                    bounce: [0; 4],
                },
            ],
        );
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
        let mut camera = Camera {
            fov_y_radians: 70.0f32.to_radians(),
            ..camera_template
        };
        if let Some((yaw, pitch)) = output.orientation {
            camera.yaw = yaw;
            camera.pitch = pitch;
        }
        queue.write_buffer(
            &sky_buffer,
            0,
            bytemuck::cast_slice(&render::sky_camera_data(
                camera,
                output.width,
                output.height,
            )),
        );
        let matrix = render::view_projection(camera, output.width, output.height);
        queue.write_buffer(
            &camera_buffer,
            0,
            bytemuck::cast_slice(&matrix.to_cols_array()),
        );
        let has_target = matches!(scene, PreviewScene::Surface)
            && output.screen == UiScreen::Playing
            && output.orientation.is_none();
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
            output.scale,
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
        let post = render::post::PostProcess::new(&device, output.width, output.height, FORMAT);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("preview commands"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("preview world"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &post.scene,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(render::SKY_COLOR),
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
            pass.set_pipeline(&sky_pipeline);
            pass.set_bind_group(0, &sky_group, &[]);
            pass.draw(0..3, 0..1);
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &camera_group, &[]);
            pass.set_bind_group(1, &texture_group, &[]);
            for (opaque, _) in &gpu_meshes {
                if let Some((vertices, indices, count)) = opaque {
                    pass.set_vertex_buffer(0, vertices.slice(..));
                    pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..*count, 0, 0..1);
                }
            }
            if let Some(Some((vertices, indices, count))) =
                drop_gpu_mesh.as_ref().map(|mesh| &mesh.0)
            {
                pass.set_vertex_buffer(0, vertices.slice(..));
                pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..*count, 0, 0..1);
            }
            avatar_renderer.draw(&mut pass);
            pass.set_pipeline(&cutout_pipeline);
            for (_, cutout) in &gpu_meshes {
                if let Some((vertices, indices, count)) = cutout {
                    pass.set_vertex_buffer(0, vertices.slice(..));
                    pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..*count, 0, 0..1);
                }
            }
            if let Some(Some((vertices, indices, count))) =
                drop_gpu_mesh.as_ref().map(|mesh| &mesh.1)
            {
                pass.set_vertex_buffer(0, vertices.slice(..));
                pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..*count, 0, 0..1);
            }
        }
        post.encode(&mut encoder, &color_view);
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

fn sample_inventory() -> [Option<Stack>; SLOTS] {
    let mut slots = std::array::from_fn(|_| None);
    for (index, item, count) in [
        (0, 1, 128),
        (1, 2, 73),
        (2, 3, 64),
        (3, 4, 18),
        (4, 5, 27),
        (5, 6, 42),
        (6, 7, 9),
        (7, 8, 12),
        (10, 1, 96),
        (13, 3, 32),
        (20, 4, 7),
        (29, 6, 128),
    ] {
        slots[index] = Some(Stack::new(crate::items::ItemId::new(item), count));
    }
    slots
}

fn preview_frame(screen: UiScreen, target: Option<[i32; 3]>, scale: f32) -> UiFrame<'static> {
    UiFrame {
        screen,
        selected_slot: 1,
        inventory: sample_inventory(),
        inventory_source: (screen == UiScreen::Inventory).then_some(10),
        admin_enabled: true,
        admin_page: 0,
        admin_input: "give bloxgloom:stone 128",
        target,
        status: (screen == UiScreen::Playing).then_some("E OPENS INVENTORY  /  Q DROPS ITEM"),
        debug: None,
        settings: UiSettings {
            sensitivity: 0.002,
            fov_degrees: 70.0,
            view_distance: 3,
            scale,
            fullscreen: false,
            bounced_gi: false,
            ..UiSettings::default()
        },
        hovered: match screen {
            UiScreen::Playing => None,
            UiScreen::Inventory => Some(UiControl::InventorySlot(10)),
            UiScreen::Admin => Some(UiControl::AdminItem(0)),
            UiScreen::Pause => Some(UiControl::Resume),
            UiScreen::Settings => Some(UiControl::Increase(SettingId::FieldOfView)),
            UiScreen::Graphics => Some(UiControl::Increase(SettingId::Exposure)),
        },
    }
}

fn measure_ui_prepare(ui_renderer: &mut ui::UiRenderer, queue: &wgpu::Queue) {
    let frame = UiFrame {
        screen: UiScreen::Settings,
        selected_slot: 4,
        inventory: sample_inventory(),
        inventory_source: None,
        admin_enabled: false,
        admin_page: 0,
        admin_input: "",
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
        let mut changed = frame.clone();
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

fn set_preview_block(
    chunks: &mut HashMap<ChunkKey, Arc<world::Chunk>>,
    x: i32,
    y: i32,
    z: i32,
    block: world::BlockId,
) {
    let (key, local) = world::world_to_chunk(x, y, z);
    let chunk = Arc::make_mut(chunks.get_mut(&key).expect("preview scene chunk exists"));
    chunk.blocks.set(world::Chunk::index(local).unwrap(), block);
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
