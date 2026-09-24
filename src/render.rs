//! Opaque voxel renderer. CPU meshing is independent of the window/GPU and can run on workers.
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use glam::{Mat4, Vec3, Vec4, camera::rh};
use wgpu::util::DeviceExt;
use winit::{dpi::PhysicalSize, window::Window};

use crate::ui::{UiFrame, UiRenderer};
use crate::world::{CHUNK_SIZE, Chunk, ChunkKey, DIRT, GRASS, STONE};

pub(crate) const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
pub(crate) const UPLOAD_BYTES_PER_FRAME: usize = 4 * 1024 * 1024;
pub(crate) const UPLOAD_MESHES_PER_FRAME: usize = 4;
pub(crate) const MAX_PENDING_MESHES: usize = 128;
const VERTEX_STRIDE: u64 = 9 * 4;
const TEXTURE_SIZE: u32 = 16;
const TEXTURE_LAYERS: u32 = 4;
const TEXTURE_MIPS: u32 = 5;
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
        let sky_pipeline = create_sky_pipeline(&device, format);
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
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("atmospheric sky shader"),
        source: wgpu::ShaderSource::Wgsl(SKY_SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("sky pipeline layout"),
        bind_group_layouts: &[],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
    })
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
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
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

const SKY_SHADER: &str = r#"
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
@fragment fn fs_main(input: SkyVertex) -> @location(0) vec4<f32> {
    let height = smoothstep(0.0, 1.0, input.uv.y);
    let horizon = vec3<f32>(0.60, 0.72, 0.81);
    let zenith = vec3<f32>(0.22, 0.46, 0.75);
    var color = mix(horizon, zenith, height);
    let aspect = fwidth(input.uv.y) / max(fwidth(input.uv.x), 0.00001);
    let sun_offset = input.uv - vec2<f32>(0.78, 0.78);
    let sun_distance = length(vec2<f32>(sun_offset.x * aspect, sun_offset.y));
    let glow = 1.0 - smoothstep(0.02, 0.24, sun_distance);
    let disc = 1.0 - smoothstep(0.027, 0.035, sun_distance);
    color = mix(color, vec3<f32>(1.0, 0.83, 0.57), glow * 0.23);
    color = mix(color, vec3<f32>(1.0, 0.91, 0.69), disc);
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
        out.vertices
            .extend_from_slice(&[du as f32, dv as f32, layer as f32]);
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
        _ => 3,
    }
}

fn material_tiles() -> Vec<u8> {
    let mut pixels =
        Vec::with_capacity((TEXTURE_SIZE * TEXTURE_SIZE * TEXTURE_LAYERS * 4) as usize);
    for layer in 0..TEXTURE_LAYERS {
        for y in 0..TEXTURE_SIZE {
            for x in 0..TEXTURE_SIZE {
                let noise = tile_noise(x, y, layer);
                let variation = (noise % 13) as i16 - 6;
                let color: [i16; 3] = match layer {
                    0 => {
                        // Speckled grass with occasional warm dry blades.
                        if noise.is_multiple_of(17) {
                            [166, 176, 79]
                        } else if noise.is_multiple_of(7) {
                            [51, 119, 47]
                        } else {
                            [98 + variation, 160 + variation, 69 + variation / 2]
                        }
                    }
                    1 if y < 3 || (y == 3 && !noise.is_multiple_of(4)) => {
                        [90 + variation, 150 + variation, 63 + variation / 2]
                    }
                    1 | 2 => {
                        if noise.is_multiple_of(19) {
                            [158, 119, 78]
                        } else {
                            [132 + variation, 94 + variation, 62 + variation / 2]
                        }
                    }
                    _ => {
                        let crack = (x + y * 3 + layer) % 31 == 0 && noise.is_multiple_of(3);
                        if crack {
                            [102, 111, 117]
                        } else {
                            [144 + variation, 153 + variation, 158 + variation]
                        }
                    }
                };
                pixels.extend(color.map(|channel| channel.clamp(0, 255) as u8));
                pixels.push(255);
            }
        }
    }
    pixels
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

fn tile_noise(x: u32, y: u32, layer: u32) -> u32 {
    let mut value =
        x.wrapping_mul(0x9e37_79b9) ^ y.wrapping_mul(0x85eb_ca6b) ^ layer.wrapping_mul(0xc2b2_ae35);
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846c_a68b);
    value ^ (value >> 16)
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
    let sunlight = max(dot(input.normal, normalize(vec3<f32>(0.48, 1.0, 0.28))), 0.0);
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
    let sky = vec3<f32>(0.47, 0.64, 0.80);
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
}
