//! Shared static actor meshes, drawn with one bounded instance buffer.
//! Cosmetics and lighting are public presentation state; no profile/inventory data.

mod appearance;
mod character;
mod character_asset;
mod mesh;
#[cfg(test)]
mod tests;

use super::DEPTH_FORMAT;
use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use wgpu::util::DeviceExt;

pub(crate) const MAX_AVATARS: usize = 512;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AvatarModel {
    Player,
    Registered(crate::content::EntityTypeId),
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct VisualAvatar {
    pub animation: bloxgloom_host_api::entity::Animation,
    pub model: AvatarModel,
    /// Yaw, stride, body bob, squash. Never used for authoritative movement.
    pub pose: [f32; 4],
    /// Separate from package pose offsets: walk seconds, idle seconds, walk blend.
    pub character_pose: [f32; 3],
    pub character_recipe: Option<crate::appearance::CharacterRecipe>,
    pub airborne: bool,
    pub id: u64,
    pub position: Vec3,
    pub cosmetics: [u8; 4],
    /// Sky/glow in 0..=15, with two reserved bytes for schema growth.
    pub light_levels: [u8; 4],
    /// Bounced RGB in 0..=255, plus one reserved byte.
    pub bounce: [u8; 4],
    pub glow_bounce: [u8; 4],
    /// Presentation-only per-instance RGB multiplier.
    pub tint: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct AvatarInstance {
    origin: [f32; 3],
    cosmetics: [u8; 4],
    light_levels: [u8; 4],
    bounce: [u8; 4],
    pose: [f32; 4],
    tint: [f32; 3],
    glow_bounce: [u8; 4],
}

impl From<&VisualAvatar> for AvatarInstance {
    fn from(avatar: &VisualAvatar) -> Self {
        Self {
            origin: avatar.position.to_array(),
            cosmetics: avatar.cosmetics,
            light_levels: avatar.light_levels,
            bounce: avatar.bounce,
            pose: avatar.pose,
            tint: avatar.tint,
            glow_bounce: avatar.glow_bounce,
        }
    }
}

pub(crate) struct AvatarRenderer {
    characters: character::CharacterRenderer,
    authored: bool,
    pipeline: wgpu::RenderPipeline,
    camera_group: wgpu::BindGroup,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    instances: wgpu::Buffer,
    models: Vec<(AvatarModel, std::ops::Range<u32>)>,
    counts: Vec<u32>,
}

impl AvatarRenderer {
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        camera_buffer: &wgpu::Buffer,
        catalog: &crate::content::Catalog,
    ) -> Self {
        let source = appearance::shader(catalog);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("public avatar shader"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("avatar camera layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
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
            8 => Float32x3,
        ];
        let instance_attrs = wgpu::vertex_attr_array![
            3 => Float32x3,
            4 => Uint8x4,
            5 => Uint8x4,
            6 => Uint8x4,
            7 => Float32x4,
            9 => Float32x3,
            10 => Uint8x4,
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
        let mut models = vec![(AvatarModel::Player, 0..mesh.indices.len() as u32)];
        for (id, definition) in catalog.mobile_entities() {
            let start = mesh.indices.len() as u32;
            for part in &definition.model {
                let base = mesh.vertices.len();
                mesh::emit_cuboid(
                    &mut mesh,
                    part.min,
                    part.max,
                    match part.motion {
                        bloxgloom_host_api::entity::PartMotion::Body => 5,
                        bloxgloom_host_api::entity::PartMotion::LeftFoot => 10,
                        bloxgloom_host_api::entity::PartMotion::RightFoot => 11,
                    },
                );
                for vertex in &mut mesh.vertices[base..] {
                    vertex.color = part.color;
                }
            }
            models.push((
                AvatarModel::Registered(id),
                start..mesh.indices.len() as u32,
            ));
        }
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
        let characters = character::CharacterRenderer::new(device, queue, format, &camera_layout);
        Self {
            characters,
            authored: false,
            pipeline,
            camera_group,
            vertices,
            indices,
            instances,
            counts: vec![0; models.len()],
            models,
        }
    }

    pub(crate) fn preview_character_clip(&mut self, clip: &'static str, time: f32) {
        self.authored = true;
        self.characters.preview_clip(clip, time);
    }

    pub(crate) fn set_authored(&mut self, enabled: bool) {
        self.authored = enabled;
    }

    /// Caller supplies nearest first. The bounded instance buffer never grows
    /// with world/entity population or modded entity count.
    pub(crate) fn set(&mut self, queue: &wgpu::Queue, avatars: &[VisualAvatar]) {
        self.characters.set(queue, avatars, self.authored);
        let mut instances = Vec::with_capacity(avatars.len().min(MAX_AVATARS));
        for (index, (model, _)) in self.models.iter().enumerate() {
            let start = instances.len();
            instances.extend(
                avatars
                    .iter()
                    .take(MAX_AVATARS)
                    .filter(|a| {
                        a.model == *model
                            && !(self.authored
                                && *model == AvatarModel::Player
                                && a.character_recipe.is_some())
                    })
                    .map(AvatarInstance::from),
            );
            self.counts[index] = (instances.len() - start) as u32;
        }
        if !instances.is_empty() {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&instances));
        }
    }

    pub(crate) fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) -> usize {
        let character_triangles = self.characters.draw(pass, &self.camera_group);
        if self.counts.iter().all(|n| *n == 0) {
            return character_triangles;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.camera_group, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_vertex_buffer(1, self.instances.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        let mut start = 0;
        let mut triangles = 0;
        for ((_, indices), count) in self.models.iter().zip(&self.counts) {
            if *count > 0 {
                pass.draw_indexed(indices.clone(), 0, start..start + count);
            }
            start += count;
            triangles += (indices.end - indices.start) as usize * *count as usize / 3;
        }
        triangles + character_triangles
    }
}

pub(crate) fn character_eye_names() -> &'static [&'static str; 8] {
    &character_asset::EYE_NAMES
}
pub(crate) fn character_mouth_names() -> &'static [&'static str; 6] {
    &character_asset::MOUTH_NAMES
}
