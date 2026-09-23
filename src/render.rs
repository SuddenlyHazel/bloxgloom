//! Opaque voxel renderer. CPU meshing is independent of the window/GPU and can run on workers.
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use glam::{Mat4, Vec3, Vec4, camera::rh};
use wgpu::util::DeviceExt;
use winit::{dpi::PhysicalSize, window::Window};

use crate::world::{CHUNK_SIZE, Chunk, ChunkKey};

pub(crate) const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const UPLOAD_BYTES_PER_FRAME: usize = 4 * 1024 * 1024;
const UPLOAD_MESHES_PER_FRAME: usize = 4;
const MAX_PENDING_MESHES: usize = 128;
const VERTEX_STRIDE: u64 = 9 * 4;

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

/// Interleaved position, normal, and color. World-space coordinates avoid per-draw uniforms.
pub struct ChunkMesh {
    pub key: ChunkKey,
    pub version: u64,
    pub(crate) vertices: Vec<f32>,
    pub(crate) indices: Vec<u32>,
}

impl ChunkMesh {
    fn byte_len(&self) -> usize {
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
    pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_group: wgpu::BindGroup,
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
        let (pipeline, camera_buffer, camera_group) = create_voxel_pipeline(&device, format);
        Ok(Self {
            instance,
            window,
            surface,
            device,
            queue,
            config,
            depth,
            pipeline,
            camera_buffer,
            camera_group,
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

    pub fn render(&mut self, camera: Camera) -> Result<RenderStats, RendererError> {
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
            &self.camera_buffer,
            0,
            bytemuck::cast_slice(&view_projection.to_cols_array()),
        );
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
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.04,
                            g: 0.06,
                            b: 0.09,
                            a: 1.0,
                        }),
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
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.camera_group, &[]);
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
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(frame);
        if reconfigure_after_present {
            self.surface.configure(&self.device, &self.config);
        }
        Ok(stats)
    }
}

pub(crate) fn create_voxel_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> (wgpu::RenderPipeline, wgpu::Buffer, wgpu::BindGroup) {
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
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("voxel pipeline layout"),
        bind_group_layouts: &[Some(&camera_layout)],
        immediate_size: 0,
    });
    let attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3];
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
    (pipeline, camera_buffer, camera_group)
}

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

fn chunk_visible(matrix: Mat4, key: ChunkKey) -> bool {
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
    let n = CHUNK_SIZE as usize;
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
    let color = block_color(block);
    let corners = [(0, 0), (width, 0), (width, height), (0, height)];
    for (du, dv) in corners {
        let mut position = origin;
        position[axis] += (slice + usize::from(side > 0)) as f32;
        position[u] += (i + du) as f32;
        position[v] += (j + dv) as f32;
        out.vertices.extend_from_slice(&position);
        out.vertices.extend_from_slice(&normal);
        out.vertices.extend_from_slice(&color);
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

fn block_color(block: u8) -> [f32; 3] {
    match block {
        1 => [0.35, 0.62, 0.27],
        2 => [0.48, 0.36, 0.26],
        3 => [0.48, 0.50, 0.54],
        _ => {
            let hue = block as f32 * 0.618_034;
            [
                0.35 + 0.25 * hue.sin().abs(),
                0.36 + 0.2 * (hue + 2.0).sin().abs(),
                0.35 + 0.2 * (hue + 4.0).sin().abs(),
            ]
        }
    }
}

const SHADER: &str = r#"
struct Camera { view_projection: mat4x4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
};
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec3<f32>,
};
@vertex fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = camera.view_projection * vec4<f32>(input.position, 1.0);
    let sunlight = max(dot(input.normal, normalize(vec3<f32>(0.4, 1.0, 0.3))), 0.0);
    output.color = input.color * (0.48 + 0.52 * sunlight);
    return output;
}
@fragment fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(input.color, 1.0);
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
}
