//! Headless GPU renders of the world and each interface screen.
mod actors;
mod daylight;
pub use daylight::render_daylight_previews;
mod block;
mod visuals;
pub use visuals::render_visual_previews;
mod egui_ui;
pub use block::render_block_preview;
pub use egui_ui::{render_egui_previews, render_package_egui_previews};
mod perf;

pub(crate) use perf::characters::run_character_benchmark;
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

/// Synthetic committed-burn cue and colored spark for visual inspection.
pub fn render_fire_preview(path: &Path) -> Result<(), Box<dyn Error>> {
    pollster::block_on(render_previews(
        vec![PreviewOutput {
            path: path.to_owned(),
            width: 1000,
            height: 600,
            scale: 1.0,
            screen: UiScreen::Playing,
            orientation: None,
        }],
        (0, 0),
        PreviewScene::Fire,
    ))
}

pub fn render_effect_preview(path: &Path) -> Result<(), Box<dyn Error>> {
    pollster::block_on(render_previews(
        vec![PreviewOutput {
            path: path.to_owned(),
            width: 1000,
            height: 600,
            scale: 1.0,
            screen: UiScreen::Playing,
            orientation: None,
        }],
        (0, 0),
        PreviewScene::Effect,
    ))
}

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
            (UiScreen::Joining, "joining"),
            (UiScreen::JoinFailed, "join-failed"),
            (UiScreen::Inventory, "inventory"),
            (UiScreen::Container, "container"),
            (UiScreen::Actions, "actions"),
            (UiScreen::Admin, "commands"),
            (UiScreen::Admin, "commands-bindings"),
            (UiScreen::Pause, "pause"),
            (UiScreen::Settings, "settings"),
            (UiScreen::Graphics, "graphics"),
            (UiScreen::Package, "package"),
            (UiScreen::Package, "package-edited"),
            (UiScreen::Package, "package-planted"),
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
        (UiScreen::Joining, "joining"),
        (UiScreen::JoinFailed, "join-failed"),
        (UiScreen::Inventory, "inventory"),
        (UiScreen::Container, "container"),
        (UiScreen::Actions, "actions"),
        (UiScreen::Admin, "commands"),
        (UiScreen::Pause, "pause"),
        (UiScreen::Settings, "settings"),
        (UiScreen::Graphics, "graphics"),
        (UiScreen::Package, "package"),
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

/// Render a verified package's authored UI through the production UI renderer.
/// The client startup worker initializes its session text/state before drawing.
pub fn render_package_ui_previews(
    directory: &Path,
    package_root: &Path,
) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(directory)?;
    let outputs = [(1280, 720), (640, 360)]
        .into_iter()
        .map(|(width, height)| PreviewOutput {
            path: directory.join(format!("package-{width}x{height}.png")),
            width,
            height,
            scale: 1.0,
            screen: UiScreen::Package,
            orientation: None,
        })
        .collect();
    pollster::block_on(render_previews_with_packages(
        outputs,
        (0, 0),
        PreviewScene::Surface,
        Some(package_root),
    ))
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

/// Deterministic production character renderer, including authored one-shot clips.
pub fn render_character_preview(
    path: &Path,
    clip: &str,
    time: f32,
    hair: u8,
) -> Result<(), Box<dyn Error>> {
    let clip = match clip {
        "idle" => "idle",
        "walk" => "walk",
        "crouch" => "crouch",
        "tool_use_left" => "tool_use_left",
        "tool_use_right" => "tool_use_right",
        _ => return Err("unknown character clip".into()),
    };
    if usize::from(hair) >= crate::appearance::HAIR.len() {
        return Err("unknown hair ID".into());
    }
    if !time.is_finite() || time < 0.0 {
        return Err("time must be finite and nonnegative".into());
    }
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
        PreviewScene::Characters(clip, time, hair),
    ))
}

/// Three simultaneous authoritative-recipe render inputs, with distinct features.
pub fn render_character_styles(path: &Path) -> Result<(), Box<dyn Error>> {
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
        PreviewScene::CharacterStyles,
    ))
}

/// One world setup, then deterministic 30 Hz native animation frames.
pub fn render_character_motion(directory: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(directory)?;
    let outputs = (0..60)
        .map(|frame| PreviewOutput {
            path: directory.join(format!("{frame:03}.png")),
            width: 1280,
            height: 720,
            scale: 1.0,
            screen: UiScreen::Playing,
            orientation: None,
        })
        .collect();
    pollster::block_on(render_previews(
        outputs,
        (0, 0),
        PreviewScene::Characters("walk", 0.0, 1),
    ))
}

/// Production actor shader, with two mossbuns and a player for scale.
pub fn render_mossbun_preview(path: &Path) -> Result<(), Box<dyn Error>> {
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
        PreviewScene::Creature(crate::content::MOSSBUN_ENTITY_TYPE, None),
    ))
}

pub fn render_kiln_preview(path: &Path) -> Result<(), Box<dyn Error>> {
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
        PreviewScene::Kilns,
    ))
}

pub fn render_hopper_previews(directory: &Path) -> Result<(), Box<dyn Error>> {
    std::fs::create_dir_all(directory)?;
    let outputs = [
        ("chain.png", 1280, 720, UiScreen::Playing),
        ("hopper-ui.png", 1280, 720, UiScreen::Container),
        ("hopper-compact.png", 640, 360, UiScreen::Container),
    ]
    .into_iter()
    .map(|(name, width, height, screen)| PreviewOutput {
        path: directory.join(name),
        width,
        height,
        scale: 1.0,
        screen,
        orientation: None,
    })
    .collect();
    pollster::block_on(render_previews(outputs, (0, 0), PreviewScene::Hoppers))
}

pub fn render_inventory_previews(entity: &str, directory: &Path) -> Result<(), Box<dyn Error>> {
    let entity = crate::content::catalog()
        .entity_type_id_by_key(entity)
        .filter(|id| crate::content::catalog().inventory_screen(*id).is_some())
        .ok_or("entity has no registered inventory screen")?;
    std::fs::create_dir_all(directory)?;
    let outputs = [
        ("desktop.png", 1280, 720, 1.0),
        ("compact.png", 640, 360, 1.0),
        ("large-ui.png", 1280, 720, 1.8),
    ]
    .into_iter()
    .map(|(name, width, height, scale)| PreviewOutput {
        path: directory.join(name),
        width,
        height,
        scale,
        screen: UiScreen::Container,
        orientation: None,
    })
    .collect();
    pollster::block_on(render_previews(
        outputs,
        (0, 0),
        PreviewScene::Inventory(entity),
    ))
}

pub fn render_chest_previews(directory: &Path) -> Result<(), Box<dyn Error>> {
    std::fs::create_dir_all(directory)?;
    let outputs = [
        ("chain.png", 1280, 720, 1.0, UiScreen::Playing),
        ("chest-ui.png", 1280, 720, 1.0, UiScreen::Container),
        ("chest-compact.png", 640, 360, 1.0, UiScreen::Container),
        ("chest-large-ui.png", 1280, 720, 1.8, UiScreen::Container),
    ]
    .into_iter()
    .map(|(name, width, height, scale, screen)| PreviewOutput {
        path: directory.join(name),
        width,
        height,
        scale,
        screen,
        orientation: None,
    })
    .collect();
    pollster::block_on(render_previews(outputs, (0, 0), PreviewScene::Chests))
}

pub fn render_drop_animation_previews(directory: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(directory)?;
    for (name, phase) in [
        ("pop.png", DropPhase::Pop),
        ("hover.png", DropPhase::Hover),
        ("pickup.png", DropPhase::Pickup),
    ] {
        for name in [name.to_owned(), format!("custom-{name}")] {
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
    }
    Ok(())
}

pub fn render_mossbun_motion_previews(directory: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(directory)?;
    for frame in [0, 18, 30, 42, 52, 72] {
        pollster::block_on(render_previews(
            vec![PreviewOutput {
                path: directory.join(format!("frame-{frame:02}.png")),
                width: 1280,
                height: 720,
                scale: 1.0,
                screen: UiScreen::Playing,
                orientation: None,
            }],
            (0, 0),
            PreviewScene::MossbunMotion(frame),
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
    Block(crate::content::BlockStateId),
    Inventory(crate::content::EntityTypeId),
    Chests,
    Kilns,
    Hoppers,
    Surface,
    SurfaceBare,
    Effect,
    Fire,
    Vegetation,
    Drops(DropPhase),
    Avatars,
    Characters(&'static str, f32, u8),
    CharacterStyles,
    Creature(crate::content::EntityTypeId, Option<[f32; 3]>),
    MossbunMotion(u32),
    Cave { lamp: bool, bounced: bool },
    NaturalCavern,
}

pub fn render_creature_preview(
    key: &str,
    path: &Path,
    tint: Option<[f32; 3]>,
) -> Result<(), Box<dyn Error>> {
    let id = crate::content::catalog()
        .entity_type_id_by_key(key)
        .filter(|id| crate::content::catalog().mobile_entity(*id).is_some())
        .ok_or("unregistered creature")?;
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
        PreviewScene::Creature(id, tint),
    ))
}

async fn render_previews(
    outputs: Vec<PreviewOutput>,
    center_chunk: (i32, i32),
    scene: PreviewScene,
) -> Result<(), Box<dyn Error>> {
    render_previews_with_packages(outputs, center_chunk, scene, None).await
}

async fn render_previews_with_packages(
    outputs: Vec<PreviewOutput>,
    center_chunk: (i32, i32),
    scene: PreviewScene,
    package_root: Option<&Path>,
) -> Result<(), Box<dyn Error>> {
    render_previews_at(
        outputs,
        center_chunk,
        scene,
        package_root,
        crate::daylight::INITIAL_MS,
    )
    .await
}

async fn render_previews_at(
    outputs: Vec<PreviewOutput>,
    center_chunk: (i32, i32),
    scene: PreviewScene,
    package_root: Option<&Path>,
    world_time: u64,
) -> Result<(), Box<dyn Error>> {
    let atmosphere = render::daylight::Atmosphere::at(world_time);
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
    let (mut pipeline, mut cutout_pipeline, camera_buffer, camera_group, texture_group) =
        render::create_voxel_pipeline(&device, &queue, render::post::HDR_FORMAT);
    let mut fire_renderer = render::fire::FireRenderer::new(&device, &camera_buffer);
    let mut avatar_renderer = render::AvatarRenderer::new(
        &device,
        &queue,
        render::post::HDR_FORMAT,
        &camera_buffer,
        crate::content::catalog(),
    );
    let (target_pipeline, target_camera_buffer, target_camera_group, target_vertices) =
        render::create_target_pipeline(&device, FORMAT);
    let mut ui_renderer = ui::UiRenderer::new_with_catalog(
        &device,
        &queue,
        FORMAT,
        std::sync::Arc::new(crate::content::catalog().clone()),
    );
    let authored_preview = outputs
        .iter()
        .any(|output| output.screen == UiScreen::Package);
    // Authored previews are visual verification, not UI preparation benchmarks.
    if !authored_preview && !matches!(scene, PreviewScene::Effect | PreviewScene::Fire) {
        measure_ui_prepare(&mut ui_renderer, &queue);
    }
    let mut package_ui = if authored_preview {
        // Same secure discovery, canonical encoding and verification as a join.
        // All filesystem access and font/image work finish before frame drawing.
        let default_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/packages");
        let snapshot =
            crate::server::PackageSnapshot::discover(package_root.unwrap_or(&default_root))
                .map_err(|error| format!("package UI preview: {error:?}"))?;
        let bundle = match package_root {
            Some(root) => crate::server::package_bundle_for_preview(root)?,
            None => Arc::clone(snapshot.client_bundle()),
        };
        let resources = Arc::clone(bundle.ui().ok_or("missing package UI")?);
        ui_renderer.install_package_ui(&device, &queue, &resources);
        Some(match package_root {
            Some(_) => ui::authored::Session::with_startup(
                resources,
                crate::client::startup::prepare(bundle)?,
            ),
            None => ui::authored::Session::new(resources),
        })
    } else {
        None
    };
    let default_effect_root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/effect-packages");
    let mut visual_resources =
        if package_root.is_some() && !authored_preview || matches!(scene, PreviewScene::Effect) {
            Some(visuals::Resources::prepare(
                &device,
                &queue,
                package_root.unwrap_or(&default_effect_root),
                &mut pipeline,
                &mut cutout_pipeline,
            )?)
        } else {
            None
        };
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
        PreviewScene::Surface
        | PreviewScene::SurfaceBare
        | PreviewScene::Effect
        | PreviewScene::Inventory(_) => (
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
        PreviewScene::Drops(_)
        | PreviewScene::Fire
        | PreviewScene::Kilns
        | PreviewScene::Hoppers
        | PreviewScene::Chests
        | PreviewScene::Avatars
        | PreviewScene::Characters(..)
        | PreviewScene::CharacterStyles
        | PreviewScene::Creature(..)
        | PreviewScene::Block(_)
        | PreviewScene::MossbunMotion(_) => {
            let target = Vec3::new(
                target_xz.0 as f32 + 0.5,
                target_height as f32 + 1.0,
                target_xz.1 as f32 + 0.5,
            );
            let offset = if matches!(scene, PreviewScene::Fire) {
                Vec3::new(3.4, 1.8, 4.8)
            } else if matches!(
                scene,
                PreviewScene::Creature(..)
                    | PreviewScene::MossbunMotion(_)
                    | PreviewScene::Block(_)
            ) {
                Vec3::new(2.8, 1.7, 4.1)
            } else if matches!(scene, PreviewScene::Hoppers | PreviewScene::Chests) {
                Vec3::new(5.0, 3.8, 7.0)
            } else if matches!(
                scene,
                PreviewScene::Avatars
                    | PreviewScene::Characters(..)
                    | PreviewScene::CharacterStyles
            ) {
                Vec3::new(5.5, 3.1, 7.0)
            } else {
                Vec3::new(4.0, 2.6, 5.0)
            };
            if matches!(
                scene,
                PreviewScene::Characters(..) | PreviewScene::CharacterStyles
            ) {
                (target + Vec3::new(0.0, 1.8, 5.0), target + Vec3::Y * 0.9)
            } else {
                (target + offset, target)
            }
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
        fov_y_radians: if matches!(
            scene,
            PreviewScene::Characters(..) | PreviewScene::CharacterStyles
        ) {
            45f32.to_radians()
        } else {
            70f32.to_radians()
        },
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
    if matches!(
        scene,
        PreviewScene::Creature(..)
            | PreviewScene::Block(_)
            | PreviewScene::MossbunMotion(_)
            | PreviewScene::Characters(..)
            | PreviewScene::CharacterStyles
            | PreviewScene::Kilns
            | PreviewScene::Hoppers
            | PreviewScene::Chests
    ) {
        // A small display lawn makes feet and the player scale reference
        // inspectable instead of burying them in generated slopes/foliage.
        for x in target_xz.0 - 4..=target_xz.0 + 5 {
            for z in target_xz.1 - 2..=target_xz.1 + 5 {
                for y in target_height - 2..=target_height + 4 {
                    let block = if y > target_height {
                        world::AIR
                    } else if y == target_height {
                        world::GRASS
                    } else {
                        world::DIRT
                    };
                    set_preview_block(&mut chunks, x, y, z, block);
                }
            }
        }
    }
    if matches!(scene, PreviewScene::Fire) {
        // The cell has already burned to AIR; do not imply nearby flammable cells are lit.
        set_preview_block(
            &mut chunks,
            target_xz.0,
            target_height + 1,
            target_xz.1,
            world::AIR,
        );
        fire_renderer.set(
            &queue,
            &[
                render::VisualFire {
                    center: Vec3::new(
                        target_xz.0 as f32 + 0.5,
                        target_height as f32 + 1.5,
                        target_xz.1 as f32 + 0.5,
                    ),
                    age: 0.28,
                    style: render::fire::FireStyle::Flame,
                },
                render::VisualFire {
                    center: Vec3::new(
                        target_xz.0 as f32 + 1.35,
                        target_height as f32 + 1.8,
                        target_xz.1 as f32 + 0.5,
                    ),
                    age: 0.28,
                    style: render::fire::FireStyle::Spark([0.25, 0.85, 1.0], 0.28),
                },
            ],
        );
    }
    if let PreviewScene::Block(state) = scene {
        set_preview_block(
            &mut chunks,
            target_xz.0,
            target_height + 1,
            target_xz.1,
            state,
        );
    }
    if matches!(scene, PreviewScene::Chests) {
        for (dy, block) in [
            (1, crate::content::CHEST_STATE),
            (2, crate::content::HOPPER_STATE),
            (3, crate::content::CHEST_STATE),
        ] {
            set_preview_block(
                &mut chunks,
                target_xz.0,
                target_height + dy,
                target_xz.1,
                block,
            );
        }
    }
    if matches!(scene, PreviewScene::Hoppers) {
        for (dy, block) in [
            (1, crate::content::HOPPER_STATE),
            (
                2,
                crate::content::BlockStateId(crate::content::KILN_DEFAULT_STATE.0 + 1),
            ),
            (
                3,
                crate::content::BlockStateId(crate::content::KILN_DEFAULT_STATE.0 + 3),
            ),
            (4, crate::content::HOPPER_STATE),
        ] {
            set_preview_block(
                &mut chunks,
                target_xz.0,
                target_height + dy,
                target_xz.1,
                block,
            );
        }
    }
    if matches!(scene, PreviewScene::Kilns) {
        for (dx, lit) in [(-2, false), (1, true)] {
            for half in 0..=1 {
                let state = crate::content::BlockStateId(
                    crate::content::KILN_DEFAULT_STATE.0 + half * 2 + u32::from(lit),
                );
                set_preview_block(
                    &mut chunks,
                    target_xz.0 + dx,
                    target_height + 1 + half as i32,
                    target_xz.1,
                    state,
                );
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

    let drop_gpu_mesh = if matches!(scene, PreviewScene::Drops(_) | PreviewScene::Cave { .. }) {
        let custom = outputs[0]
            .path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("custom-"));
        let mut drop_catalog = crate::content::catalog().clone();
        let custom_item = if custom {
            use bloxgloom_host_api::content::{
                Components, DropAnimation, DropPolicy, DropSize, Item,
            };
            drop_catalog.public_item(&Item {
                key: "preview:animated-token".into(),
                name: "Animated token".into(),
                texture: "bloxgloom:stone".into(),
                swatch: [1.0; 4],
                placeable: None,
                sprite: false,
                drop_size: DropSize::Large,
                drop_animation: DropAnimation {
                    pop_duration: 1.0,
                    pop_height: 1.5,
                    hover_amplitude: 0.3,
                    hover_speed: 4.0,
                    spin_speed: 7.0,
                    pickup_duration: 0.8,
                    pickup_arc: 1.2,
                    pickup_turn: 8.0,
                },
                drop_policy: DropPolicy::default(),
                components: Components::None,
            })?;
            Some(drop_catalog.item_by_key("preview:animated-token").unwrap())
        } else {
            None
        };
        let phase = if let PreviewScene::Drops(phase) = scene {
            phase
        } else {
            DropPhase::Hover
        };
        let items: Vec<_> = [
            crate::items::ItemId::new(world::RED_FLOWER.get()),
            custom_item.unwrap_or_else(|| crate::items::ItemId::new(world::STONE.get())),
            crate::items::ItemId::new(
                if matches!(scene, PreviewScene::Cave { .. }) {
                    world::DIRT
                } else {
                    world::GLOWSTONE
                }
                .get(),
            ),
            SEEDS,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, item)| DroppedItem {
            id: index as u64 + 1,
            item,
            count: 1,
            position: if matches!(scene, PreviewScene::Cave { .. }) {
                [32.5, 10.0, 13.5 + index as f32 * 2.0]
            } else {
                [
                    target_xz.0 as f32 + index as f32 - 0.5,
                    target_height as f32 + 1.2 + index as f32 * 0.2,
                    target_xz.1 as f32 + 0.5,
                ]
            },
            age_ms: if matches!(phase, DropPhase::Pop) {
                0
            } else {
                2000
            },
        })
        .collect();
        let now = Instant::now();
        let mut animator = DropAnimator::new(now, std::sync::Arc::new(drop_catalog.clone()));
        animator.snapshot(items.clone(), now);
        let moment = match phase {
            DropPhase::Pop => now + std::time::Duration::from_millis(250),
            DropPhase::Hover => now,
            DropPhase::Pickup => {
                animator.picked_up(vec![items[1]], now);
                now + std::time::Duration::from_millis(180)
            }
        };
        let mut visuals = animator.visuals(moment, camera_position - Vec3::Y * 1.6);
        let mut fields = HashMap::new();
        for visual in &mut visuals {
            let p = visual.center.floor().as_ivec3();
            let (key, local) = world::world_to_chunk(p.x, p.y, p.z);
            let field = fields.entry(key).or_insert_with(|| {
                LightField::build_with_bounce(
                    key,
                    &chunks,
                    SEED,
                    matches!(scene, PreviewScene::Cave { bounced: true, .. }),
                )
            });
            visual.light = field.face(local, 1, 0);
        }
        let meshes = if custom {
            render::mesh_dropped_items_with_catalog(&visuals, &drop_catalog)
        } else {
            render::mesh_dropped_items(&visuals)
        };
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
    let mut character_visuals = None;
    if matches!(
        scene,
        PreviewScene::Avatars
            | PreviewScene::Characters(..)
            | PreviewScene::CharacterStyles
            | PreviewScene::Creature(..)
            | PreviewScene::MossbunMotion(_)
    ) {
        let mut visuals = [
            render::VisualAvatar {
                character_pose: [0.0; 3],
                character_recipe: None,
                animation: Default::default(),
                id: 1,
                model: if matches!(
                    scene,
                    PreviewScene::Creature(..) | PreviewScene::MossbunMotion(_)
                ) {
                    render::AvatarModel::Registered(if let PreviewScene::Creature(id, _) = scene {
                        id
                    } else {
                        crate::content::MOSSBUN_ENTITY_TYPE
                    })
                } else {
                    render::AvatarModel::Player
                },
                pose: [0.0, 0.0, 0.004, 0.0],
                airborne: false,
                position: Vec3::new(
                    target_xz.0 as f32 - 1.25,
                    target_height as f32 + 1.0,
                    target_xz.1 as f32 + 0.5,
                ),
                cosmetics: [0, 0, 0, 0],
                light_levels: [15, 0, 0, 0],
                bounce: [0; 4],
                glow_bounce: [0; 4],
                tint: [1.0; 3],
            },
            render::VisualAvatar {
                character_pose: [0.0; 3],
                character_recipe: None,
                animation: Default::default(),
                id: 2,
                model: if matches!(
                    scene,
                    PreviewScene::Creature(..) | PreviewScene::MossbunMotion(_)
                ) {
                    render::AvatarModel::Registered(if let PreviewScene::Creature(id, _) = scene {
                        id
                    } else {
                        crate::content::MOSSBUN_ENTITY_TYPE
                    })
                } else {
                    render::AvatarModel::Player
                },
                pose: if matches!(
                    scene,
                    PreviewScene::Creature(..) | PreviewScene::MossbunMotion(_)
                ) {
                    [std::f32::consts::FRAC_PI_2, 0.8, 0.018, 0.12]
                } else {
                    [0.0; 4]
                },
                airborne: false,
                position: Vec3::new(
                    target_xz.0 as f32 + 0.5,
                    target_height as f32 + 1.0,
                    target_xz.1 as f32 + 0.5,
                ),
                cosmetics: [2, 4, 2, 0],
                light_levels: [15, 0, 0, 0],
                bounce: [0; 4],
                glow_bounce: [0; 4],
                tint: if let PreviewScene::Creature(_, Some(tint)) = scene {
                    tint
                } else {
                    [1.0; 3]
                },
            },
            render::VisualAvatar {
                character_pose: [0.0; 3],
                character_recipe: None,
                animation: Default::default(),
                id: 3,
                model: render::AvatarModel::Player,
                pose: [0.0; 4],
                airborne: false,
                position: Vec3::new(
                    target_xz.0 as f32 + 2.25,
                    target_height as f32 + 1.0,
                    target_xz.1 as f32 + 0.5,
                ),
                cosmetics: [4, 1, 4, 0],
                light_levels: [15, 0, 0, 0],
                bounce: [0; 4],
                glow_bounce: [0; 4],
                tint: [1.0; 3],
            },
        ];
        if let PreviewScene::MossbunMotion(frame) = scene {
            actors::animate(&mut visuals, frame);
        }
        if let PreviewScene::Characters(clip, time, hair) = scene {
            avatar_renderer.preview_character_clip(clip, time);
            for visual in &mut visuals {
                visual.character_recipe = Some(crate::appearance::CharacterRecipe {
                    hair,
                    ..Default::default()
                });
            }
            // Front, three-quarter and back use the same production skinning path.
            visuals[0].pose[0] = 0.0;
            visuals[1].pose[0] = -0.7;
            visuals[2].pose[0] = std::f32::consts::PI;
            character_visuals = Some(visuals);
        }
        if matches!(scene, PreviewScene::CharacterStyles) {
            avatar_renderer.preview_character_clip("idle", 0.35);
            for (visual, recipe) in visuals.iter_mut().zip([
                crate::appearance::CharacterRecipe::default(),
                crate::appearance::CharacterRecipe {
                    hair: 2,
                    eyes: 5,
                    mouth: 5,
                    iris: Some([36, 220, 95]),
                },
                crate::appearance::CharacterRecipe {
                    hair: 0,
                    eyes: 2,
                    mouth: 2,
                    iris: Some([235, 80, 155]),
                },
            ]) {
                visual.character_recipe = Some(recipe);
                visual.pose[0] = 0.0;
            }
        }
        avatar_renderer.set(&queue, &visuals);
    }

    for (frame, output) in outputs.into_iter().enumerate() {
        if let (PreviewScene::Characters(clip, time, _), Some(visuals)) =
            (scene, &character_visuals)
        {
            avatar_renderer.preview_character_clip(clip, time + frame as f32 / 30.0);
            avatar_renderer.set(&queue, visuals);
        }
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
                atmosphere,
            )),
        );
        let matrix = render::view_projection(camera, output.width, output.height);
        queue.write_buffer(
            &camera_buffer,
            0,
            bytemuck::cast_slice(&atmosphere.camera_data(matrix)),
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
        let mut ui_frame = preview_frame(
            output.screen,
            has_target.then_some([target_xz.0, target_height, target_xz.1]),
            output.scale,
        );
        if output.screen == UiScreen::Admin
            && output
                .path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("commands-bindings"))
        {
            ui_frame.admin_input = "\u{1}\nInventory  :  E\nKiln input  :  R\nKiln fuel  :  F\nDrop  :  Q\ndemo:wave  :  T\nother:absent  :  Y (NOT IN SESSION)";
            ui_frame.hovered = Some(UiControl::AdminBindingRow(4));
        }
        if matches!(scene, PreviewScene::Hoppers | PreviewScene::Chests) && ui_frame.kiln.is_some()
        {
            let entity = if matches!(scene, PreviewScene::Hoppers) {
                crate::content::HOPPER_ENTITY_TYPE
            } else {
                crate::content::CHEST_ENTITY_TYPE
            };
            let screen = crate::content::catalog()
                .inventory_screen(entity)
                .unwrap()
                .clone();
            ui_frame.kiln = Some(crate::protocol::workstation::WorkstationView {
                slots: (0..screen.slots)
                    .map(|i| {
                        (i % 3 == 0)
                            .then(|| Stack::new(crate::items::ItemId(crate::world::STONE.0), 128))
                    })
                    .collect(),
                status: vec![],
            });
            ui_frame.container_screen = Some(screen);
        }
        if let PreviewScene::Inventory(entity) = scene {
            let screen = crate::content::catalog()
                .inventory_screen(entity)
                .unwrap()
                .clone();
            ui_frame.kiln = Some(crate::protocol::workstation::WorkstationView {
                slots: (0..screen.slots)
                    .map(|i| {
                        (i % 3 == 0)
                            .then(|| Stack::new(crate::items::ItemId(crate::world::STONE.0), 128))
                    })
                    .collect(),
                status: screen.status.iter().map(|s| s.maximum / 2).collect(),
            });
            ui_frame.container_screen = Some(screen);
        }
        if output.screen == UiScreen::Package
            && let Some(session) = &mut package_ui
        {
            session.next_document(); // Sample has one document; reset before each image.
            session.resize(output.width, output.height, output.scale);
            session.click(-1.0, -1.0);
            session.tab(false);
            let name = output.path.file_name().unwrap().to_string_lossy();
            if name.starts_with("package-edited") {
                session.edit(false, Some(" garden"));
                session.wait_for_presentation()?;
            } else if name.starts_with("package-planted") {
                session.tab(false);
                session.activate();
                session.wait_for_presentation()?;
            }
            ui_frame.package_ui = Some(session);
        }
        if !matches!(scene, PreviewScene::SurfaceBare) {
            ui_renderer.prepare(&queue, output.width, output.height, &ui_frame);
        }
        let bytes_per_row = output.width * 4;
        let padded_bytes_per_row = bytes_per_row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("preview readback"),
            size: u64::from(padded_bytes_per_row) * u64::from(output.height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut post = render::post::PostProcess::new(&device, output.width, output.height, FORMAT);
        if let Some(resources) = &mut visual_resources {
            if let Some(effect) = &resources.effect {
                post.install_effect(&device, effect)?;
            }
            resources.apply_effect_updates(&mut post)?;
            if let Some(gpu) = &mut resources.material {
                gpu.update(&queue);
            }
        }
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
            if let Some(gpu) = visual_resources.as_ref().and_then(|r| r.material.as_ref()) {
                pass.set_bind_group(2, &gpu.group, &[]);
            }
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
            pass.set_bind_group(0, &camera_group, &[]);
            pass.set_bind_group(1, &texture_group, &[]);
            if let Some(gpu) = visual_resources.as_ref().and_then(|r| r.material.as_ref()) {
                pass.set_bind_group(2, &gpu.group, &[]);
            }
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
            if matches!(scene, PreviewScene::Fire) {
                fire_renderer.draw(&mut pass);
            }
        }
        post.encode(&device, &queue, &mut encoder, &color_view);
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
        if !matches!(scene, PreviewScene::SurfaceBare) {
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
    for (slot, key) in [(11, "bloxgloom:wood"), (12, "bloxgloom:stick")] {
        if let Some(item) = crate::content::catalog().items().find(|i| i.key == key) {
            slots[slot] = Some(Stack::new(item.id, 8));
        }
    }
    #[cfg(feature = "lifecycle-fixture")]
    if let Some(item) = crate::content::catalog()
        .items()
        .find(|i| i.key == bloxgloom_lifecycle_fixture::content::CHIP)
    {
        slots[14] = Stack::with_components(item.id, 3, 2, vec![1]);
    }
    slots
}

fn action_preview_panel() -> bloxgloom_host_api::actions::Panel {
    #[cfg(feature = "lifecycle-fixture")]
    {
        bloxgloom_lifecycle_fixture::actions::definition()
            .panel
            .unwrap()
    }
    #[cfg(not(feature = "lifecycle-fixture"))]
    {
        use bloxgloom_host_api::actions::*;
        Panel {
            title: "REGISTERED ACTIONS".into(),
            widgets: vec![
                Widget::Label("Actions are supplied by installed content.".into()),
                Widget::Label("The server validates every request.".into()),
                Widget::Button {
                    action: None,
                    label: "EXAMPLE CONTROL".into(),
                    tooltip: "UI controls never authorize gameplay changes.".into(),
                },
            ],
        }
    }
}

fn preview_frame(screen: UiScreen, target: Option<[i32; 3]>, scale: f32) -> UiFrame<'static> {
    UiFrame {
        character: None,
        package_ui: None,
        join_address: None,
        join_progress: None,
        screen,
        selected_slot: 1,
        inventory: sample_inventory(),
        inventory_source: (screen == UiScreen::Inventory).then_some(10),
        inventory_search: if screen == UiScreen::Inventory {
            "stone"
        } else {
            ""
        },
        action_panel: (screen == UiScreen::Actions).then(action_preview_panel),
        container_screen: (screen == UiScreen::Container).then(|| {
            crate::content::catalog()
                .inventory_screen(crate::content::KILN_ENTITY_TYPE)
                .unwrap()
                .clone()
        }),
        kiln: (screen == UiScreen::Container).then(|| {
            crate::protocol::workstation::WorkstationView {
                status: vec![25_600, 667],
                slots: vec![
                    Some(Stack::new(crate::items::STICK, 12)),
                    Some(Stack::new(crate::items::ItemId(crate::world::GRAVEL.0), 24)),
                    Some(Stack::new(crate::items::ItemId(crate::world::STONE.0), 8)),
                ],
            }
        }),
        kiln_source: None,
        admin_enabled: screen != UiScreen::Admin,
        admin_page: 0,
        admin_input: "give bloxgloom:stone 128",
        target,
        status: match screen {
            UiScreen::Joining => {
                Some("Server: 127.0.0.1:4000\n\npackage download and verification")
            }
            UiScreen::JoinFailed => Some(
                "Server: 127.0.0.1:4000\n\nJoin 127.0.0.1:4000 failed during package client startup: client startup uidemo@1.0.0:client_startup: deliberate join failure",
            ),
            UiScreen::Playing => Some("E OPENS INVENTORY  /  Q DROPS ITEM"),
            _ => None,
        },
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
            UiScreen::Joining | UiScreen::JoinFailed => None,
            UiScreen::Actions => Some(UiControl::Action(2)),
            UiScreen::Container => Some(UiControl::KilnSlot(1)),
            UiScreen::Playing | UiScreen::Package => None,
            UiScreen::Inventory => Some(UiControl::InventorySearch),
            UiScreen::Admin => Some(UiControl::AdminItem(0)),
            UiScreen::Pause => Some(UiControl::Resume),
            UiScreen::Settings => Some(UiControl::Increase(SettingId::FieldOfView)),
            UiScreen::Graphics => Some(UiControl::Increase(SettingId::Exposure)),
            UiScreen::Character => Some(UiControl::ApplyCharacter),
        },
    }
}

fn measure_ui_prepare(ui_renderer: &mut ui::UiRenderer, queue: &wgpu::Queue) {
    let frame = UiFrame {
        character: None,
        package_ui: None,
        join_address: None,
        join_progress: None,
        inventory_search: "",
        action_panel: None,
        container_screen: None,
        screen: UiScreen::Settings,
        selected_slot: 4,
        inventory: sample_inventory(),
        inventory_source: None,
        kiln: None,
        kiln_source: None,
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
