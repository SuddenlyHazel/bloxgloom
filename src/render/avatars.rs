//! Shared static actor meshes, drawn with one bounded instance buffer.
//! Cosmetics and lighting are public presentation state; no profile/inventory data.

mod appearance;
mod character;
pub(crate) use character::first_person::View as FirstPersonView;
mod character_asset;
pub(crate) use character_asset::tool_duration as character_tool_duration;
mod mesh;
#[cfg(test)]
mod moving_tests;
mod pipeline;
#[cfg(test)]
mod projectile_preview_tests;
#[cfg(test)]
mod shadow_tests;
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
    Moving(crate::content::EntityTypeId),
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct VisualAvatar {
    pub motion: Option<MovingVisual>,
    pub animation: bloxgloom_host_api::entity::Animation,
    pub model: AvatarModel,
    /// Yaw, stride, body bob, squash. Never used for authoritative movement.
    pub pose: [f32; 4],
    /// Presentation-only walk seconds, idle seconds, walk blend and run blend.
    pub character_pose: [f32; 4],
    /// Local head yaw/pitch in radians, clamped to the hair-tested envelope.
    pub character_look: [f32; 2],
    pub character_crouch: f32,
    pub character_tool: Option<(bool, f32)>,
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

/// Authoritative motion metadata retained only for presentation interpolation.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MovingVisual {
    pub tick: u64,
    pub revision: u64,
    pub orientation: [f32; 4],
    pub velocity: [f32; 3],
    pub stopped: bool,
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
    orientation: [f32; 4],
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
            orientation: avatar
                .motion
                .map_or([0.0, 0.0, 0.0, 1.0], |motion| motion.orientation),
        }
    }
}

pub(crate) struct AvatarRenderer {
    characters: character::CharacterRenderer,
    pipeline: wgpu::RenderPipeline,
    shadow_pipeline: wgpu::RenderPipeline,
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
        let camera_layout = super::sun_shadow::camera_layout(device);
        let camera_group = super::sun_shadow::fallback_camera_group(device, camera_buffer);
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
            11 => Float32x4,
        ];
        let (pipeline, shadow_pipeline) = pipeline::pair(
            device,
            &shader,
            &pipeline_layout,
            format,
            &[
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
            false,
        );
        let mut mesh = mesh::AvatarMesh {
            vertices: Vec::new(),
            indices: Vec::new(),
        };
        let mut models = Vec::new();
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
        for (id, definition) in catalog.moving_entities() {
            let start = mesh.indices.len() as u32;
            for part in &definition.model {
                let base = mesh.vertices.len();
                // Rigid parts never receive the creature foot/body deformation.
                mesh::emit_cuboid(&mut mesh, part.min, part.max, 12);
                for vertex in &mut mesh.vertices[base..] {
                    vertex.color = part.color;
                }
            }
            models.push((AvatarModel::Moving(id), start..mesh.indices.len() as u32));
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
        let characters =
            character::CharacterRenderer::new(device, queue, format, &camera_layout, catalog);
        Self {
            characters,
            pipeline,
            shadow_pipeline,
            camera_group,
            vertices,
            indices,
            instances,
            counts: vec![0; models.len()],
            models,
        }
    }

    pub(crate) fn set_camera_group(&mut self, group: wgpu::BindGroup) {
        self.camera_group = group;
    }

    pub(crate) fn preview_character_clip(&mut self, clip: &'static str, time: f32) {
        self.characters.preview_clip(clip, time);
    }

    pub(crate) fn set_first_person(&mut self, view: Option<FirstPersonView>) {
        self.characters.first_person = view;
    }

    /// Caller supplies nearest first. The bounded instance buffer never grows
    /// with world/entity population or modded entity count.
    pub(crate) fn set(&mut self, queue: &wgpu::Queue, avatars: &[VisualAvatar]) {
        self.characters.set(queue, avatars);
        let mut instances = Vec::with_capacity(avatars.len().min(MAX_AVATARS));
        for (index, (model, _)) in self.models.iter().enumerate() {
            let start = instances.len();
            instances.extend(
                avatars
                    .iter()
                    .take(MAX_AVATARS)
                    .filter(|a| {
                        a.model == *model
                            && self
                                .characters
                                .first_person
                                .is_none_or(|view| a.id != view.id)
                            && *model != AvatarModel::Player
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
        character_triangles + self.draw_instances(pass, &self.pipeline, &self.camera_group)
    }

    pub(crate) fn draw_shadow<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        caster_camera_group: &'a wgpu::BindGroup,
    ) -> usize {
        let character_triangles = self.characters.draw_shadow(pass, caster_camera_group);
        character_triangles + self.draw_instances(pass, &self.shadow_pipeline, caster_camera_group)
    }

    fn draw_instances<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        pipeline: &'a wgpu::RenderPipeline,
        camera: &'a wgpu::BindGroup,
    ) -> usize {
        if self.counts.iter().all(|n| *n == 0) {
            return 0;
        }
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, camera, &[]);
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
        triangles
    }
}

pub(crate) fn prepare_character_asset() {
    let _ = character_asset::CharacterAsset::builtin();
}

pub(super) fn character_shader(catalog: &crate::content::Catalog) -> String {
    super::daylight::shader(
        &include_str!("avatars/character.wgsl")
            .replace("// REGISTERED_PALETTES", &appearance::palettes(catalog))
            .replace(
                "// FIRST_PERSON_JOINT_OFFSET",
                &format!(
                    "const FIRST_PERSON_JOINT_OFFSET: u32 = {}u;",
                    MAX_AVATARS * character_asset::JOINT_COUNT
                ),
            ),
    )
}
