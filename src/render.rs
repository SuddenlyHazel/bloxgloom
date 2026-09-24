//! Opaque voxel renderer. CPU meshing is independent of the window/GPU and can run on workers.
use std::collections::{HashMap, VecDeque};
use std::io::Cursor;
use std::sync::Arc;

use glam::{Mat4, Vec3, Vec4, camera::rh};
use wgpu::util::DeviceExt;
use winit::{dpi::PhysicalSize, window::Window};

use crate::ui::{UiFrame, UiRenderer};
use crate::world::{CHUNK_SIZE, Chunk, ChunkKey, DIRT, GRASS, GRAVEL, MOSS, SAND, SNOW, STONE};

pub(crate) const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
pub(crate) const UPLOAD_BYTES_PER_FRAME: usize = 4 * 1024 * 1024;
pub(crate) const UPLOAD_MESHES_PER_FRAME: usize = 4;
pub(crate) const MAX_PENDING_MESHES: usize = 128;
const VERTEX_STRIDE: u64 = 9 * 4;
const TEXTURE_SIZE: u32 = 128;
const TEXTURE_LAYERS: u32 = 8;
const TEXTURE_MIPS: u32 = 8;
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

/// Interleaved position, normal, tiled UV, and texture layer. World-space
/// coordinates avoid per-draw uniforms; one material bind group serves all chunks.
pub struct ChunkMesh {
    pub key: ChunkKey,
    pub version: u64,
    pub(crate) vertices: Vec<f32>,
    pub(crate) indices: Vec<u32>,
}

impl ChunkMesh {
    pub(crate) fn byte_len(&self) -> usize {
        self.vertices.len() * 4 + self.indices.len() * 4
    }

    #[cfg(test)]
    pub fn triangles(&self) -> usize {
        self.indices.len() / 3
    }
}

struct GpuMesh {
    version: u64,
    vertex: wgpu::Buffer,
    index: wgpu::Buffer,
    indices: u32,
}

pub struct Renderer {
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
    camera_buffer: wgpu::Buffer,
    camera_group: wgpu::BindGroup,
    texture_group: wgpu::BindGroup,
    target_pipeline: wgpu::RenderPipeline,
    target_camera_buffer: wgpu::Buffer,
    target_camera_group: wgpu::BindGroup,
    target_vertices: wgpu::Buffer,
    ui: UiRenderer,
    meshes: HashMap<ChunkKey, GpuMesh>,
    pending: HashMap<ChunkKey, ChunkMesh>,
    pending_order: VecDeque<ChunkKey>,
    size: PhysicalSize<u32>,
}

impl Renderer {
    pub async fn new(window: Arc<Window>) -> Result<Self, RendererError> {
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
        let (pipeline, camera_buffer, camera_group, texture_group) =
            create_voxel_pipeline(&device, &queue, format);
        let (target_pipeline, target_camera_buffer, target_camera_group, target_vertices) =
            create_target_pipeline(&device, format);
        let ui = UiRenderer::new(&device, &queue, format);
        Ok(Self {
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
            camera_buffer,
            camera_group,
            texture_group,
            target_pipeline,
            target_camera_buffer,
            target_camera_group,
            target_vertices,
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

    /// Replace a pending mesh of the same chunk; a full queue returns ownership for retry.
    pub fn enqueue_mesh(&mut self, mesh: ChunkMesh) -> Result<(), ChunkMesh> {
        if self
            .meshes
            .get(&mesh.key)
            .is_some_and(|old| old.version > mesh.version)
            || self
                .pending
                .get(&mesh.key)
                .is_some_and(|old| old.version > mesh.version)
        {
            return Ok(());
        }
        if !self.pending.contains_key(&mesh.key) {
            if self.pending.len() >= MAX_PENDING_MESHES {
                return Err(mesh);
            }
            self.pending_order.push_back(mesh.key);
        }
        self.pending.insert(mesh.key, mesh);
        Ok(())
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
            if mesh.indices.is_empty() {
                self.meshes.remove(&key);
                continue;
            }
            let vertex = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("chunk vertices"),
                    contents: bytemuck::cast_slice(&mesh.vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });
            let index = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("chunk indices"),
                    contents: bytemuck::cast_slice(&mesh.indices),
                    usage: wgpu::BufferUsages::INDEX,
                });
            self.meshes.insert(
                key,
                GpuMesh {
                    version: mesh.version,
                    vertex,
                    index,
                    indices: mesh.indices.len() as u32,
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
                pass.set_vertex_buffer(0, mesh.vertex.slice(..));
                pass.set_index_buffer(mesh.index.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.indices, 0, 0..1);
                stats.visible_chunks += 1;
                stats.drawn_triangles += mesh.indices as usize / 3;
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

pub(crate) fn create_sky_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> (wgpu::RenderPipeline, wgpu::Buffer, wgpu::BindGroup) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("atmospheric sky shader"),
        source: wgpu::ShaderSource::Wgsl(with_world_sun(SKY_SHADER).into()),
    });
    let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("sky camera basis"),
        size: 48,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("sky camera layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let camera_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("sky camera bind group"),
        layout: &camera_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: camera_buffer.as_entire_binding(),
        }],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("sky pipeline layout"),
        bind_group_layouts: &[Some(&camera_layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("atmospheric sky pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    (pipeline, camera_buffer, camera_group)
}

/// Camera basis packed as three aligned vec4 uniforms. The sun itself stays in
/// world space; looking away from it cannot leave a screen-fixed bright disc.
pub(crate) fn sky_camera_data(camera: Camera, width: u32, height: u32) -> [f32; 12] {
    let forward = camera.direction();
    let right = Vec3::new(-camera.yaw.sin(), 0.0, camera.yaw.cos());
    let up = right.cross(forward).normalize();
    let vertical = (camera.fov_y_radians * 0.5).tan();
    let horizontal = vertical * width as f32 / height.max(1) as f32;
    [
        forward.x, forward.y, forward.z, 0.0, right.x, right.y, right.z, horizontal, up.x, up.y,
        up.z, vertical,
    ]
}

pub(crate) fn create_voxel_pipeline(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
) -> (
    wgpu::RenderPipeline,
    wgpu::Buffer,
    wgpu::BindGroup,
    wgpu::BindGroup,
) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("opaque voxel shader"),
        source: wgpu::ShaderSource::Wgsl(with_world_sun(SHADER).into()),
    });
    let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("camera matrix"),
        size: 64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("camera layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let camera_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("camera bind group"),
        layout: &camera_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: camera_buffer.as_entire_binding(),
        }],
    });
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("voxel material tiles"),
        size: wgpu::Extent3d {
            width: TEXTURE_SIZE,
            height: TEXTURE_SIZE,
            depth_or_array_layers: TEXTURE_LAYERS,
        },
        mip_level_count: TEXTURE_MIPS,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (level, pixels) in material_mips().iter().enumerate() {
        let size = TEXTURE_SIZE >> level;
        let layer_bytes = (size * size * 4) as usize;
        for layer in 0..TEXTURE_LAYERS {
            let start = layer as usize * layer_bytes;
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &pixels[start..start + layer_bytes],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(size * 4),
                    rows_per_image: Some(size),
                },
                wgpu::Extent3d {
                    width: size,
                    height: size,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
    let texture_view = texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("nearest repeating voxel tiles"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        ..Default::default()
    });
    let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("voxel material layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let texture_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("voxel material bind group"),
        layout: &texture_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("voxel pipeline layout"),
        bind_group_layouts: &[Some(&camera_layout), Some(&texture_layout)],
        immediate_size: 0,
    });
    let attributes =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32];
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("opaque voxel pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: VERTEX_STRIDE,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &attributes,
            })],
        },
        primitive: wgpu::PrimitiveState {
            cull_mode: Some(wgpu::Face::Back),
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    (pipeline, camera_buffer, camera_group, texture_group)
}

fn with_world_sun(source: &str) -> String {
    source.replace(
        "WORLD_SUN_DIRECTION",
        &format!(
            "vec3<f32>({}, {}, {})",
            SUN_DIRECTION.x, SUN_DIRECTION.y, SUN_DIRECTION.z
        ),
    )
}

const SKY_SHADER: &str = r#"
struct SkyCamera {
    forward: vec4<f32>,
    right: vec4<f32>,
    up: vec4<f32>,
};
@group(0) @binding(0) var<uniform> sky_camera: SkyCamera;
struct SkyVertex {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};
@vertex fn vs_main(@builtin(vertex_index) index: u32) -> SkyVertex {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );
    var output: SkyVertex;
    let p = positions[index];
    output.position = vec4<f32>(p, 0.99999, 1.0);
    output.uv = p * 0.5 + vec2<f32>(0.5);
    return output;
}
fn sky_hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}
fn sky_noise(p: vec2<f32>) -> f32 {
    let cell = floor(p);
    let f = fract(p);
    let curve = f * f * (vec2<f32>(3.0) - 2.0 * f);
    let low = mix(sky_hash(cell), sky_hash(cell + vec2<f32>(1.0, 0.0)), curve.x);
    let high = mix(sky_hash(cell + vec2<f32>(0.0, 1.0)), sky_hash(cell + vec2<f32>(1.0, 1.0)), curve.x);
    return mix(low, high, curve.y);
}
@fragment fn fs_main(input: SkyVertex) -> @location(0) vec4<f32> {
    let ndc = input.uv * 2.0 - vec2<f32>(1.0);
    let ray = normalize(
        sky_camera.forward.xyz
        + sky_camera.right.xyz * ndc.x * sky_camera.right.w
        + sky_camera.up.xyz * ndc.y * sky_camera.up.w
    );
    let horizon = vec3<f32>(0.59, 0.72, 0.82);
    let zenith = vec3<f32>(0.20, 0.45, 0.75);
    var color = mix(horizon, zenith, smoothstep(-0.08, 0.86, ray.y));
    let sun_direction = normalize(WORLD_SUN_DIRECTION);
    let alignment = dot(ray, sun_direction);
    let haze = pow(max(alignment, 0.0), 10.0) * (1.0 - smoothstep(0.1, 0.75, ray.y));
    color = mix(color, vec3<f32>(0.95, 0.72, 0.54), haze * 0.22);
    let cloud_coordinates = ray.xz / max(ray.y, 0.10) * 8.0;
    let cloud_noise = sky_noise(cloud_coordinates * 0.45) * 0.68
        + sky_noise(cloud_coordinates * 0.90) * 0.32;
    let cloud = smoothstep(0.55, 0.70, cloud_noise)
        * smoothstep(0.10, 0.28, ray.y) * 0.54;
    color = mix(color, vec3<f32>(0.92, 0.94, 0.94), cloud);
    let glow = smoothstep(0.88, 0.997, alignment);
    let disc = smoothstep(0.9990, 0.99955, alignment);
    color = mix(color, vec3<f32>(1.0, 0.82, 0.55), glow * 0.28);
    color = mix(color, vec3<f32>(1.0, 0.92, 0.72), disc);
    return vec4<f32>(color, 1.0);
}
"#;

pub(crate) fn create_target_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> (
    wgpu::RenderPipeline,
    wgpu::Buffer,
    wgpu::BindGroup,
    wgpu::Buffer,
) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("target outline shader"),
        source: wgpu::ShaderSource::Wgsl(TARGET_SHADER.into()),
    });
    let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("target camera matrix"),
        size: 64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("target camera layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let camera_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("target camera bind group"),
        layout: &camera_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: camera_buffer.as_entire_binding(),
        }],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("target outline pipeline layout"),
        bind_group_layouts: &[Some(&camera_layout)],
        immediate_size: 0,
    });
    let attributes = wgpu::vertex_attr_array![0 => Float32x3];
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("target outline pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: 3 * std::mem::size_of::<f32>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &attributes,
            })],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::LineList,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    let vertices = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("target outline vertices"),
        size: 24 * 3 * std::mem::size_of::<f32>() as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    (pipeline, camera_buffer, camera_group, vertices)
}

pub(crate) fn target_outline_vertices(block: [i32; 3]) -> [[f32; 3]; 24] {
    let [x, y, z] = block.map(|v| v as f32);
    let e = 0.003;
    let min = [x - e, y - e, z - e];
    let max = [x + 1.0 + e, y + 1.0 + e, z + 1.0 + e];
    let corners = [
        [min[0], min[1], min[2]],
        [max[0], min[1], min[2]],
        [min[0], max[1], min[2]],
        [max[0], max[1], min[2]],
        [min[0], min[1], max[2]],
        [max[0], min[1], max[2]],
        [min[0], max[1], max[2]],
        [max[0], max[1], max[2]],
    ];
    const EDGES: [(usize, usize); 12] = [
        (0, 1),
        (0, 2),
        (0, 4),
        (1, 3),
        (1, 5),
        (2, 3),
        (2, 6),
        (3, 7),
        (4, 5),
        (4, 6),
        (5, 7),
        (6, 7),
    ];
    let mut output = [[0.0; 3]; 24];
    for (edge, (a, b)) in EDGES.into_iter().enumerate() {
        output[edge * 2] = corners[a];
        output[edge * 2 + 1] = corners[b];
    }
    output
}

const TARGET_SHADER: &str = r#"
struct Camera { view_projection: mat4x4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;

@vertex
fn vs_main(@location(0) position: vec3<f32>) -> @builtin(position) vec4<f32> {
    return camera.view_projection * vec4<f32>(position, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(0.98, 0.86, 0.39, 0.98);
}
"#;

pub(crate) fn view_projection(camera: Camera, width: u32, height: u32) -> Mat4 {
    let view = rh::view::look_to_mat4(camera.position, camera.direction(), Vec3::Y);
    let projection = rh::proj::directx::perspective(
        camera.fov_y_radians,
        width as f32 / height as f32,
        0.05,
        4096.0,
    );
    projection * view
}

fn create_depth(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

pub(crate) fn chunk_visible(matrix: Mat4, key: ChunkKey) -> bool {
    let n = CHUNK_SIZE as f32;
    let min = Vec3::new(key.x as f32 * n, key.y as f32 * n, key.z as f32 * n);
    let max = min + Vec3::splat(n);
    // Reject only when all corners lie outside one clip plane. This avoids any
    // dependence on matrix row/column extraction conventions.
    let corners = [
        Vec3::new(min.x, min.y, min.z),
        Vec3::new(max.x, min.y, min.z),
        Vec3::new(min.x, max.y, min.z),
        Vec3::new(max.x, max.y, min.z),
        Vec3::new(min.x, min.y, max.z),
        Vec3::new(max.x, min.y, max.z),
        Vec3::new(min.x, max.y, max.z),
        Vec3::new(max.x, max.y, max.z),
    ]
    .map(|p| matrix * p.extend(1.0));
    for plane in 0..6 {
        if corners.iter().all(|v| outside_clip(*v, plane)) {
            return false;
        }
    }
    true
}

fn outside_clip(v: Vec4, plane: usize) -> bool {
    match plane {
        0 => v.x < -v.w,
        1 => v.x > v.w,
        2 => v.y < -v.w,
        3 => v.y > v.w,
        4 => v.z < 0.0,
        _ => v.z > v.w,
    }
}

/// Greedy mesh opaque blocks, merging coplanar faces with the same block ID.
/// Uses the shared world model's x, z, y indexing; 0 is air.
pub fn mesh_chunk(chunk: &Chunk) -> ChunkMesh {
    let n = CHUNK_SIZE;
    let mut out = ChunkMesh {
        key: chunk.key,
        version: chunk.version,
        vertices: Vec::new(),
        indices: Vec::new(),
    };
    if chunk.blocks.len() != n * n * n {
        return out;
    }
    let origin = [
        chunk.key.x as f32 * n as f32,
        chunk.key.y as f32 * n as f32,
        chunk.key.z as f32 * n as f32,
    ];
    for axis in 0..3 {
        let u = (axis + 1) % 3;
        let v = (axis + 2) % 3;
        for side in [-1i32, 1] {
            let mut mask = vec![0u8; n * n];
            for slice in 0..n {
                mask.fill(0);
                for j in 0..n {
                    for i in 0..n {
                        let mut p = [0usize; 3];
                        p[axis] = slice;
                        p[u] = i;
                        p[v] = j;
                        let block = block_at(chunk, p, n);
                        if block == 0 {
                            continue;
                        }
                        let edge = if side > 0 { slice + 1 == n } else { slice == 0 };
                        let exposed = if edge {
                            true
                        } else {
                            let mut adjacent = p;
                            adjacent[axis] = (slice as i32 + side) as usize;
                            block_at(chunk, adjacent, n) == 0
                        };
                        if exposed {
                            mask[i + n * j] = block;
                        }
                    }
                }
                for j in 0..n {
                    let mut i = 0;
                    while i < n {
                        let block = mask[i + n * j];
                        if block == 0 {
                            i += 1;
                            continue;
                        }
                        let mut width = 1;
                        while i + width < n && mask[i + width + n * j] == block {
                            width += 1;
                        }
                        let mut height = 1;
                        'grow: while j + height < n {
                            for dx in 0..width {
                                if mask[i + dx + n * (j + height)] != block {
                                    break 'grow;
                                }
                            }
                            height += 1;
                        }
                        for dy in 0..height {
                            for dx in 0..width {
                                mask[i + dx + n * (j + dy)] = 0;
                            }
                        }
                        emit_quad(
                            &mut out, origin, axis, u, v, side, slice, i, j, width, height, block,
                        );
                        i += width;
                    }
                }
            }
        }
    }
    out
}

fn block_at(chunk: &Chunk, p: [usize; 3], n: usize) -> u8 {
    chunk.blocks[p[0] + n * (p[2] + n * p[1])]
}

#[allow(clippy::too_many_arguments)]
fn emit_quad(
    out: &mut ChunkMesh,
    origin: [f32; 3],
    axis: usize,
    u: usize,
    v: usize,
    side: i32,
    slice: usize,
    i: usize,
    j: usize,
    width: usize,
    height: usize,
    block: u8,
) {
    let base = (out.vertices.len() / 9) as u32;
    let mut normal = [0.0; 3];
    normal[axis] = side as f32;
    let layer = material_layer(block, axis, side);
    let corners = [(0, 0), (width, 0), (width, height), (0, height)];
    for (du, dv) in corners {
        let mut position = origin;
        position[axis] += (slice + usize::from(side > 0)) as f32;
        position[u] += (i + du) as f32;
        position[v] += (j + dv) as f32;
        out.vertices.extend_from_slice(&position);
        out.vertices.extend_from_slice(&normal);
        let (texture_u, texture_v) = if axis == 1 {
            (du as f32, dv as f32)
        } else if u == 1 {
            (dv as f32, (width - du) as f32)
        } else {
            (du as f32, (height - dv) as f32)
        };
        out.vertices
            .extend_from_slice(&[texture_u, texture_v, layer as f32]);
    }
    // (u, v, axis) is cyclic for every axis, so +axis is CCW.
    if side > 0 {
        out.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    } else {
        out.indices
            .extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
    }
}

fn material_layer(block: u8, axis: usize, side: i32) -> u8 {
    match block {
        GRASS if axis == 1 && side > 0 => 0,
        GRASS if axis == 1 => 2,
        GRASS => 1,
        DIRT => 2,
        STONE => 3,
        SAND => 4,
        SNOW => 5,
        MOSS => 6,
        GRAVEL => 7,
        _ => 3,
    }
}

fn material_tiles() -> Vec<u8> {
    const SOURCES: [&[u8]; 8] = [
        include_bytes!("../assets/textures/grass_top.png"),
        include_bytes!("../assets/textures/grass_side.png"),
        include_bytes!("../assets/textures/dirt.png"),
        include_bytes!("../assets/textures/stone.png"),
        include_bytes!("../assets/textures/sand.png"),
        include_bytes!("../assets/textures/snow.png"),
        include_bytes!("../assets/textures/moss.png"),
        include_bytes!("../assets/textures/gravel.png"),
    ];
    let mut pixels =
        Vec::with_capacity((TEXTURE_SIZE * TEXTURE_SIZE * TEXTURE_LAYERS * 4) as usize);
    for (layer, source) in SOURCES.into_iter().enumerate() {
        let layer_start = pixels.len();
        let mut decoder = png::Decoder::new(Cursor::new(source));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info().expect("embedded material PNG is valid");
        let mut decoded = vec![0; reader.output_buffer_size().expect("material PNG size fits")];
        let info = reader
            .next_frame(&mut decoded)
            .expect("embedded material PNG decodes");
        let channels = match info.color_type {
            png::ColorType::Rgb => 3,
            png::ColorType::Rgba => 4,
            other => panic!("embedded material PNG must be RGB or RGBA, got {other:?}"),
        };
        for y in 0..TEXTURE_SIZE {
            let source_y = ((y * 2 + 1) * info.height / (2 * TEXTURE_SIZE)) as usize;
            for x in 0..TEXTURE_SIZE {
                let source_x = ((x * 2 + 1) * info.width / (2 * TEXTURE_SIZE)) as usize;
                let index = (source_y * info.width as usize + source_x) * channels;
                pixels.extend_from_slice(&decoded[index..index + 3]);
                pixels.push(255);
            }
        }
        stitch_material_edges(&mut pixels[layer_start..], layer != 1);
    }
    pixels
}

fn stitch_material_edges(pixels: &mut [u8], stitch_vertical: bool) {
    let size = TEXTURE_SIZE as usize;
    const BAND: usize = 4;
    for y in 0..size {
        for offset in 0..BAND {
            let left = (y * size + offset) * 4;
            let right = (y * size + size - 1 - offset) * 4;
            blend_opposite_pixels(pixels, left, right, BAND - offset, BAND);
        }
    }
    if stitch_vertical {
        for x in 0..size {
            for offset in 0..BAND {
                let top = (offset * size + x) * 4;
                let bottom = ((size - 1 - offset) * size + x) * 4;
                blend_opposite_pixels(pixels, top, bottom, BAND - offset, BAND);
            }
        }
    }
}

fn blend_opposite_pixels(pixels: &mut [u8], a: usize, b: usize, weight: usize, total: usize) {
    for channel in 0..3 {
        let first = usize::from(pixels[a + channel]);
        let second = usize::from(pixels[b + channel]);
        let shared = (first + second) / 2;
        pixels[a + channel] = ((first * (total - weight) + shared * weight) / total) as u8;
        pixels[b + channel] = ((second * (total - weight) + shared * weight) / total) as u8;
    }
}

fn material_mips() -> Vec<Vec<u8>> {
    let mut levels = Vec::with_capacity(TEXTURE_MIPS as usize);
    levels.push(material_tiles());
    for level in 1..TEXTURE_MIPS {
        let previous_size = TEXTURE_SIZE >> (level - 1);
        let size = TEXTURE_SIZE >> level;
        let previous = levels.last().unwrap();
        let previous_layer_bytes = (previous_size * previous_size * 4) as usize;
        let mut pixels = Vec::with_capacity((size * size * TEXTURE_LAYERS * 4) as usize);
        for layer in 0..TEXTURE_LAYERS as usize {
            for y in 0..size {
                for x in 0..size {
                    for channel in 0..4usize {
                        let mut sum = 0u16;
                        for dy in 0..2 {
                            for dx in 0..2 {
                                let index = layer * previous_layer_bytes
                                    + ((((y * 2 + dy) * previous_size + x * 2 + dx) * 4) as usize)
                                    + channel;
                                sum += u16::from(previous[index]);
                            }
                        }
                        pixels.push((sum / 4) as u8);
                    }
                }
            }
        }
        levels.push(pixels);
    }
    levels
}

const SHADER: &str = r#"
struct Camera { view_projection: mat4x4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) layer: f32,
};
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) light: vec3<f32>,
    @location(2) @interpolate(flat) layer: i32,
    @location(3) distance: f32,
};
@group(1) @binding(0) var material: texture_2d_array<f32>;
@group(1) @binding(1) var material_sampler: sampler;
@vertex fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = camera.view_projection * vec4<f32>(input.position, 1.0);
    let sunlight = max(dot(input.normal, normalize(WORLD_SUN_DIRECTION)), 0.0);
    output.light = mix(
        vec3<f32>(0.55, 0.62, 0.72),
        vec3<f32>(1.05, 1.0, 0.91),
        sunlight
    );
    output.uv = input.uv;
    output.layer = i32(input.layer);
    output.distance = output.position.w;
    return output;
}
@fragment fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let albedo = textureSample(material, material_sampler, input.uv, input.layer).rgb;
    let fog = smoothstep(38.0, 135.0, input.distance);
    let sky = vec3<f32>(0.59, 0.72, 0.82);
    return vec4<f32>(mix(albedo * input.light, sky, fog), 1.0);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_chunk_merges_to_six_quads() {
        let chunk = Chunk {
            key: ChunkKey { x: 0, y: 0, z: 0 },
            version: 0,
            blocks: vec![1; 16 * 16 * 16],
        };
        assert_eq!(mesh_chunk(&chunk).triangles(), 12);
    }

    #[test]
    fn adjacent_blocks_have_no_internal_faces() {
        let mut chunk = Chunk {
            key: ChunkKey { x: 0, y: 0, z: 0 },
            version: 0,
            blocks: vec![0; 16 * 16 * 16],
        };
        chunk.blocks[0] = 1;
        chunk.blocks[1] = 1;
        assert_eq!(mesh_chunk(&chunk).triangles(), 12);
    }

    #[test]
    fn meshing_uses_shared_chunk_layout_and_world_origin() {
        let mut chunk = Chunk {
            key: ChunkKey { x: -1, y: 2, z: 3 },
            version: 7,
            blocks: vec![0; 16 * 16 * 16],
        };
        chunk.blocks[Chunk::index([2, 3, 4]).unwrap()] = 3;
        let mesh = mesh_chunk(&chunk);
        let positions = mesh.vertices.chunks_exact(9).map(|vertex| &vertex[..3]);
        let (min, max) = positions.fold(
            ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]),
            |(mut min, mut max), position| {
                for axis in 0..3 {
                    min[axis] = min[axis].min(position[axis]);
                    max[axis] = max[axis].max(position[axis]);
                }
                (min, max)
            },
        );
        assert_eq!(min, [-14.0, 35.0, 52.0]);
        assert_eq!(max, [-13.0, 36.0, 53.0]);
        assert_eq!(mesh.version, 7);
    }

    #[test]
    fn grass_uses_top_side_and_underlying_dirt_tiles() {
        assert_eq!(material_layer(GRASS, 1, 1), 0);
        assert_eq!(material_layer(GRASS, 0, 1), 1);
        assert_eq!(material_layer(GRASS, 2, -1), 1);
        assert_eq!(material_layer(GRASS, 1, -1), 2);
        assert_eq!(material_layer(DIRT, 1, 1), 2);
        assert_eq!(material_layer(STONE, 1, 1), 3);
        assert_eq!(material_layer(SAND, 1, 1), 4);
        assert_eq!(material_layer(SNOW, 1, 1), 5);
        assert_eq!(material_layer(MOSS, 1, 1), 6);
        assert_eq!(material_layer(GRAVEL, 1, 1), 7);
    }

    #[test]
    fn grass_side_is_upright_on_both_wall_axes() {
        let mut chunk = Chunk {
            key: ChunkKey { x: 0, y: 0, z: 0 },
            version: 0,
            blocks: vec![0; CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE],
        };
        chunk.blocks[Chunk::index([1, 1, 1]).unwrap()] = GRASS;
        let mesh = mesh_chunk(&chunk);
        for wall_axis in [0, 2] {
            let vertices = mesh
                .vertices
                .chunks_exact(9)
                .filter(|vertex| vertex[3 + wall_axis].abs() == 1.0 && vertex[8] == 1.0);
            let mut count = 0;
            for vertex in vertices {
                let expected_v = if vertex[1] == 2.0 { 0.0 } else { 1.0 };
                assert_eq!(vertex[7], expected_v, "grass cap must face world up");
                count += 1;
            }
            assert_eq!(count, 8);
        }
    }

    #[test]
    fn greedy_quads_repeat_material_once_per_voxel() {
        let chunk = Chunk {
            key: ChunkKey { x: 0, y: 0, z: 0 },
            version: 0,
            blocks: vec![STONE; CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE],
        };
        let mesh = mesh_chunk(&chunk);
        let vertices = mesh.vertices.chunks_exact(9).collect::<Vec<_>>();
        assert_eq!(vertices.len(), 24);
        assert!(vertices.iter().all(|vertex| vertex[8] == 3.0));
        assert!(vertices.iter().any(|vertex| vertex[6] == 16.0));
        assert!(vertices.iter().any(|vertex| vertex[7] == 16.0));
    }

    #[test]
    fn material_mips_are_complete_and_opaque() {
        let mips = material_mips();
        assert_eq!(mips.len(), TEXTURE_MIPS as usize);
        for (level, pixels) in mips.iter().enumerate() {
            let size = TEXTURE_SIZE >> level;
            assert_eq!(pixels.len(), (size * size * TEXTURE_LAYERS * 4) as usize);
            assert!(pixels.chunks_exact(4).all(|pixel| pixel[3] == 255));
        }
        assert_ne!(
            &mips[0][..3],
            &mips[0][(TEXTURE_SIZE * TEXTURE_SIZE * 3 * 4) as usize..][..3]
        );
    }

    #[test]
    fn material_edges_tile_without_seams() {
        let tiles = material_tiles();
        let size = TEXTURE_SIZE as usize;
        let layer_bytes = size * size * 4;
        for layer in 0..TEXTURE_LAYERS as usize {
            let pixels = &tiles[layer * layer_bytes..(layer + 1) * layer_bytes];
            for y in 0..size {
                let left = &pixels[y * size * 4..y * size * 4 + 3];
                let right = &pixels[(y * size + size - 1) * 4..][..3];
                assert_eq!(left, right, "horizontal seam in layer {layer}, row {y}");
            }
            if layer != 1 {
                for x in 0..size {
                    let top = &pixels[x * 4..x * 4 + 3];
                    let bottom = &pixels[((size - 1) * size + x) * 4..][..3];
                    assert_eq!(top, bottom, "vertical seam in layer {layer}, column {x}");
                }
            }
        }
    }

    #[test]
    fn sky_basis_tracks_camera_turns_in_world_space() {
        let sun = SUN_DIRECTION.normalize();
        let facing = Camera {
            position: Vec3::ZERO,
            yaw: sun.z.atan2(sun.x),
            pitch: sun.y.asin(),
            fov_y_radians: 70.0f32.to_radians(),
        };
        let facing_data = sky_camera_data(facing, 1280, 720);
        let facing_center = Vec3::new(facing_data[0], facing_data[1], facing_data[2]);
        assert!(facing_center.dot(sun) > 0.999);

        let away = Camera {
            yaw: (-sun.z).atan2(-sun.x),
            pitch: -sun.y.asin(),
            ..facing
        };
        let away_data = sky_camera_data(away, 1280, 720);
        let away_center = Vec3::new(away_data[0], away_data[1], away_data[2]);
        assert!(away_center.dot(sun) < -0.999);
        assert!((facing_data[7] - facing_data[11] * (1280.0 / 720.0)).abs() < 1e-6);
    }
}
