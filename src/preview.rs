//! Headless GPU renders of the world and each interface screen.
mod actors;
pub(crate) mod model;
mod third_person;
pub use third_person::{
    render_first_person_previews, render_gameplay_animation_previews, render_third_person_previews,
};
mod calibration;
mod outdoor;
pub use outdoor::showcase::render as render_showcase_previews;
pub use outdoor::showcase::render_motion as render_showcase_motion;
pub use outdoor::workshop::local_shadow::render as render_local_shadow_previews;
pub use outdoor::workshop::render_workshop_previews;
pub use outdoor::{
    install_outdoor_creatures, render_outdoor_depth, render_outdoor_motion, render_outdoor_previews,
};
mod sandbox;
mod sun_shadow;
pub use sandbox::{install_sandbox_materials, render_sandbox_previews};
mod daylight;
mod landscape;
pub use calibration::render_calibration_previews;
pub use landscape::render_landscape_previews;
mod water;
mod weather;
pub use daylight::render_daylight_previews;
pub(crate) use water::render_water_previews;
pub use weather::render_weather_previews;
mod block;
mod material;
pub use material::render_material_previews;
mod visuals;
pub use visuals::render_visual_previews;
mod egui_ui;
pub use block::render_block_preview;
pub use egui_ui::{render_egui_previews, render_package_egui_previews};
pub(crate) mod capture;
mod lod;
mod perf;
mod transport;
pub(crate) use lod::render_lod_previews;

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
            (UiScreen::Dead, "death"),
            (UiScreen::Dead, "death"),
            (UiScreen::Settings, "settings"),
            (UiScreen::Graphics, "graphics"),
            (UiScreen::Audio, "audio"),
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
        (UiScreen::Audio, "audio"),
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
        "run" => "run",
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
    pollster::block_on(run_perf_benchmark_async(steady_frames, radius, bounced, 0))
}

pub(crate) fn run_lod_benchmark(
    steady_frames: usize,
    radius: u8,
    horizon: u16,
    bounced: bool,
) -> Result<(), Box<dyn Error>> {
    pollster::block_on(run_perf_benchmark_async(
        steady_frames,
        radius,
        bounced,
        horizon,
    ))
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
    Material(crate::content::BlockStateId, render::MaterialPreviewMode),
    Inventory(crate::content::EntityTypeId),
    Chests,
    Kilns,
    Hoppers,
    Surface,
    SurfaceBare,
    Landscape(landscape::Shot),
    Water { x: i32, z: i32, level: i64 },
    Effect,
    Fire,
    Vegetation,
    Drops(DropPhase),
    Avatars,
    Characters(&'static str, f32, u8),
    CharacterStyles,
    ThirdPerson(third_person::Shot),
    Calibration(calibration::Scene),
    Sandbox(sandbox::Shot),
    Outdoor(outdoor::View),
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
    render_previews_weather(
        outputs,
        center_chunk,
        scene,
        package_root,
        world_time,
        render::weather::Presentation::default(),
    )
    .await
}

async fn render_previews_weather(
    outputs: Vec<PreviewOutput>,
    center_chunk: (i32, i32),
    scene: PreviewScene,
    package_root: Option<&Path>,
    world_time: u64,
    weather: render::weather::Presentation,
) -> Result<(), Box<dyn Error>> {
    let preview_config =
        std::env::var_os("BLOXGLOOM_PREVIEW_CONFIG").map(crate::config::Config::load);
    let bounced_lighting = match scene {
        PreviewScene::Cave { bounced, .. } => bounced,
        _ => preview_config
            .as_ref()
            .is_some_and(|config| config.bounced_gi),
    };
    let mut atmosphere = weather.atmosphere(render::daylight::Atmosphere::at(world_time));
    if let Some(config) = &preview_config {
        atmosphere.lighting = config.lighting;
    }
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
            required_features: transport::profile::features(&adapter)?,
            required_limits: render::material_device_limits(
                adapter.limits(),
                render::material_texture_layers(crate::content::catalog()) as usize,
            )?,
            ..Default::default()
        })
        .await?;
    let mut sky = render::SkyRenderer::new(&device, 1, 1, render::post::HDR_FORMAT);
    let (mut pipeline, mut cutout_pipeline, camera_buffer, _camera_group, texture_group) =
        if let PreviewScene::Material(_, mode) = scene {
            render::create_material_preview_pipeline(
                &device,
                &queue,
                render::post::HDR_FORMAT,
                crate::content::catalog(),
                mode,
            )?
        } else {
            render::create_voxel_pipeline(&device, &queue, render::post::HDR_FORMAT)
        };
    let mut water_renderer = render::water::WaterRenderer::new(&device, &camera_buffer);
    let mut fire_renderer = render::fire::FireRenderer::new(&device, &camera_buffer);
    let mut rain_renderer = render::weather::Renderer::new(&device, &camera_buffer);
    let mut avatar_renderer = render::AvatarRenderer::new(
        &device,
        &queue,
        render::post::HDR_FORMAT,
        &camera_buffer,
        crate::content::catalog(),
    );
    let mut sun_shadows =
        render::sun_shadow::SunShadows::new(&device, &camera_buffer, sun_shadow::quality()?);
    let mut local_shadows = render::local_shadow::LocalShadows::new(&device, &camera_buffer);
    if matches!(
        scene,
        PreviewScene::Outdoor(outdoor::View::Workshop(
            outdoor::workshop::View::LocalShadow { enabled: false }
        ))
    ) {
        local_shadows = render::local_shadow::LocalShadows::new_with_settings(
            &device,
            &camera_buffer,
            render::local_shadow::Settings {
                count: 0,
                ..Default::default()
            },
        );
    }
    sun_shadows.bind_local(&device, &camera_buffer, &local_shadows);
    let camera_group = sun_shadows.camera_group.clone();
    avatar_renderer.set_camera_group(camera_group.clone());
    water_renderer.set_camera_group(camera_group.clone());
    avatar_renderer.preview_animation_dt(0.0);
    avatar_renderer.enable_motion(
        render::post::temporal::supported(&device) && render::post::temporal_requested(),
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
    if !authored_preview
        && !matches!(
            scene,
            PreviewScene::Effect | PreviewScene::Fire | PreviewScene::Outdoor(_)
        )
    {
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
    let sun_pipelines = visual_resources
        .as_mut()
        .and_then(|resources| resources.sun_pipelines.take())
        .unwrap_or_else(|| render::create_sun_shadow_pipelines(&device, &pipeline, None));
    let camera_xz = (center_x + 40, center_z + 16);
    let target_xz = (center_x + 8, center_z - 16);
    let target_height = if matches!(scene, PreviewScene::Vegetation) {
        world::terrain_height(i64::from(target_xz.0), i64::from(target_xz.1), SEED) as i32
    } else {
        surface_height(target_xz.0, target_xz.1)
    };
    let (camera_position, target) = match scene {
        PreviewScene::Landscape(shot) => shot.camera(),
        PreviewScene::Material(..) => (Vec3::new(2.5, 35.0, 3.0), Vec3::new(0.5, 33.5, 0.5)),
        PreviewScene::Water { x, z, level } => {
            let target = Vec3::new(x as f32 + 0.5, level as f32 + 1.0, z as f32 + 0.5);
            (
                target
                    + if world::water_feature(i64::from(x), i64::from(z), SEED)
                        .is_some_and(|feature| feature.2 == "pond")
                    {
                        Vec3::new(9.0, 35.0, 11.0)
                    } else {
                        Vec3::new(27.0, 24.0, 32.0)
                    },
                target,
            )
        }
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
        | PreviewScene::Calibration(_)
        | PreviewScene::Sandbox(_)
        | PreviewScene::Outdoor(_)
        | PreviewScene::ThirdPerson(_)
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
    let mut camera_template = Camera {
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
    } else if matches!(scene, PreviewScene::Landscape(..)) {
        -1
    } else {
        0
    };
    let top_chunk = world::MAX_GENERATED_HEIGHT.div_euclid(world::CHUNK_SIZE as i32);
    let terrain_radius = if matches!(scene, PreviewScene::Landscape(..)) {
        6
    } else if matches!(scene, PreviewScene::Water { .. }) {
        4
    } else {
        2
    };
    for z in -terrain_radius..=terrain_radius {
        for x in -terrain_radius..=terrain_radius {
            for y in bottom_chunk..=top_chunk {
                let key = ChunkKey {
                    x: center_chunk.0 + x,
                    y,
                    z: center_chunk.1 + z,
                };
                let chunk = if matches!(scene, PreviewScene::Material(..)) {
                    world::Chunk::from_blocks(key, 0, vec![world::AIR; world::CHUNK_VOLUME])
                } else {
                    world::generate_chunk(key, SEED)
                };
                chunks.insert(key, Arc::new(chunk));
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
    let mut shadow_avatars = Vec::new();
    if let PreviewScene::Material(state, _) = scene {
        camera_template = material::prepare(state, &mut chunks);
    }
    if let PreviewScene::Calibration(calibration) = scene {
        camera_template = calibration::prepare(calibration, &mut chunks);
        avatar_renderer.preview_character_clip("idle", 0.35);
        shadow_avatars = calibration::avatars(&chunks);
        avatar_renderer.set(&queue, &shadow_avatars);
    }
    if let PreviewScene::Outdoor(view) = scene {
        camera_template = outdoor::prepare(view, &mut chunks);
        avatar_renderer.preview_character_clip("idle", 0.35);
        shadow_avatars = if matches!(
            view,
            outdoor::View::Workshop(outdoor::workshop::View::LocalShadow { .. })
        ) {
            outdoor::workshop::local_shadow::actors(&chunks, 0)
        } else if matches!(view, outdoor::View::Workshop(_)) {
            outdoor::workshop::avatars(&chunks)
        } else {
            outdoor::avatars(&chunks)
        };
        avatar_renderer.set(&queue, &shadow_avatars);
        println!("outdoor adapter: {:?}", adapter.get_info());
    }
    if let PreviewScene::Sandbox(shot) = scene {
        camera_template = sandbox::prepare(shot, &mut chunks);
        avatar_renderer.preview_character_clip(shot.clip, shot.seconds);
        shadow_avatars = sandbox::avatars(&chunks);
        avatar_renderer.set(&queue, &shadow_avatars);
        println!("sandbox adapter: {:?}", adapter.get_info());
    }
    if let PreviewScene::ThirdPerson(shot) = scene {
        camera_template = third_person::prepare(shot, &mut chunks, target_xz, target_height);
        if shot.perspective == render::camera::Perspective::FirstPerson {
            avatar_renderer.set_first_person(Some(render::FirstPersonView {
                id: 1,
                eye_height: shot.eye_height(),
                pitch: camera_template.pitch,
            }));
        }
        shadow_avatars.push(third_person::avatar(shot, target_xz, target_height));
        avatar_renderer.set(&queue, &shadow_avatars);
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
    let motion_sequence = matches!(
        scene,
        PreviewScene::Outdoor(
            outdoor::View::Motion | outdoor::View::Showcase(outdoor::showcase::View::Motion)
        )
    );
    let mut trace_chunks = Vec::new();
    let mut water_meshes = Vec::new();
    let mut gpu_meshes = Vec::new();
    let mut local_sources = Vec::new();
    let mut reference_rain_lights = HashMap::new();
    let cache_reference_rain = render::bsl_reference::enabled() && atmosphere.rain_strength > 0.0;
    for z in -terrain_radius..=terrain_radius {
        for x in -terrain_radius..=terrain_radius {
            for y in bottom_chunk..=top_chunk {
                let key = ChunkKey {
                    x: center_chunk.0 + x,
                    y,
                    z: center_chunk.1 + z,
                };
                let chunk = &chunks[&key];
                let light = LightField::build_with_bounce(key, &chunks, SEED, bounced_lighting);
                if cache_reference_rain {
                    reference_rain_lights.insert(key, render::weather::lightmap::cache(&light));
                }
                let mesh = render::mesh_chunk_lit_with_neighbors(
                    chunk,
                    &light,
                    0,
                    crate::content::catalog(),
                    &chunks,
                );
                local_sources.extend_from_slice(&mesh.local_sources);
                // Loaded empty chunks certify clear space for secondary rays,
                // even though they need no raster buffers.
                trace_chunks.push(mesh.trace.clone());
                if mesh.indices.is_empty()
                    && mesh.cutout_indices.is_empty()
                    && mesh.water_indices.is_empty()
                {
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
                            usage: wgpu::BufferUsages::VERTEX
                                | if motion_sequence {
                                    wgpu::BufferUsages::COPY_DST
                                } else {
                                    wgpu::BufferUsages::empty()
                                },
                        }),
                        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("preview indices"),
                            contents: bytemuck::cast_slice(indices),
                            usage: wgpu::BufferUsages::INDEX,
                        }),
                        indices.len() as u32,
                    ))
                };
                if let Some((vertices, indices, count)) =
                    upload(&mesh.water_vertices, &mesh.water_indices)
                {
                    water_meshes.push((key, vertices, indices, count));
                }
                gpu_meshes.push((
                    upload(&mesh.vertices, &mesh.indices),
                    upload(&mesh.cutout_vertices, &mesh.cutout_indices),
                ));
            }
        }
    }

    let mut drop_ray_targets = render::trace::dynamic::DynamicTargets::default();
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
            components: None,
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
                animator.picked_up(vec![items[1].clone()], now);
                now + std::time::Duration::from_millis(180)
            }
        };
        let mut visuals = animator.visuals(moment, camera_position - Vec3::Y * 1.6);
        let mut fields = HashMap::new();
        for visual in &mut visuals {
            let p = visual.center.floor().as_ivec3();
            let (key, local) = world::world_to_chunk(p.x, p.y, p.z);
            let field = fields.entry(key).or_insert_with(|| {
                LightField::build_with_bounce(key, &chunks, SEED, bounced_lighting)
            });
            visual.light = field.face(local, 1, 0);
        }
        let mut ray_drops = render::trace::dynamic::DropTargets::new(&drop_catalog);
        ray_drops.set(&visuals);
        drop_ray_targets = ray_drops.targets().clone();
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
                motion: None,
                model_pose: None,
                character_pose: [0.0; 4],
                character_look: [0.0; 2],
                character_crouch: 0.0,
                character_tool: None,
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
                glow_color: [0; 3],
                glow_direction: [0; 3],
                glow_bounce: [0; 4],
                tint: [1.0; 3],
            },
            render::VisualAvatar {
                motion: None,
                model_pose: None,
                character_pose: [0.0; 4],
                character_look: [0.0; 2],
                character_crouch: 0.0,
                character_tool: None,
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
                glow_color: [0; 3],
                glow_direction: [0; 3],
                glow_bounce: [0; 4],
                tint: if let PreviewScene::Creature(_, Some(tint)) = scene {
                    tint
                } else {
                    [1.0; 3]
                },
            },
            render::VisualAvatar {
                motion: None,
                model_pose: None,
                character_pose: [0.0; 4],
                character_look: [0.0; 2],
                character_crouch: 0.0,
                character_tool: None,
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
                glow_color: [0; 3],
                glow_direction: [0; 3],
                glow_bounce: [0; 4],
                tint: [1.0; 3],
            },
        ];
        if let PreviewScene::MossbunMotion(frame) = scene {
            actors::animate(&mut visuals, frame);
        }
        if let PreviewScene::Characters(clip, time, hair) = scene {
            avatar_renderer.preview_character_clip(clip, time);
            for (index, visual) in visuals.iter_mut().enumerate() {
                visual.character_recipe = Some(crate::appearance::CharacterRecipe {
                    hair,
                    body: u8::from(index == 1),
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
                    eyes: 0,
                    mouth: 0,
                    iris: Some([36, 220, 95]),
                    body: 1,
                    hair_color: [125, 85, 180],
                },
                crate::appearance::CharacterRecipe {
                    hair: 0,
                    eyes: 0,
                    mouth: 0,
                    iris: Some([235, 80, 155]),
                    ..Default::default()
                },
            ]) {
                visual.character_recipe = Some(recipe);
                visual.pose[0] = 0.0;
            }
        }
        avatar_renderer.set(&queue, &visuals);
        shadow_avatars.extend(visuals);
    }
    let shadow_patches = render::contact_shadow::patches(
        &shadow_avatars,
        camera_template.position,
        crate::content::catalog(),
        |x, y, z| {
            let (key, local) = world::world_to_chunk(x, y, z);
            chunks.get(&key).and_then(|chunk| chunk.block(local))
        },
    );
    if visual_resources
        .as_ref()
        .and_then(|r| r.material.as_ref())
        .is_none()
    {
        sun_shadows.set_contacts(&queue, &shadow_patches);
    }

    let motion_actor_base = shadow_avatars.clone();
    let mut motion_post: Option<render::post::PostProcess> = None;
    let mut local_maps_ready = false;
    let mut sky_sample = 0u32;
    let mut landscape_horizon = if matches!(scene, PreviewScene::Landscape(..)) {
        Some(landscape::horizon::build(
            &device,
            camera_template,
            &pipeline,
            &texture_group,
            &camera_group,
        )?)
    } else {
        None
    };
    if let Some(horizon) = &mut landscape_horizon {
        horizon.set_reference_water_inputs(&device, water_renderer.reference_inputs());
        horizon.set_optical_water_inputs(&device, water_renderer.optical_inputs());
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
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let color_view = color.create_view(&Default::default());
        let depth_view = depth.create_view(&Default::default());
        let mut camera = Camera {
            fov_y_radians: if matches!(
                scene,
                PreviewScene::Calibration(_) | PreviewScene::Sandbox(_) | PreviewScene::Outdoor(_)
            ) {
                camera_template.fov_y_radians
            } else {
                70.0f32.to_radians()
            },
            ..camera_template
        };
        if motion_sequence {
            shadow_avatars = outdoor::motion::actors(&motion_actor_base, frame);
            let (motion_camera, first_person) = outdoor::motion::camera(frame, &shadow_avatars);
            camera = motion_camera;
            if matches!(
                scene,
                PreviewScene::Outdoor(outdoor::View::Showcase(outdoor::showcase::View::Motion))
            ) && first_person.is_none()
            {
                camera = camera_template;
                camera.position.x += frame.saturating_sub(7).min(12) as f32 * 0.08;
            }
            avatar_renderer.set_first_person(first_person);
            avatar_renderer.preview_animation_dt(if frame < 8 { 0.0 } else { 1.0 / 30.0 });
            avatar_renderer.preview_character_clip("walk", outdoor::motion::seconds(frame));
            avatar_renderer.set(&queue, &shadow_avatars);
            atmosphere.wind_seconds = ((world_time % 128_000) as f32 / 1_000.0
                + outdoor::motion::seconds(frame))
            .rem_euclid(128.0);
            atmosphere.presentation_seconds = outdoor::motion::seconds(frame);
            let patches = render::contact_shadow::patches(
                &shadow_avatars,
                camera.position,
                crate::content::catalog(),
                |x, y, z| {
                    let (key, local) = world::world_to_chunk(x, y, z);
                    chunks.get(&key).and_then(|chunk| chunk.block(local))
                },
            );
            sun_shadows.set_contacts(&queue, &patches);
        }
        if matches!(
            scene,
            PreviewScene::Outdoor(outdoor::View::Workshop(
                outdoor::workshop::View::LocalShadow { .. }
            ))
        ) {
            shadow_avatars = outdoor::workshop::local_shadow::actors(&chunks, frame);
            avatar_renderer.preview_animation_dt(1.0 / 8.0);
            avatar_renderer.preview_character_clip("walk", frame as f32 / 8.0);
            avatar_renderer.set(&queue, &shadow_avatars);
            let patches = render::contact_shadow::patches(
                &shadow_avatars,
                camera.position,
                crate::content::catalog(),
                |x, y, z| {
                    let (key, local) = world::world_to_chunk(x, y, z);
                    chunks.get(&key).and_then(|chunk| chunk.block(local))
                },
            );
            sun_shadows.set_contacts(&queue, &patches);
        }
        if let Some((yaw, pitch)) = output.orientation {
            camera.yaw = yaw;
            camera.pitch = pitch;
        }
        queue.write_buffer(
            &sky.camera,
            0,
            bytemuck::cast_slice(&render::sky_camera_data_at_sample(
                camera,
                output.width,
                output.height,
                atmosphere,
                sky_sample,
            )),
        );
        let rain_mesh = weather.vertices(camera);
        if render::bsl_reference::enabled() {
            let glow = rain_mesh
                .chunks_exact(9)
                .map(|v| {
                    render::weather::lightmap::sample(
                        Vec3::new(v[0], v[1], v[2]),
                        &reference_rain_lights,
                    )
                })
                .collect::<Vec<_>>();
            rain_renderer.set_lightmaps(&glow);
        }
        rain_renderer.set_mesh(&queue, &rain_mesh);
        sun_shadows.update(&queue, camera, atmosphere);
        let matrix = render::view_projection(camera, output.width, output.height);
        queue.write_buffer(
            &camera_buffer,
            0,
            bytemuck::cast_slice(&atmosphere.camera_data(matrix, camera.position)),
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
        if let PreviewScene::ThirdPerson(shot) = scene {
            ui_frame.show_crosshair = shot.perspective != render::camera::Perspective::Front;
            ui_frame.status = None;
        }
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
        if !matches!(
            scene,
            PreviewScene::SurfaceBare
                | PreviewScene::Landscape(..)
                | PreviewScene::Material(..)
                | PreviewScene::Calibration(_)
                | PreviewScene::Sandbox(_)
                | PreviewScene::Outdoor(_)
        ) {
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
        let new_transport_scene = motion_post.is_none();
        let mut post = motion_post.take().unwrap_or_else(|| {
            render::post::PostProcess::new(&device, output.width, output.height, FORMAT)
        });
        if new_transport_scene {
            post.trace.set_water_reconstruction_adapter(&adapter);
        }
        let post_size = post.scene.texture().size();
        if post_size.width != output.width || post_size.height != output.height {
            post.resize(&device, output.width, output.height);
        }
        let eye_voxel = camera.position.floor().as_ivec3();
        let (eye_key, eye_local) = world::world_to_chunk(eye_voxel.x, eye_voxel.y, eye_voxel.z);
        let eye_in_water = chunks
            .get(&eye_key)
            .and_then(|chunk| chunk.block(eye_local))
            == Some(world::WATER);
        if new_transport_scene {
            let distant = if let Some(horizon) = &mut landscape_horizon {
                horizon.set_eye_in_water(eye_in_water);
                horizon.prepare(
                    &queue,
                    camera,
                    output.width,
                    output.height,
                    atmosphere,
                    chunks.keys().copied(),
                );
                horizon
                    .ray_targets()
                    .into_iter()
                    .map(|(_, _, chunk)| chunk)
                    .collect()
            } else {
                Vec::new()
            };
            post.trace
                .prepare_scene_with_lod(trace_chunks.iter().cloned(), distant);
        }
        let mut dynamic = avatar_renderer.ray_targets().clone();
        dynamic
            .instances
            .extend(drop_ray_targets.instances.iter().cloned());
        transport::prepare(
            &mut post,
            &device,
            &queue,
            &pipeline,
            &dynamic,
            eye_in_water,
        )?;
        atmosphere.scene_transport = post.trace.ready();
        sky.configure(atmosphere);
        queue.write_buffer(
            &sky.camera,
            0,
            bytemuck::cast_slice(&render::sky_camera_data_at_sample(
                camera,
                output.width,
                output.height,
                atmosphere,
                sky_sample,
            )),
        );

        if matches!(
            scene,
            PreviewScene::Calibration(_) | PreviewScene::Sandbox(_) | PreviewScene::Outdoor(_)
        ) {
            post.configure(&queue, true, 1.0, 0.12);
        }
        if let Some(resources) = &mut visual_resources {
            if let Some(effect) = &resources.effect {
                post.install_effect(&device, effect)?;
            }
            resources.apply_effect_updates(&mut post)?;
            if let Some(gpu) = &mut resources.material {
                gpu.update(&queue);
            }
        }
        if let Some(config) = &preview_config {
            post.configure(
                &queue,
                config.post_processing,
                config.exposure,
                if config.bloom_enabled {
                    config.bloom_strength
                } else {
                    0.0
                },
            );
        }
        let temporal = render::post::temporal_requested();
        post.enable_temporal(&device, temporal);
        // Optional fixed wave time makes controlled headless A/B captures
        // independent of compilation latency and GPU frame duration.
        let fixed_water_time = std::env::var("BLOXGLOOM_PREVIEW_WATER_TIME")
            .ok()
            .and_then(|value| value.parse::<f32>().ok())
            .filter(|value| value.is_finite() && *value >= 0.0);
        let default_samples = if post.temporal_enabled() && !motion_sequence {
            if post.trace.ready() || render::bsl_reference::enabled() {
                32
            } else {
                8
            }
        } else {
            1
        };
        let capture =
            transport::Capture::new(default_samples, post.trace.ready(), motion_sequence)?;
        let samples = capture.samples;
        transport::profile::prepare(&mut post, &device, samples)?;
        if !local_maps_ready {
            // Warm up admission with real submitted depth passes. Never mark a
            // face initialized without drawing it (important with updates<count).
            for _ in 0..=local_shadows.settings.count {
                local_shadows.update(&queue, camera.position, &local_sources, 0.25);
                let mut warmup = device.create_command_encoder(&Default::default());
                sun_shadow::draw_local(
                    &mut warmup,
                    &local_shadows,
                    &sun_pipelines,
                    &texture_group,
                    visual_resources.as_ref().and_then(|r| r.material.as_ref()),
                    &gpu_meshes,
                    drop_gpu_mesh.as_ref(),
                    &avatar_renderer,
                );
                queue.submit(Some(warmup.finish()));
            }
            local_maps_ready = true;
        }
        let mut sample = 0;
        let mut encoder = loop {
            let water_time = fixed_water_time.unwrap_or_else(render::water::time);
            post.trace.set_water_time(water_time);
            let (matrix, jitter) = post.prepare_temporal(&queue, camera);
            if let Some(horizon) = &mut landscape_horizon {
                horizon.set_jitter(jitter);
                horizon.set_eye_in_water(eye_in_water);
                horizon.set_reference_handlight(0, [0.0; 3]);
                horizon.prepare_at(
                    &queue,
                    camera,
                    output.width,
                    output.height,
                    atmosphere,
                    chunks.keys().copied(),
                    water_time,
                );
            }
            queue.write_buffer(
                &sky.camera,
                0,
                bytemuck::cast_slice(&render::sky_camera_data_at_sample_in_medium(
                    camera,
                    output.width,
                    output.height,
                    atmosphere,
                    sky_sample,
                    eye_in_water,
                )),
            );
            let mut camera_data = atmosphere.camera_data(matrix, camera.position);
            render::bsl_reference::handlight::configure(&mut camera_data, 0, [0.0; 3]);
            if render::bsl_reference::enabled() && eye_in_water {
                camera_data[31] = -1.0;
            }
            if landscape_horizon.is_some() {
                landscape::horizon::configure_camera(&mut camera_data);
            }
            if let Some(config) = &preview_config {
                camera_data[32..36].copy_from_slice(&config.parallax.uniform());
            }
            post.configure_reference_ao(camera_data, camera.fov_y_radians);
            queue.write_buffer(&camera_buffer, 0, bytemuck::cast_slice(&camera_data));
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("preview commands"),
            });
            sky.prepare(&device, &mut encoder, output.width, output.height);
            local_shadows.update(&queue, camera.position, &local_sources, 1.0 / 60.0);
            sun_shadow::draw_local(
                &mut encoder,
                &local_shadows,
                &sun_pipelines,
                &texture_group,
                visual_resources.as_ref().and_then(|r| r.material.as_ref()),
                &gpu_meshes,
                drop_gpu_mesh.as_ref(),
                &avatar_renderer,
            );
            sun_shadow::draw(
                &mut encoder,
                &sun_shadows,
                &sun_pipelines,
                &texture_group,
                visual_resources
                    .as_ref()
                    .and_then(|resources| resources.material.as_ref()),
                &gpu_meshes,
                drop_gpu_mesh.as_ref(),
                &avatar_renderer,
            );
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("preview world"),
                    color_attachments: &render::scene_ao::attachments(
                        &post.scene,
                        &post.ambient.indirect,
                        &post.reflections.normal,
                        &post.reflections.response,
                        render::SKY_COLOR,
                    ),
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
                pass.set_pipeline(&sky.pipeline);
                pass.set_bind_group(0, &sky.group, &[]);
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
                if let Some(horizon) = &landscape_horizon {
                    horizon.draw(&mut pass);
                }
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
            }
            post.reflections.configure(camera.position, atmosphere);
            post.resolve_ambient(&device, &queue, &mut encoder, &depth_view, matrix);
            water_renderer.prepare_frame_at(
                &queue,
                render::water::Frame {
                    atmosphere,
                    eye_in_water,
                    sample: sky_sample,
                    camera,
                    size: [output.width, output.height],
                    view_projection: matrix,
                },
                water_time,
            );
            let water_target =
                water_renderer.begin_frame(&device, &mut encoder, &post.scene, &depth_view);
            if let Some(horizon) = &mut landscape_horizon {
                horizon.set_reference_water_inputs(&device, water_renderer.reference_inputs());
                horizon.set_optical_water_inputs(&device, water_renderer.optical_inputs());
            }
            let water_depth = water_renderer
                .reference_front_depth()
                .cloned()
                .unwrap_or_else(|| depth_view.clone());
            if let Some(horizon) = &landscape_horizon {
                horizon.draw_water_pass(
                    &mut encoder,
                    &water_target,
                    &post.ambient.indirect,
                    &post.reflections.normal,
                    &post.reflections.response,
                    &water_depth,
                );
            }
            {
                let mut attachments = render::scene_ao::attachments(
                    &water_target,
                    &post.ambient.indirect,
                    &post.reflections.normal,
                    &post.reflections.response,
                    wgpu::Color::TRANSPARENT,
                );
                for attachment in attachments.iter_mut().flatten() {
                    attachment.ops.load = wgpu::LoadOp::Load;
                }
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("translucent particles after ambient occlusion"),
                    color_attachments: &attachments,
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &water_depth,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    ..Default::default()
                });
                water_meshes.sort_by(|a, b| {
                    render::water::distance(b.0, camera.position)
                        .total_cmp(&render::water::distance(a.0, camera.position))
                        .then_with(|| a.0.cmp(&b.0))
                });
                for (_, vertices, indices, count) in &water_meshes {
                    water_renderer.draw(&mut pass, vertices, indices, *count);
                }
            }

            water_renderer.finish_frame(&device, &mut encoder, &post.scene);

            post.configure_reference_water_depth(water_renderer.reference_front_depth());
            post.resolve_transport(
                &device,
                &queue,
                &mut encoder,
                &depth_view,
                matrix,
                camera.position,
                atmosphere,
                &pipeline,
                &texture_group,
            );
            if let Some(error) = post.trace.take_submission_error() {
                // Drop the unsubmitted continuation: no filter/composite,
                // final readback or PNG is allowed after a partial ray frame.
                return Err(error.into());
            }
            if (matches!(scene, PreviewScene::Fire) && !fire_renderer.is_empty())
                || !rain_renderer.is_empty()
            {
                let mut attachments = render::scene_ao::attachments(
                    &post.scene,
                    &post.ambient.indirect,
                    &post.reflections.normal,
                    &post.reflections.response,
                    wgpu::Color::TRANSPARENT,
                );
                for attachment in attachments.iter_mut().flatten() {
                    attachment.ops.load = wgpu::LoadOp::Load;
                }
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("preview particles over resolved reflections"),
                    color_attachments: &attachments,
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
                if matches!(scene, PreviewScene::Fire) {
                    fire_renderer.draw(&mut pass);
                }
                rain_renderer.draw(&mut pass);
            }
            rain_renderer.resolve(
                &device,
                &mut encoder,
                &post.scene,
                water_renderer
                    .reference_front_depth()
                    .unwrap_or(&depth_view),
            );
            post.resolve_atmosphere(
                &device,
                &queue,
                &mut encoder,
                &depth_view,
                matrix,
                camera.position,
                atmosphere,
                &sun_shadows,
            );
            post.draw_motion(&queue, &mut encoder, &depth_view, Some(&avatar_renderer));
            post.resolve_temporal(&device, &mut encoder, &depth_view);
            post.configure_reference_lens(
                render::view_projection(camera, output.width, output.height),
                camera.position,
                atmosphere,
                1.0 / 60.0,
                eye_in_water,
            );
            post.encode(&device, &queue, &mut encoder, &color_view);
            sample += 1;
            if sample == samples {
                break encoder;
            }
            // Separate submissions preserve each jitter/camera uniform update.
            let submitted = Instant::now();
            let submission = queue.submit(Some(encoder.finish()));
            sky_sample = sky_sample.wrapping_add(1);
            post.submitted();
            avatar_renderer.submitted();
            capture.checkpoint(&device, submission, sample, submitted.elapsed())?;
        };
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
        if !matches!(
            scene,
            PreviewScene::SurfaceBare
                | PreviewScene::Landscape(..)
                | PreviewScene::Material(..)
                | PreviewScene::Calibration(_)
                | PreviewScene::Sandbox(_)
                | PreviewScene::Outdoor(_)
        ) {
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
        let submitted = Instant::now();
        let submission = queue.submit(Some(encoder.finish()));
        post.submitted();
        sky_sample = sky_sample.wrapping_add(1);
        avatar_renderer.submitted();
        capture.checkpoint(&device, submission.clone(), samples, submitted.elapsed())?;
        let (sender, receiver) = mpsc::channel();
        readback.map_async(wgpu::MapMode::Read, .., move |result| {
            let _ = sender.send(result);
        });
        device.poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            // Static captures queue 32 path-traced samples before readback.
            // Bound the whole batch rather than treating it as one live frame.
            timeout: Some(std::time::Duration::from_secs(if post.trace.ready() {
                180
            } else {
                30
            })),
        })?;
        receiver.recv()??;
        let mapped = readback.get_mapped_range(..)?;
        let mut pixels = Vec::with_capacity((bytes_per_row * output.height) as usize);
        for row in mapped.chunks_exact(padded_bytes_per_row as usize) {
            pixels.extend_from_slice(&row[..bytes_per_row as usize]);
        }
        drop(mapped);
        readback.unmap();
        transport::profile::report(&post, &device, &queue)?;
        if post.trace.ready() && pixels.chunks_exact(4).all(|pixel| pixel[3] == 0) {
            return Err(
                "path-traced preview returned an empty readback; GPU submission may have failed"
                    .into(),
            );
        }
        write_png(&output.path, output.width, output.height, &pixels)?;
        if std::env::var("BLOXGLOOM_GI_DIAGNOSTICS").as_deref() == Ok("1")
            && let Some(diagnostics) = post.trace.water_history_diagnostics(&device, &queue)?
        {
            println!("{diagnostics}");
        }
        if std::env::var("BLOXGLOOM_GI_WATER_FILTER_DIAGNOSTICS").as_deref() == Ok("1") {
            let stem = output
                .path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy();
            let directory = output.path.with_file_name(format!("{stem}-water-filters"));
            let full_frame = !render::bsl_reference::enabled()
                && !post.temporal_enabled()
                && matches!(
                    scene,
                    PreviewScene::Landscape(landscape::Shot::Coast) | PreviewScene::Water { .. }
                )
                && rain_renderer.is_empty()
                && fire_renderer.is_empty();
            if let Some(diagnostics) = post.trace.write_water_filter_diagnostics(
                &device,
                &queue,
                [
                    &depth_view,
                    &post.reflections.normal,
                    &post.reflections.response,
                    &post.ambient.indirect,
                ],
                &directory,
                full_frame,
            )? {
                println!("{}", diagnostics.summary);
                if let Some(hdr) = diagnostics.hdr {
                    transport::water_comparison::write(
                        &device, &queue, &mut post, hdr, &directory,
                    )?;
                } else {
                    println!(
                        "production water comparisons skipped: require enhanced static coast/water, TAAoff and no posttrace particles"
                    );
                }
            }
        }
        if motion_sequence {
            motion_post = Some(post);
        }
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
        health: Some(bloxgloom_host_api::player_health::View::new(
            bloxgloom_host_api::player_health::State {
                current: if screen == UiScreen::Dead { 0 } else { 72 },
                ..Default::default()
            },
            7,
        )),
        chat: None,
        show_crosshair: true,
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
        flying: true,
        flying_pending: false,
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
            audio_preset: 0,
            sensitivity: 0.002,
            fov_degrees: 70.0,
            view_distance: 3,
            scale,
            fullscreen: false,
            bounced_gi: false,
            ..UiSettings::default()
        },
        hovered: match screen {
            UiScreen::Dead => Some(UiControl::Respawn),
            UiScreen::Joining | UiScreen::JoinFailed => None,
            UiScreen::Actions => Some(UiControl::Action(2)),
            UiScreen::Container => Some(UiControl::KilnSlot(1)),
            UiScreen::Playing | UiScreen::Package => None,
            UiScreen::Inventory => Some(UiControl::InventorySearch),
            UiScreen::Admin => Some(UiControl::AdminItem(0)),
            UiScreen::Pause => Some(UiControl::Resume),
            UiScreen::Settings => Some(UiControl::Increase(SettingId::FieldOfView)),
            UiScreen::Graphics => Some(UiControl::Increase(SettingId::Exposure)),
            UiScreen::Audio => Some(UiControl::AudioTest),
            UiScreen::Character => Some(UiControl::ApplyCharacter),
        },
    }
}

fn measure_ui_prepare(ui_renderer: &mut ui::UiRenderer, queue: &wgpu::Queue) {
    let frame = UiFrame {
        health: None,
        chat: None,
        show_crosshair: true,
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
        flying: true,
        flying_pending: false,
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
    for chunk_y in (0..=world::MAX_GENERATED_HEIGHT.div_euclid(world::CHUNK_SIZE as i32)).rev() {
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
