//! Previous **submitted** poses, keyed by stable entity identity rather than a
//! packed GPU slot. Color/shadow preparation never advances temporal history.
use glam::{Mat4, Vec3};
use std::collections::HashMap;

pub(crate) const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Frame {
    pub previous: [f32; 16],
    /// Width, height, current jitter in pixels.
    pub viewport: [f32; 4],
    /// Near, far, history valid, reserved.
    pub depth: [f32; 4],
}

#[derive(Clone)]
struct Pose {
    id: u64,
    identity: u64,
    position: Vec3,
    matrices: Vec<Mat4>,
}

#[derive(Default)]
pub(super) struct History {
    previous: HashMap<u64, Pose>,
    pending: Vec<Pose>,
}

impl History {
    pub fn clear_pending(&mut self) {
        self.pending.clear();
    }
    pub fn stage(&mut self, id: u64, identity: u64, position: Vec3, matrices: Vec<Mat4>) {
        self.pending.push(Pose {
            id,
            identity,
            position,
            matrices,
        });
    }
    fn palette(&self, valid: bool) -> Vec<[f32; 16]> {
        let mut matrices = Vec::new();
        for pose in &self.pending {
            let previous = self.previous.get(&pose.id).filter(|previous| {
                valid
                    && pose.identity == previous.identity
                    && pose.position.distance(previous.position) < 2.0
                    && pose.matrices.len() == previous.matrices.len()
                    && previous.matrices.iter().all(|m| m.is_finite())
                    && pose.matrices.iter().all(|m| m.is_finite())
            });
            // Zero homogeneous W marks a newly admitted, changed or teleported
            // actor explicitly reactive. It must never use camera-only motion.
            matrices.extend(
                (0..pose.matrices.len())
                    .map(|i| previous.map_or([0.0; 16], |p| p.matrices[i].to_cols_array())),
            );
        }
        matrices
    }
    pub fn submitted(&mut self) {
        // Replacing the set also invalidates disappear/reappear and cap churn.
        self.previous.clear();
        self.previous
            .extend(self.pending.iter().map(|p| (p.id, p.clone())));
    }
}

pub(super) fn fingerprint(value: impl std::hash::Hash) -> u64 {
    use std::hash::Hasher;
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hash);
    hash.finish()
}

pub(super) struct Palette {
    pub layout: wgpu::BindGroupLayout,
    pub group: wgpu::BindGroup,
    matrices: wgpu::Buffer,
    frame: wgpu::Buffer,
    pub history: History,
    pub enabled: bool,
}

impl Palette {
    pub fn new(device: &wgpu::Device, matrix_count: usize) -> Self {
        let buffer = |label, size, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let matrices = buffer(
            "previous submitted actor poses",
            (matrix_count.max(1) * 64) as u64,
            wgpu::BufferUsages::STORAGE,
        );
        let frame = buffer(
            "actor reprojection camera",
            std::mem::size_of::<Frame>() as u64,
            wgpu::BufferUsages::UNIFORM,
        );
        let entry = |binding, ty, visibility| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("actor motion palette"),
            entries: &[
                entry(
                    0,
                    wgpu::BufferBindingType::Storage { read_only: true },
                    wgpu::ShaderStages::VERTEX,
                ),
                entry(
                    1,
                    wgpu::BufferBindingType::Uniform,
                    wgpu::ShaderStages::VERTEX_FRAGMENT,
                ),
            ],
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("actor motion palette"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: matrices.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: frame.as_entire_binding(),
                },
            ],
        });
        Self {
            layout,
            group,
            matrices,
            frame,
            history: History::default(),
            enabled: false,
        }
    }
    pub fn enable(&mut self, enabled: bool) {
        if self.enabled != enabled {
            self.enabled = enabled;
            self.history = History::default();
        }
    }
    pub fn submitted(&mut self) {
        if self.enabled {
            self.history.submitted();
        }
    }
    pub fn prepare(&self, queue: &wgpu::Queue, frame: &Frame) {
        let matrices = self.history.palette(frame.depth[2] > 0.0);
        if !matrices.is_empty() {
            queue.write_buffer(&self.matrices, 0, bytemuck::cast_slice(&matrices));
        }
        queue.write_buffer(&self.frame, 0, bytemuck::bytes_of(frame));
    }
}

pub(super) fn shader(source: String, group: u32) -> String {
    source + &include_str!("motion.wgsl").replace("MOTION_GROUP", &group.to_string())
}

pub(super) fn pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    buffers: &[Option<wgpu::VertexBufferLayout<'_>>],
    cull_mode: Option<wgpu::Face>,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("depth-tested actor motion"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_motion"),
            compilation_options: Default::default(),
            buffers,
        },
        primitive: wgpu::PrimitiveState {
            cull_mode,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: super::DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Equal),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_motion"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

#[cfg(test)]
mod tests;
