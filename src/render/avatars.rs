//! Shared static actor meshes, drawn with one bounded instance buffer.
//! Cosmetics and lighting are public presentation state; no profile/inventory data.

mod mesh;
mod mossbun;

use super::{DEPTH_FORMAT, shader::with_world_sun};
use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use wgpu::util::DeviceExt;

pub(crate) const MAX_AVATARS: usize = 512;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AvatarModel {
    Player,
    Mossbun,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct VisualAvatar {
    pub model: AvatarModel,
    /// Yaw and cosmetic stride. Never used for authoritative movement.
    pub pose: [f32; 2],
    pub id: u64,
    pub position: Vec3,
    pub cosmetics: [u8; 4],
    /// Sky/glow in 0..=15, with two reserved bytes for schema growth.
    pub light_levels: [u8; 4],
    /// Bounced RGB in 0..=255, plus one reserved byte.
    pub bounce: [u8; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct AvatarInstance {
    origin: [f32; 3],
    cosmetics: [u8; 4],
    light_levels: [u8; 4],
    bounce: [u8; 4],
    pose: [f32; 2],
}

pub(crate) struct AvatarRenderer {
    pipeline: wgpu::RenderPipeline,
    camera_group: wgpu::BindGroup,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    instances: wgpu::Buffer,
    player_indices: u32,
    total_indices: u32,
    counts: [u32; 2],
}

impl AvatarRenderer {
    pub(crate) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        camera_buffer: &wgpu::Buffer,
    ) -> Self {
        let source = with_world_sun(include_str!("avatars/shader.wgsl"));
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("public avatar shader"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("avatar camera layout"),
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
            label: Some("avatar camera group"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("avatar pipeline layout"),
            bind_group_layouts: &[Some(&camera_layout)],
            immediate_size: 0,
        });
        let vertex_attrs = wgpu::vertex_attr_array![
            0 => Float32x3,
            1 => Float32x3,
            2 => Uint32,
        ];
        let instance_attrs = wgpu::vertex_attr_array![
            3 => Float32x3,
            4 => Uint8x4,
            5 => Uint8x4,
            6 => Uint8x4,
            7 => Float32x2,
        ];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("instanced public avatars"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[
                    Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<mesh::AvatarVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &vertex_attrs,
                    }),
                    Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<AvatarInstance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &instance_attrs,
                    }),
                ],
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
        let mut mesh = mesh::build();
        let player_indices = mesh.indices.len() as u32;
        mossbun::append(&mut mesh);
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("shared avatar vertices"),
            contents: bytemuck::cast_slice(&mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("shared avatar indices"),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bounded public avatar instances"),
            size: (MAX_AVATARS * std::mem::size_of::<AvatarInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            camera_group,
            vertices,
            indices,
            instances,
            player_indices,
            total_indices: mesh.indices.len() as u32,
            counts: [0; 2],
        }
    }

    /// Caller supplies nearest first. The bounded instance buffer never grows
    /// with world/entity population or modded entity count.
    pub(crate) fn set(&mut self, queue: &wgpu::Queue, avatars: &[VisualAvatar]) {
        let mut instances = Vec::with_capacity(avatars.len().min(MAX_AVATARS));
        for (index, model) in [AvatarModel::Player, AvatarModel::Mossbun]
            .into_iter()
            .enumerate()
        {
            let start = instances.len();
            instances.extend(
                avatars
                    .iter()
                    .take(MAX_AVATARS)
                    .filter(|a| a.model == model)
                    .map(|avatar| AvatarInstance {
                        origin: avatar.position.to_array(),
                        cosmetics: avatar.cosmetics,
                        light_levels: avatar.light_levels,
                        bounce: avatar.bounce,
                        pose: avatar.pose,
                    }),
            );
            self.counts[index] = (instances.len() - start) as u32;
        }
        if !instances.is_empty() {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&instances));
        }
    }

    pub(crate) fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) -> usize {
        if self.counts == [0; 2] {
            return 0;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.camera_group, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_vertex_buffer(1, self.instances.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint16);
        let [players, buns] = self.counts;
        if players > 0 {
            pass.draw_indexed(0..self.player_indices, 0, 0..players);
        }
        if buns > 0 {
            pass.draw_indexed(
                self.player_indices..self.total_indices,
                0,
                players..players + buns,
            );
        }
        (self.player_indices * players + (self.total_indices - self.player_indices) * buns) as usize
            / 3
    }
}
