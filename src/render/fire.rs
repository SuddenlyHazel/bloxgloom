//! Short-lived translucent flame streaks at committed burn cells. No simulation state.
use super::{DEPTH_FORMAT, post};
use glam::Vec3;

pub(crate) const MAX_FIRES: usize = 128;
const VERTICES_PER_FIRE: usize = 15; // four flame triangles and one rising ember
const FLOATS: usize = 9; // position, uv, rgba
pub(crate) const MAX_BYTES: u64 = (MAX_FIRES * VERTICES_PER_FIRE * FLOATS * 4) as u64;

#[derive(Clone, Copy, Debug)]
pub(crate) struct VisualFire {
    pub center: Vec3,
    pub age: f32, // 0..1
    pub style: FireStyle,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum FireStyle {
    Flame,
    Spark([f32; 3], f32),
}

pub(crate) fn vertices(fires: &[VisualFire]) -> Vec<f32> {
    let mut out = Vec::with_capacity(fires.len().min(MAX_FIRES) * VERTICES_PER_FIRE * FLOATS);
    for fire in fires.iter().take(MAX_FIRES) {
        let fade = (1.0 - fire.age).clamp(0.0, 1.0);
        if let FireStyle::Spark(rgb, base_size) = fire.style {
            let rise = fire.age * 0.65;
            let center = fire.center + Vec3::Y * rise;
            for axis in [Vec3::X, Vec3::Z] {
                let size = base_size * fade.max(0.2);
                triangle(
                    &mut out,
                    [
                        center - axis * size,
                        center + Vec3::Y * size * 1.8,
                        center + axis * size,
                    ],
                    [0.0, 1.0, 0.0],
                    [rgb[0], rgb[1], rgb[2], fade * 0.85],
                );
                triangle(
                    &mut out,
                    [
                        center - axis * size,
                        center + axis * size,
                        center - Vec3::Y * size * 1.2,
                    ],
                    [0.0, 0.5, 1.0],
                    [rgb[0], rgb[1], rgb[2], fade * 0.55],
                );
            }
            continue;
        }
        let height = 1.0 * fade.max(0.12);
        let sway = (fire.age * 13.0 + fire.center.x * 1.7 + fire.center.z).sin() * 0.14;
        let center = fire.center + Vec3::new(0.0, -0.15, 0.0);
        for axis in [Vec3::X, Vec3::Z] {
            // Diamond-shaped crossed sheets, warm center and transparent tips.
            let bottom = center - Vec3::Y * 0.18;
            let middle = center + Vec3::Y * (height * 0.32) + Vec3::X * sway;
            let tip = center + Vec3::Y * height + Vec3::X * (sway * 1.6);
            triangle(
                &mut out,
                [bottom - axis * 0.33, middle, tip],
                [0.0, 0.5, 1.0],
                [1.0, 0.28, 0.025, fade * 0.9],
            );
            triangle(
                &mut out,
                [bottom + axis * 0.33, tip, middle],
                [0.0, 1.0, 0.5],
                [1.0, 0.55, 0.045, fade * 0.85],
            );
        }
        let ember = center + Vec3::new(sway * 2.0, 0.35 + fire.age * 0.85, 0.08);
        triangle(
            &mut out,
            [
                ember - Vec3::X * 0.045,
                ember + Vec3::Y * 0.13,
                ember + Vec3::X * 0.045,
            ],
            [0.0, 1.0, 0.0],
            [1.0, 0.85, 0.28, fade * 0.8],
        );
    }
    out
}

fn triangle(out: &mut Vec<f32>, positions: [Vec3; 3], uv_y: [f32; 3], color: [f32; 4]) {
    for (position, v) in positions.into_iter().zip(uv_y) {
        out.extend_from_slice(&[position.x, position.y, position.z, 0.5, v]);
        out.extend_from_slice(&color);
    }
}

pub(crate) struct FireRenderer {
    pipeline: wgpu::RenderPipeline,
    vertices: wgpu::Buffer,
    capacity_bytes: u64,
    count: u32,
    camera_group: wgpu::BindGroup,
}

impl FireRenderer {
    pub(crate) fn new(device: &wgpu::Device, camera: &wgpu::Buffer) -> Self {
        Self::with_capacity(device, camera, MAX_BYTES)
    }

    pub(crate) fn with_capacity(
        device: &wgpu::Device,
        camera: &wgpu::Buffer,
        capacity_bytes: u64,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("burn flame shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("fire.wgsl").into()),
        });
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("burn flame camera layout"),
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
            label: Some("burn flame camera"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera.as_entire_binding(),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("burn flame pipeline layout"),
            bind_group_layouts: &[Some(&camera_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("burn flame pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: (FLOATS * 4) as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x4],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &super::scene_ao::color_targets(post::HDR_FORMAT,Some(wgpu::BlendState::ALPHA_BLENDING)),
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
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
            multiview_mask: None,
            cache: None,
        });
        let vertices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("burn flame vertices"),
            size: capacity_bytes,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Keep the group owned with the pipeline instead of relying on the voxel shader layout.
        Self {
            pipeline,
            vertices,
            capacity_bytes,
            count: 0,
            camera_group,
        }
    }

    pub(crate) fn set(&mut self, queue: &wgpu::Queue, fires: &[VisualFire]) {
        let mesh = vertices(fires);
        self.set_mesh(queue, &mesh);
    }

    /// Shared bounded, depth-tested translucent streak geometry (fire and rain).
    pub(crate) fn set_mesh(&mut self, queue: &wgpu::Queue, mesh: &[f32]) {
        debug_assert!(std::mem::size_of_val(mesh) as u64 <= self.capacity_bytes);
        self.count = (mesh.len() / FLOATS) as u32;
        if !mesh.is_empty() {
            queue.write_buffer(&self.vertices, 0, bytemuck::cast_slice(mesh));
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) -> usize {
        if self.count == 0 {
            return 0;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.camera_group, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.draw(0..self.count, 0..1);
        self.count as usize / 3
    }
}
