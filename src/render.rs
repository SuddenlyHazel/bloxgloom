//! Voxel renderer. CPU meshing is independent of the window/GPU and can run on workers.

mod avatars;
pub(crate) mod camera;
mod character_preview;
pub(crate) mod contact_shadow;
pub(crate) use character_preview::CharacterPreview;
pub(crate) mod custom;
mod drops;
pub(crate) mod effects;
pub(crate) mod fire;
mod fog;
pub(crate) mod game_ui;
mod hooks;
pub(crate) mod lod;
mod material;
pub(crate) mod water;
pub(crate) mod weather;
pub(crate) use material::resources::required_limits as material_device_limits;
pub(crate) use material::texture_layers_for as material_texture_layers;
mod mesh;
pub(crate) mod model_asset;
pub(crate) mod model_renderer;
pub(crate) mod parameters;
mod preparation;
pub(crate) use preparation::{Preparation, Ready as ReadyVisuals};
pub(crate) mod daylight;
pub(crate) mod local_shadow;
mod pipeline;
pub(crate) mod post;
pub(crate) mod scene_ao;
mod scene_contact;
mod sky;
pub(crate) mod sun_shadow;
mod target;
mod visibility;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use glam::Vec3;
use wgpu::util::DeviceExt;
use winit::{dpi::PhysicalSize, window::Window};

use crate::content::Catalog;
use crate::ui::UiFrame;
use crate::world::ChunkKey;

use mesh::{GpuMesh, GpuSubmesh};
use visibility::create_depth;

pub(crate) use avatars::{
    AvatarModel, AvatarRenderer, FirstPersonView, MAX_AVATARS, MovingVisual, VisualAvatar,
    character_tool_duration, prepare_character_asset,
};
pub(crate) use drops::VisualDrop;
pub(crate) use drops::mesh as mesh_dropped_items;
pub(crate) use drops::mesh_with_catalog as mesh_dropped_items_with_catalog;
pub(crate) use fire::VisualFire;
pub(crate) use game_ui::Intent as GameUiIntent;
#[cfg(test)]
pub use mesh::mesh_chunk;
#[cfg(test)]
pub use mesh::mesh_chunk_lit_with_catalog;
pub use mesh::{ChunkMesh, mesh_chunk_lit_with_neighbors};
pub(crate) use pipeline::{MaterialPreviewMode, create_material_preview_pipeline};
pub(crate) use pipeline::{create_custom_voxel_pipeline, create_voxel_pipeline};
pub(crate) use pipeline::{create_sun_shadow_pipelines, create_voxel_pipeline_with_catalog};
pub(crate) use sky::{create_sky_pipeline, sky_camera_data};
pub(crate) use target::{create_target_pipeline, target_outline_vertices};
pub(crate) use visibility::{chunk_visible, view_projection};

pub(crate) const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
pub(crate) const UPLOAD_BYTES_PER_FRAME: usize = 1024 * 1024;
pub(crate) const UPLOAD_MESHES_PER_FRAME: usize = 2;
pub(crate) const MAX_PENDING_MESHES: usize = 128;
pub(crate) const VERTEX_FLOATS: usize = 15;
pub(crate) const SUN_DIRECTION: Vec3 = Vec3::new(-0.55, 0.65, -0.52);
pub(crate) const SKY_COLOR: wgpu::Color = wgpu::Color {
    r: 0.43,
    g: 0.63,
    b: 0.82,
    a: 1.0,
};

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub fov_y_radians: f32,
}

impl Camera {
    pub fn direction(self) -> Vec3 {
        let pitch = self.pitch.clamp(-1.55, 1.55);
        Vec3::new(
            self.yaw.cos() * pitch.cos(),
            pitch.sin(),
            self.yaw.sin() * pitch.cos(),
        )
        .normalize_or_zero()
    }
}

#[derive(Debug)]
pub enum RendererError {
    Surface(wgpu::CreateSurfaceError),
    Adapter(wgpu::RequestAdapterError),
    Device(wgpu::RequestDeviceError),
    Materials(String),
}

impl std::fmt::Display for RendererError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Surface(error) => write!(formatter, "surface creation failed: {error}"),
            Self::Adapter(error) => write!(formatter, "GPU adapter unavailable: {error}"),
            Self::Device(error) => write!(formatter, "GPU device creation failed: {error}"),
            Self::Materials(error) => write!(formatter, "material GPU admission failed: {error}"),
        }
    }
}

impl std::error::Error for RendererError {}

#[derive(Default, Debug, Clone, Copy)]
pub struct RenderStats {
    pub visible_chunks: usize,
    pub uploaded_chunks: usize,
    pub pending_chunks: usize,
    pub drawn_triangles: usize,
    pub lod_tiles: usize,
    pub lod_bytes: usize,
}
pub struct Renderer {
    catalog: Arc<Catalog>,
    atmosphere: daylight::Atmosphere,
    package_lighting: crate::config::lighting::Lighting,
    user_lighting: crate::config::lighting::Lighting,
    user_local_shadows: local_shadow::Settings,
    package_local_shadows: Option<local_shadow::Settings>,
    applied_package_local_shadows: Option<local_shadow::Settings>,
    weather: weather::Presentation,
    rain: fire::FireRenderer,
    instance: wgpu::Instance,
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    depth: wgpu::TextureView,
    post: post::PostProcess,
    sky_pipeline: wgpu::RenderPipeline,
    sky_buffer: wgpu::Buffer,
    sky_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    cutout_pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_group: wgpu::BindGroup,
    texture_group: wgpu::BindGroup,
    material_gpu: Option<custom::Gpu>,
    target_pipeline: wgpu::RenderPipeline,
    target_camera_buffer: wgpu::Buffer,
    target_camera_group: wgpu::BindGroup,
    target_vertices: wgpu::Buffer,
    drop_vertices: wgpu::Buffer,
    drop_indices: wgpu::Buffer,
    drop_index_count: u32,
    drop_cutout_vertices: wgpu::Buffer,
    drop_cutout_indices: wgpu::Buffer,
    drop_cutout_index_count: u32,
    avatars: avatars::AvatarRenderer,
    sun_shadows: sun_shadow::SunShadows,
    local_shadows: local_shadow::LocalShadows,
    local_shadow_frame: std::time::Instant,
    sun_pipelines: (wgpu::RenderPipeline, wgpu::RenderPipeline),
    fire: fire::FireRenderer,
    water: water::WaterRenderer,
    game_ui: game_ui::GameUi,
    meshes: HashMap<ChunkKey, GpuMesh>,
    ready_near: std::collections::HashSet<ChunkKey>,
    lod: lod::Gpu,
    lod_horizon: u16,
    pending: HashMap<ChunkKey, ChunkMesh>,
    pending_order: VecDeque<ChunkKey>,
    pending_immediate: std::collections::HashSet<ChunkKey>,
    upload_burst: u8,
    size: PhysicalSize<u32>,
}

impl Renderer {
    pub(crate) fn enqueue_lod_mesh(&mut self, mesh: lod::Mesh) -> Result<(), lod::UploadError> {
        self.lod.enqueue(mesh)
    }
    pub(crate) fn discard_obsolete_lod(&mut self, key: crate::lod::TileKey, minimum: u64) {
        self.lod.discard_obsolete(key, minimum);
    }
    pub(crate) fn remove_lod_tile(&mut self, key: crate::lod::TileKey) {
        self.lod.remove(key);
    }
    pub(crate) fn clear_lod(&mut self) {
        self.lod.clear();
    }
    pub(crate) fn set_lod_horizon(&mut self, horizon: u16) {
        self.lod_horizon = horizon;
        self.lod.set_horizon(horizon);
    }

    pub(crate) fn set_visual_parameter(
        &mut self,
        update: &parameters::Update,
    ) -> Result<(), String> {
        if let Some(gpu) = &mut self.material_gpu
            && gpu.set(&update.resource, &update.name, &update.value)?
        {
            return Ok(());
        }
        if self.post.set_parameter(update)? {
            return Ok(());
        }
        Err(format!(
            "{}: visual parameter resource not installed",
            update.resource
        ))
    }
    pub(crate) fn install_package_ui(&mut self, resources: &crate::ui::authored::Resources) {
        self.game_ui.install_package_ui(resources);
    }

    pub(crate) fn clear_game_ui_intents(&mut self) {
        self.game_ui.clear_intents();
    }

    pub(crate) fn game_ui_event(&mut self, event: &winit::event::WindowEvent) {
        self.game_ui.on_window_event(&self.window, event);
    }

    pub(crate) fn game_ui_controller_pointer(
        &mut self,
        position: Option<[f32; 2]>,
        button: Option<(bool, bool)>,
        scroll: [f32; 2],
    ) {
        self.game_ui.controller_pointer(position, button, scroll);
    }

    pub(crate) fn take_game_ui_intents(&mut self) -> Vec<GameUiIntent> {
        self.game_ui.take_intents()
    }

    pub(crate) fn egui_wants_keyboard_input(&self) -> bool {
        self.game_ui.wants_keyboard_input()
    }
    pub async fn new_with_catalog(
        window: Arc<Window>,
        catalog: Arc<Catalog>,
    ) -> Result<Self, RendererError> {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(window.clone())
            .map_err(RendererError::Surface)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
            .map_err(RendererError::Adapter)?;
        let required_limits =
            material_device_limits(adapter.limits(), material_texture_layers(&catalog) as usize)
                .map_err(RendererError::Materials)?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_limits,
                ..Default::default()
            })
            .await
            .map_err(RendererError::Device)?;
        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .unwrap_or(capabilities.formats[0]);
        let present_mode = if capabilities
            .present_modes
            .contains(&wgpu::PresentMode::AutoVsync)
        {
            wgpu::PresentMode::AutoVsync
        } else {
            capabilities.present_modes[0]
        };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            alpha_mode: capabilities.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);
        let depth = create_depth(&device, config.width, config.height);
        let mut post = post::PostProcess::new(&device, config.width, config.height, format);
        post.enable_temporal(
            &device,
            std::env::var("BLOXGLOOM_TAA").is_ok_and(|value| value == "1"),
        );
        let (sky_pipeline, sky_buffer, sky_group) = create_sky_pipeline(&device, post::HDR_FORMAT);
        let (pipeline, cutout_pipeline, camera_buffer, _camera_group, texture_group) =
            create_voxel_pipeline_with_catalog(&device, &queue, post::HDR_FORMAT, &catalog)
                .map_err(RendererError::Materials)?;
        let lod = lod::Gpu::new(&device, post::HDR_FORMAT, &pipeline, &texture_group);
        let fire = fire::FireRenderer::new(&device, &camera_buffer);
        let water = water::WaterRenderer::new(&device, &camera_buffer);
        let rain =
            fire::FireRenderer::with_capacity(&device, &camera_buffer, weather::MAX_VERTEX_BYTES);
        let mut avatars = avatars::AvatarRenderer::new(
            &device,
            &queue,
            post::HDR_FORMAT,
            &camera_buffer,
            &catalog,
        );
        let mut sun_shadows = sun_shadow::SunShadows::new(
            &device,
            &camera_buffer,
            crate::config::SunShadowQuality::default(),
        );
        let local_shadows = local_shadow::LocalShadows::new(&device, &camera_buffer);
        sun_shadows.bind_local(&device, &camera_buffer, &local_shadows);
        avatars.set_camera_group(sun_shadows.camera_group.clone());
        avatars.enable_motion(post.temporal_enabled());
        let camera_group = sun_shadows.camera_group.clone();
        let sun_pipelines = create_sun_shadow_pipelines(&device, &pipeline, None);
        let (target_pipeline, target_camera_buffer, target_camera_group, target_vertices) =
            create_target_pipeline(&device, format);
        let game_ui = game_ui::GameUi::new(&window, &device, &queue, format, &catalog);
        let drop_vertices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dropped item vertices"),
            size: drops::MAX_VERTEX_BYTES,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let drop_indices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dropped item indices"),
            size: drops::MAX_INDEX_BYTES,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let drop_cutout_vertices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cutout dropped item vertices"),
            size: drops::MAX_CUTOUT_VERTEX_BYTES,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let drop_cutout_indices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cutout dropped item indices"),
            size: drops::MAX_CUTOUT_INDEX_BYTES,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self {
            atmosphere: daylight::Atmosphere::at(crate::daylight::INITIAL_MS),
            package_lighting: Default::default(),
            user_lighting: Default::default(),
            user_local_shadows: Default::default(),
            package_local_shadows: None,
            applied_package_local_shadows: None,
            weather: weather::Presentation::default(),
            rain,
            catalog,
            instance,
            window,
            surface,
            device,
            queue,
            config,
            depth,
            post,
            sky_pipeline,
            sky_buffer,
            sky_group,
            pipeline,
            cutout_pipeline,
            camera_buffer,
            camera_group,
            texture_group,
            material_gpu: None,
            target_pipeline,
            target_camera_buffer,
            target_camera_group,
            target_vertices,
            drop_vertices,
            drop_indices,
            drop_index_count: 0,
            drop_cutout_vertices,
            drop_cutout_indices,
            drop_cutout_index_count: 0,
            avatars,
            sun_shadows,
            local_shadows,
            local_shadow_frame: std::time::Instant::now(),
            sun_pipelines,
            fire,
            water,
            game_ui,
            meshes: HashMap::new(),
            ready_near: std::collections::HashSet::new(),
            lod,
            lod_horizon: 0,
            pending: HashMap::new(),
            pending_order: VecDeque::new(),
            pending_immediate: std::collections::HashSet::new(),
            upload_burst: 0,
            size,
        })
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        self.size = size;
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
        self.depth = create_depth(&self.device, size.width, size.height);
        self.post.resize(&self.device, size.width, size.height);
    }

    pub fn configure_post(&mut self, enabled: bool, exposure: f32, bloom_strength: f32) {
        self.post
            .configure(&self.queue, enabled, exposure, bloom_strength);
    }

    pub(crate) fn configure_sun_shadows(&mut self, quality: crate::config::SunShadowQuality) {
        if self
            .sun_shadows
            .configure(&self.device, &self.camera_buffer, quality)
        {
            self.sun_shadows
                .bind_local(&self.device, &self.camera_buffer, &self.local_shadows);
            self.camera_group = self.sun_shadows.camera_group.clone();
            self.avatars.set_camera_group(self.camera_group.clone());
        }
    }

    pub(crate) fn configure_lighting(&mut self, settings: crate::config::lighting::Lighting) {
        self.user_lighting = settings.sanitized();
        let user = self.user_lighting;
        let package = self.package_lighting;
        self.atmosphere.lighting = crate::config::lighting::Lighting {
            sun_intensity: user.sun_intensity * package.sun_intensity,
            ambient_intensity: user.ambient_intensity * package.ambient_intensity,
            environment_intensity: user.environment_intensity * package.environment_intensity,
            local_directionality: user.local_directionality * package.local_directionality,
        }
        .sanitized();
    }

    pub(crate) fn set_world_time(&mut self, time: u64) {
        let lighting = self.atmosphere.lighting;
        self.atmosphere = daylight::Atmosphere::at(time);
        self.atmosphere.lighting = lighting;
    }

    pub(crate) fn set_weather(
        &mut self,
        cloud: f32,
        rain: f32,
        wind: [f32; 2],
        exposure: f32,
        seconds: f32,
        flash: f32,
    ) {
        self.weather = weather::Presentation::new(cloud, rain, wind, exposure, seconds, flash);
    }

    pub(crate) fn set_weather_rain_cover(&mut self, origin: [i32; 2], heights: [f32; 256]) {
        self.weather.set_cover(origin, heights);
    }

    pub fn set_drops(&mut self, items: &[VisualDrop]) {
        let meshes = drops::mesh_with_catalog(items, &self.catalog);
        if !meshes.opaque_vertices.is_empty() {
            self.queue.write_buffer(
                &self.drop_vertices,
                0,
                bytemuck::cast_slice(&meshes.opaque_vertices),
            );
        }
        if !meshes.opaque_indices.is_empty() {
            self.queue.write_buffer(
                &self.drop_indices,
                0,
                bytemuck::cast_slice(&meshes.opaque_indices),
            );
        }
        if !meshes.cutout_vertices.is_empty() {
            self.queue.write_buffer(
                &self.drop_cutout_vertices,
                0,
                bytemuck::cast_slice(&meshes.cutout_vertices),
            );
        }
        if !meshes.cutout_indices.is_empty() {
            self.queue.write_buffer(
                &self.drop_cutout_indices,
                0,
                bytemuck::cast_slice(&meshes.cutout_indices),
            );
        }
        self.drop_index_count = meshes.opaque_indices.len() as u32;
        self.drop_cutout_index_count = meshes.cutout_indices.len() as u32;
    }

    pub(crate) fn set_fire(&mut self, fires: &[VisualFire]) {
        self.fire.set(&self.queue, fires);
    }

    pub(crate) fn set_first_person_character(&mut self, view: Option<avatars::FirstPersonView>) {
        self.avatars.set_first_person(view);
    }

    pub(crate) fn set_contact_shadows(&mut self, patches: &[contact_shadow::Patch]) {
        let patches = if self.material_gpu.is_none() {
            patches
        } else {
            &[]
        };
        self.sun_shadows.set_contacts(&self.queue, patches);
    }

    pub(crate) fn set_avatars(&mut self, avatars: &[VisualAvatar]) {
        self.avatars.set(&self.queue, avatars);
    }

    /// Whether geometry is already available to draw while a replacement builds.
    pub fn has_chunk_mesh(&self, key: ChunkKey) -> bool {
        self.meshes.contains_key(&key)
    }

    /// Floor decals must not use newly edited geometry before its GPU upload.
    pub(crate) fn has_current_chunk_mesh(&self, key: ChunkKey, revision: u64) -> bool {
        self.meshes
            .get(&key)
            .is_some_and(|mesh| mesh.lighting_revision == revision)
    }

    /// Replace a pending mesh of the same chunk; a full queue returns ownership for retry.
    #[allow(clippy::result_large_err)] // Returning ownership avoids copying mesh buffers on queue pressure.
    pub fn enqueue_mesh(&mut self, mesh: ChunkMesh, urgent: bool) -> Result<(), ChunkMesh> {
        if self
            .meshes
            .get(&mesh.key)
            .is_some_and(|old| old.lighting_revision > mesh.lighting_revision)
            || self
                .pending
                .get(&mesh.key)
                .is_some_and(|old| old.lighting_revision > mesh.lighting_revision)
        {
            return Ok(());
        }
        let limit = if urgent {
            MAX_PENDING_MESHES
        } else {
            MAX_PENDING_MESHES.saturating_sub(16)
        };
        if !self.pending.contains_key(&mesh.key) && self.pending.len() >= limit {
            return Err(mesh);
        }
        order_pending_mesh(
            &mut self.pending_order,
            mesh.key,
            self.pending.contains_key(&mesh.key),
            urgent,
        );
        if urgent {
            self.pending_immediate.insert(mesh.key);
        }
        self.pending.insert(mesh.key, mesh);
        Ok(())
    }

    /// An authoritative edit supersedes a queued mesh but not the last
    /// rendered one. Keep drawing until its replacement is ready.
    pub fn discard_pending_chunk(&mut self, key: ChunkKey) {
        self.pending_immediate.remove(&key);
        self.pending.remove(&key);
        self.pending_order.retain(|pending_key| *pending_key != key);
    }

    pub fn remove_chunk(&mut self, key: ChunkKey) {
        self.ready_near.remove(&key);
        self.pending_immediate.remove(&key);
        self.meshes.remove(&key);
        self.pending.remove(&key);
        self.pending_order.retain(|pending_key| *pending_key != key);
    }

    fn upload_pending(&mut self) -> usize {
        let mut bytes = 0;
        let mut count = 0;
        while count < UPLOAD_MESHES_PER_FRAME {
            let index = next_upload_index(
                &self.pending_order,
                &self.pending_immediate,
                self.upload_burst,
            );
            let Some(key) = self.pending_order.get(index).copied() else {
                break;
            };
            let Some(mesh) = self.pending.get(&key) else {
                self.pending_order.remove(index);
                self.pending_immediate.remove(&key);
                continue;
            };
            let mesh_bytes = mesh.byte_len();
            if count > 0 && bytes + mesh_bytes > UPLOAD_BYTES_PER_FRAME {
                break;
            }
            let mesh = self.pending.remove(&key).unwrap();
            crate::client::trace::event(format_args!(
                "upload {key:?} version={} rev={}",
                mesh.version, mesh.lighting_revision
            ));
            self.pending_order.remove(index);
            self.upload_burst = if self.pending_immediate.remove(&key) {
                (self.upload_burst + 1).min(3)
            } else {
                0
            };
            self.ready_near.insert(key);
            if mesh.indices.is_empty()
                && mesh.cutout_indices.is_empty()
                && mesh.water_indices.is_empty()
                && mesh.local_sources.is_empty()
            {
                self.meshes.remove(&key);
                continue;
            }
            let upload = |vertices: &[f32], indices: &[u32], label| {
                if indices.is_empty() {
                    return None;
                }
                Some(GpuSubmesh {
                    vertex: self
                        .device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some(label),
                            contents: bytemuck::cast_slice(vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        }),
                    index: self
                        .device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some(label),
                            contents: bytemuck::cast_slice(indices),
                            usage: wgpu::BufferUsages::INDEX,
                        }),
                    indices: indices.len() as u32,
                })
            };
            self.meshes.insert(
                key,
                GpuMesh {
                    lighting_revision: mesh.lighting_revision,
                    local_sources: mesh.local_sources,
                    opaque: upload(&mesh.vertices, &mesh.indices, "opaque chunk"),
                    cutout: upload(&mesh.cutout_vertices, &mesh.cutout_indices, "cutout chunk"),
                    water: upload(&mesh.water_vertices, &mesh.water_indices, "water chunk"),
                },
            );
            bytes += mesh_bytes;
            count += 1;
        }
        count
    }

    pub fn render(
        &mut self,
        camera: Camera,
        ui_frame: &UiFrame<'_>,
    ) -> Result<RenderStats, RendererError> {
        let uploaded_chunks = self.upload_pending();
        let atmosphere = self.weather.atmosphere(self.atmosphere);
        if uploaded_chunks < UPLOAD_MESHES_PER_FRAME {
            self.lod.upload(&self.device);
        }
        let (view_projection, jitter) = self.post.prepare_temporal(&self.queue, camera);
        self.lod.set_jitter(jitter);
        self.lod.prepare(
            &self.queue,
            camera,
            self.config.width,
            self.config.height,
            atmosphere,
            self.ready_near.iter().copied(),
        );
        self.sun_shadows.update(&self.queue, camera, atmosphere);
        self.rain
            .set_mesh(&self.queue, &self.weather.vertices(camera));
        let mut stats = RenderStats {
            uploaded_chunks,
            pending_chunks: self.pending.len(),
            lod_tiles: self.lod.selected_count(),
            lod_bytes: self.lod.resident_bytes(),
            ..Default::default()
        };
        if self.size.width == 0 || self.size.height == 0 {
            return Ok(stats);
        }
        self.queue.write_buffer(
            &self.sky_buffer,
            0,
            bytemuck::cast_slice(&sky_camera_data(
                camera,
                self.config.width,
                self.config.height,
                atmosphere,
            )),
        );
        let mut camera_data = atmosphere.camera_data(view_projection, camera.position);
        if self.lod_horizon > 0 {
            camera_data[28] = f32::from(self.lod_horizon) * 0.65;
            camera_data[29] = f32::from(self.lod_horizon);
        }
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&camera_data));
        self.queue.write_buffer(
            &self.camera_buffer,
            128,
            bytemuck::cast_slice(&ui_frame.settings.parallax.uniform()),
        );
        if let Some(gpu) = &mut self.material_gpu {
            gpu.update(&self.queue);
        }
        if let Some(target) = ui_frame.target {
            self.queue.write_buffer(
                &self.target_camera_buffer,
                0,
                bytemuck::cast_slice(
                    &visibility::view_projection(camera, self.config.width, self.config.height)
                        .to_cols_array(),
                ),
            );
            self.queue.write_buffer(
                &self.target_vertices,
                0,
                bytemuck::cast_slice(&target_outline_vertices(target)),
            );
        }
        let mut reconfigure_after_present = false;
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                reconfigure_after_present = true;
                frame
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                return Ok(stats);
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.surface = self
                    .instance
                    .create_surface(self.window.clone())
                    .map_err(RendererError::Surface)?;
                self.surface.configure(&self.device, &self.config);
                return Ok(stats);
            }
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Validation => return Ok(stats),
        };
        let view = frame.texture.create_view(&Default::default());
        // Admission/refresh state advances only for a frame that will encode
        // every scheduled face. Surface loss/occlusion must not publish an
        // initialized slot whose depth was never rendered.
        self.prepare_local_shadows(camera);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        self.draw_sun_shadows(&mut encoder);
        self.draw_local_shadows(&mut encoder);
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("opaque chunks"),
                color_attachments: &scene_ao::attachments(
                    &self.post.scene,
                    &self.post.ambient.indirect,
                    SKY_COLOR,
                ),
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&self.sky_pipeline);
            pass.set_bind_group(0, &self.sky_group, &[]);
            pass.draw(0..3, 0..1);
            stats.drawn_triangles += self.lod.draw(&mut pass);
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.camera_group, &[]);
            pass.set_bind_group(1, &self.texture_group, &[]);
            if let Some(gpu) = &self.material_gpu {
                pass.set_bind_group(2, &gpu.group, &[]);
            }
            for (key, mesh) in &self.meshes {
                if !visibility::chunk_visible_padded(
                    view_projection,
                    *key,
                    self.material_gpu.as_ref().map_or(0.0, custom::Gpu::padding),
                ) {
                    continue;
                }
                if let Some(opaque) = &mesh.opaque {
                    pass.set_vertex_buffer(0, opaque.vertex.slice(..));
                    pass.set_index_buffer(opaque.index.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..opaque.indices, 0, 0..1);
                    stats.drawn_triangles += opaque.indices as usize / 3;
                }
                stats.visible_chunks += 1;
            }
            if self.drop_index_count > 0 {
                pass.set_vertex_buffer(0, self.drop_vertices.slice(..));
                pass.set_index_buffer(self.drop_indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..self.drop_index_count, 0, 0..1);
                stats.drawn_triangles += self.drop_index_count as usize / 3;
            }
            stats.drawn_triangles += self.avatars.draw(&mut pass);
            pass.set_pipeline(&self.cutout_pipeline);
            pass.set_bind_group(0, &self.camera_group, &[]);
            pass.set_bind_group(1, &self.texture_group, &[]);
            if let Some(gpu) = &self.material_gpu {
                pass.set_bind_group(2, &gpu.group, &[]);
            }
            for (key, mesh) in &self.meshes {
                if !visibility::chunk_visible_padded(
                    view_projection,
                    *key,
                    self.material_gpu.as_ref().map_or(0.0, custom::Gpu::padding),
                ) {
                    continue;
                }
                if let Some(cutout) = &mesh.cutout {
                    pass.set_vertex_buffer(0, cutout.vertex.slice(..));
                    pass.set_index_buffer(cutout.index.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..cutout.indices, 0, 0..1);
                    stats.drawn_triangles += cutout.indices as usize / 3;
                }
            }
            if self.drop_cutout_index_count > 0 {
                pass.set_vertex_buffer(0, self.drop_cutout_vertices.slice(..));
                pass.set_index_buffer(
                    self.drop_cutout_indices.slice(..),
                    wgpu::IndexFormat::Uint32,
                );
                pass.draw_indexed(0..self.drop_cutout_index_count, 0, 0..1);
                stats.drawn_triangles += self.drop_cutout_index_count as usize / 3;
            }
        }
        self.post.resolve_ambient(
            &self.device,
            &self.queue,
            &mut encoder,
            &self.depth,
            view_projection,
        );
        {
            let mut attachments = scene_ao::attachments(
                &self.post.scene,
                &self.post.ambient.indirect,
                wgpu::Color::TRANSPARENT,
            );
            for attachment in attachments.iter_mut().flatten() {
                attachment.ops.load = wgpu::LoadOp::Load;
            }
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("translucent particles after ambient occlusion"),
                color_attachments: &attachments,
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            let mut water = self
                .meshes
                .iter()
                .filter_map(|(key, mesh)| mesh.water.as_ref().map(|mesh| (*key, mesh)))
                .filter(|(key, _)| visibility::chunk_visible_padded(view_projection, *key, 0.0))
                .collect::<Vec<_>>();
            water.sort_by(|(a, _), (b, _)| {
                water::distance(*b, camera.position)
                    .total_cmp(&water::distance(*a, camera.position))
                    .then_with(|| a.cmp(b))
            });
            stats.drawn_triangles += self.lod.draw_water(&mut pass);
            self.water.prepare(&self.queue);
            for (_, mesh) in water {
                stats.drawn_triangles +=
                    self.water
                        .draw(&mut pass, &mesh.vertex, &mesh.index, mesh.indices);
            }
            stats.drawn_triangles += self.fire.draw(&mut pass);
            stats.drawn_triangles += self.rain.draw(&mut pass);
        }

        self.post
            .draw_motion(&self.queue, &mut encoder, &self.depth, Some(&self.avatars));
        self.post
            .resolve_temporal(&self.device, &mut encoder, &self.depth);
        self.post
            .encode(&self.device, &self.queue, &mut encoder, &view);
        if ui_frame.target.is_some() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("target block outline"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&self.target_pipeline);
            pass.set_bind_group(0, &self.target_camera_group, &[]);
            pass.set_vertex_buffer(0, self.target_vertices.slice(..));
            pass.draw(0..24, 0..1);
        }
        self.game_ui.encode(
            game_ui::DrawTarget {
                window: &self.window,
                device: &self.device,
                queue: &self.queue,
                encoder: &mut encoder,
                view: &view,
                size: [self.config.width, self.config.height],
            },
            ui_frame,
            &self.catalog,
        );
        self.queue.submit(Some(encoder.finish()));
        self.post.submitted();
        self.avatars.submitted();
        self.queue.present(frame);
        if uploaded_chunks != 0 {
            crate::client::trace::event(format_args!("present uploaded_chunks={uploaded_chunks}"));
        }
        if reconfigure_after_present {
            self.surface.configure(&self.device, &self.config);
        }
        Ok(stats)
    }
}

fn next_upload_index(
    order: &VecDeque<ChunkKey>,
    immediate: &std::collections::HashSet<ChunkKey>,
    burst: u8,
) -> usize {
    if burst >= 3 {
        order
            .iter()
            .position(|key| !immediate.contains(key))
            .unwrap_or(0)
    } else {
        0
    }
}

fn order_pending_mesh(
    order: &mut VecDeque<ChunkKey>,
    key: ChunkKey,
    already_pending: bool,
    urgent: bool,
) {
    if urgent {
        order.retain(|pending_key| *pending_key != key);
        order.push_front(key);
    } else if !already_pending {
        order.push_back(key);
    }
}
