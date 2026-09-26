//! Voxel renderer. CPU meshing is independent of the window/GPU and can run on workers.

mod avatars;
mod drops;
mod material;
mod mesh;
mod pipeline;
mod shader;
mod sky;
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
use crate::ui::{UiFrame, UiRenderer};
use crate::world::ChunkKey;

use mesh::{GpuMesh, GpuSubmesh};
use visibility::create_depth;

pub(crate) use avatars::{AvatarRenderer, MAX_AVATARS, VisualAvatar};
pub(crate) use drops::VisualDrop;
pub(crate) use drops::mesh as mesh_dropped_items;
#[cfg(test)]
pub use mesh::mesh_chunk;
pub use mesh::{ChunkMesh, mesh_chunk_lit, mesh_chunk_lit_with_catalog};
pub(crate) use pipeline::create_voxel_pipeline;
pub(crate) use pipeline::create_voxel_pipeline_with_catalog;
pub(crate) use sky::{create_sky_pipeline, sky_camera_data};
pub(crate) use target::{create_target_pipeline, target_outline_vertices};
pub(crate) use visibility::{chunk_visible, view_projection};

pub(crate) const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
pub(crate) const UPLOAD_BYTES_PER_FRAME: usize = 1024 * 1024;
pub(crate) const UPLOAD_MESHES_PER_FRAME: usize = 2;
pub(crate) const MAX_PENDING_MESHES: usize = 128;
pub(crate) const VERTEX_FLOATS: usize = 12;
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
}

impl std::fmt::Display for RendererError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Surface(error) => write!(formatter, "surface creation failed: {error}"),
            Self::Adapter(error) => write!(formatter, "GPU adapter unavailable: {error}"),
            Self::Device(error) => write!(formatter, "GPU device creation failed: {error}"),
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
}
pub struct Renderer {
    catalog: Arc<Catalog>,
    instance: wgpu::Instance,
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    depth: wgpu::TextureView,
    sky_pipeline: wgpu::RenderPipeline,
    sky_buffer: wgpu::Buffer,
    sky_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    cutout_pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_group: wgpu::BindGroup,
    texture_group: wgpu::BindGroup,
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
    ui: UiRenderer,
    meshes: HashMap<ChunkKey, GpuMesh>,
    pending: HashMap<ChunkKey, ChunkMesh>,
    pending_order: VecDeque<ChunkKey>,
    size: PhysicalSize<u32>,
}

impl Renderer {
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
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
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
        let (sky_pipeline, sky_buffer, sky_group) = create_sky_pipeline(&device, format);
        let (pipeline, cutout_pipeline, camera_buffer, camera_group, texture_group) =
            create_voxel_pipeline_with_catalog(&device, &queue, format, &catalog);
        let avatars = avatars::AvatarRenderer::new(&device, format, &camera_buffer);
        let (target_pipeline, target_camera_buffer, target_camera_group, target_vertices) =
            create_target_pipeline(&device, format);
        let ui = UiRenderer::new_with_catalog(&device, &queue, format, Arc::clone(&catalog));
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
            catalog,
            instance,
            window,
            surface,
            device,
            queue,
            config,
            depth,
            sky_pipeline,
            sky_buffer,
            sky_group,
            pipeline,
            cutout_pipeline,
            camera_buffer,
            camera_group,
            texture_group,
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
            ui,
            meshes: HashMap::new(),
            pending: HashMap::new(),
            pending_order: VecDeque::new(),
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

    pub(crate) fn set_avatars(&mut self, avatars: &[VisualAvatar]) {
        self.avatars.set(&self.queue, avatars);
    }

    /// Whether geometry is already available to draw while a replacement builds.
    pub fn has_chunk_mesh(&self, key: ChunkKey) -> bool {
        self.meshes.contains_key(&key)
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
        if !self.pending.contains_key(&mesh.key) && self.pending.len() >= MAX_PENDING_MESHES {
            return Err(mesh);
        }
        order_pending_mesh(
            &mut self.pending_order,
            mesh.key,
            self.pending.contains_key(&mesh.key),
            urgent,
        );
        self.pending.insert(mesh.key, mesh);
        Ok(())
    }

    /// An authoritative edit supersedes a queued mesh but not the last
    /// rendered one. Keep drawing until its replacement is ready.
    pub fn discard_pending_chunk(&mut self, key: ChunkKey) {
        self.pending.remove(&key);
        self.pending_order.retain(|pending_key| *pending_key != key);
    }

    pub fn remove_chunk(&mut self, key: ChunkKey) {
        self.meshes.remove(&key);
        self.pending.remove(&key);
        self.pending_order.retain(|pending_key| *pending_key != key);
    }

    fn upload_pending(&mut self) -> usize {
        let mut bytes = 0;
        let mut count = 0;
        while count < UPLOAD_MESHES_PER_FRAME {
            let Some(key) = self.pending_order.front().copied() else {
                break;
            };
            let Some(mesh) = self.pending.get(&key) else {
                self.pending_order.pop_front();
                continue;
            };
            let mesh_bytes = mesh.byte_len();
            if count > 0 && bytes + mesh_bytes > UPLOAD_BYTES_PER_FRAME {
                break;
            }
            let mesh = self.pending.remove(&key).unwrap();
            self.pending_order.pop_front();
            if mesh.indices.is_empty() && mesh.cutout_indices.is_empty() {
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
                    opaque: upload(&mesh.vertices, &mesh.indices, "opaque chunk"),
                    cutout: upload(&mesh.cutout_vertices, &mesh.cutout_indices, "cutout chunk"),
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
        let mut stats = RenderStats {
            uploaded_chunks,
            pending_chunks: self.pending.len(),
            ..Default::default()
        };
        if self.size.width == 0 || self.size.height == 0 {
            return Ok(stats);
        }
        let view_projection = view_projection(camera, self.config.width, self.config.height);
        self.queue.write_buffer(
            &self.sky_buffer,
            0,
            bytemuck::cast_slice(&sky_camera_data(
                camera,
                self.config.width,
                self.config.height,
            )),
        );
        self.ui
            .prepare(&self.queue, self.config.width, self.config.height, ui_frame);
        self.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::cast_slice(&view_projection.to_cols_array()),
        );
        if let Some(target) = ui_frame.target {
            self.queue.write_buffer(
                &self.target_camera_buffer,
                0,
                bytemuck::cast_slice(&view_projection.to_cols_array()),
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
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("opaque chunks"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(SKY_COLOR),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
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
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.camera_group, &[]);
            pass.set_bind_group(1, &self.texture_group, &[]);
            for (key, mesh) in &self.meshes {
                if !chunk_visible(view_projection, *key) {
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
            for (key, mesh) in &self.meshes {
                if !chunk_visible(view_projection, *key) {
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
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("screen-space UI"),
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
            self.ui.encode(&mut pass);
        }
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(frame);
        if reconfigure_after_present {
            self.surface.configure(&self.device, &self.config);
        }
        Ok(stats)
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
