//! Shared static actor meshes, drawn with one bounded instance buffer.
//! Cosmetics and lighting are public presentation state; no profile/inventory data.

mod appearance;
mod authored;
mod character;
pub(crate) use character::first_person::View as FirstPersonView;
mod character_asset;
pub(crate) use character_asset::tool_duration as character_tool_duration;
mod mesh;
pub(crate) mod motion;
#[cfg(test)]
mod moving_tests;
mod pipeline;
#[cfg(test)]
mod projectile_preview_tests;
mod ray_targets;
#[cfg(test)]
pub(in crate::render) use ray_targets::idle_ray_target;
mod reference;
#[cfg(test)]
mod shadow_tests;
#[cfg(test)]
mod tests;

use super::DEPTH_FORMAT;
use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use wgpu::util::DeviceExt;

pub(crate) const MAX_AVATARS: usize = 512;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) enum AvatarModel {
    Player,
    PackagedPlayer(u32),
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
    /// Validated, public packaged-model appearance and clip selection.
    pub model_pose: Option<bloxgloom_host_api::entity::VisualState>,
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
    /// Normalized source RGB and world-space upstream direction/confidence.
    pub glow_color: [u8; 3],
    pub glow_direction: [i8; 3],
    /// Presentation-only per-instance RGB multiplier.
    pub tint: [f32; 3],
}

impl VisualAvatar {
    /// Eight bytes retain both scalar levels, RGB and signed direction without
    /// adding attributes to the character pipeline's portable 16-slot budget.
    fn packed_light(&self) -> [u32; 2] {
        [
            u32::from_le_bytes([
                self.light_levels[0],
                self.light_levels[1],
                self.glow_color[0],
                self.glow_color[1],
            ]),
            u32::from_le_bytes([
                self.glow_color[2],
                self.glow_direction[0] as u8,
                self.glow_direction[1] as u8,
                self.glow_direction[2] as u8,
            ]),
        ]
    }
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
    light_levels: [u32; 2],
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
            light_levels: avatar.packed_light(),
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
    ray_assets: Vec<(
        AvatarModel,
        std::sync::Arc<super::trace::dynamic::DynamicAsset>,
    )>,
    ray_targets: super::trace::dynamic::DynamicTargets,
    characters: character::CharacterRenderer,
    authored: authored::Renderer,
    pipeline: wgpu::RenderPipeline,
    shadow_pipeline: wgpu::RenderPipeline,
    motion_pipeline: wgpu::RenderPipeline,
    motion: motion::Palette,
    camera_group: wgpu::BindGroup,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    instances: wgpu::Buffer,
    models: Vec<(AvatarModel, std::ops::Range<u32>)>,
    counts: Vec<u32>,
    shadow_counts: Vec<u32>,
}

impl AvatarRenderer {
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        camera_buffer: &wgpu::Buffer,
        catalog: &crate::content::Catalog,
    ) -> Self {
        let motion = motion::Palette::new(device, MAX_AVATARS);
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
            5 => Uint32x2,
            6 => Uint8x4,
            7 => Float32x4,
            9 => Float32x3,
            10 => Uint8x4,
            11 => Float32x4,
        ];
        let buffers = [
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
        ];
        let (pipeline, shadow_pipeline) =
            pipeline::pair(device, &shader, &pipeline_layout, format, &buffers, false);
        let motion_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("primitive actor motion layout"),
            bind_group_layouts: &[Some(&camera_layout), Some(&motion.layout)],
            immediate_size: 0,
        });
        let motion_pipeline = motion::pipeline(
            device,
            &shader,
            &motion_layout,
            &buffers,
            Some(wgpu::Face::Back),
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
        let authored = authored::Renderer::new(device, queue, format, &camera_layout, catalog);
        let ray_assets = if super::trace::dynamic::enabled() {
            models
                .iter()
                .map(|(model, range)| (*model, ray_targets::primitive(&mesh, range.clone())))
                .collect()
        } else {
            Vec::new()
        };
        Self {
            ray_assets,
            ray_targets: Default::default(),
            characters,
            authored,
            pipeline,
            shadow_pipeline,
            motion_pipeline,
            motion,
            camera_group,
            vertices,
            indices,
            instances,
            counts: vec![0; models.len()],
            shadow_counts: vec![0; models.len()],
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
        self.authored.first_person = view;
    }

    /// Caller supplies nearest first. The bounded instance buffer never grows
    /// with world/entity population or modded entity count.
    pub(crate) fn set(&mut self, queue: &wgpu::Queue, avatars: &[VisualAvatar]) {
        self.characters.set(queue, avatars);
        self.authored.set(queue, avatars);
        self.ray_targets.clear();
        self.ray_targets.append(&self.characters.ray_targets);
        self.ray_targets.append(&self.authored.ray_targets);
        for avatar in avatars.iter().take(MAX_AVATARS) {
            if matches!(avatar.model, AvatarModel::Registered(id) if self.authored.has_model(id)) {
                continue;
            }
            if let Some((_, asset)) = self
                .ray_assets
                .iter()
                .find(|(model, _)| *model == avatar.model)
            {
                let world = glam::Mat4::from_translation(avatar.position)
                    * if matches!(avatar.model, AvatarModel::Moving(_)) {
                        glam::Mat4::IDENTITY
                    } else {
                        glam::Mat4::from_rotation_y(avatar.pose[0])
                    };
                self.ray_targets.instances.push(ray_targets::instance(
                    asset.clone(),
                    avatar,
                    world,
                    super::trace::dynamic::Deformation::Primitive,
                    Vec::new(),
                    Vec::new(),
                ));
                self.ray_targets.instances.last_mut().unwrap().skip_primary = self
                    .characters
                    .first_person
                    .is_some_and(|view| view.id == avatar.id);
            }
        }
        self.motion.history.clear_pending();
        let mut instances = Vec::with_capacity(avatars.len().min(MAX_AVATARS));
        for (index, (model, _)) in self.models.iter().enumerate() {
            let start = instances.len();
            // Visible instances are a prefix of each model's full caster range.
            // Retain the first-person owner's exact world pose for shadows.
            for caster_only in [false, true] {
                for avatar in avatars.iter().take(MAX_AVATARS).filter(|avatar| {
                    let authored = matches!(avatar.model,
                        AvatarModel::Registered(id) if self.authored.has_model(id));
                    let owner = self
                        .characters
                        .first_person
                        .is_some_and(|view| avatar.id == view.id);
                    avatar.model == *model
                        && !authored
                        && owner == caster_only
                        && *model != AvatarModel::Player
                }) {
                    let instance = AvatarInstance::from(avatar);
                    if self.motion.enabled {
                        let data = glam::Mat4::from_cols(
                            avatar.position.extend(1.0),
                            glam::Vec4::from_array(instance.pose),
                            glam::Vec4::from_array(instance.orientation),
                            glam::Vec4::ZERO,
                        );
                        let identity = match avatar.model {
                            AvatarModel::Registered(id) => u64::from(id.0),
                            AvatarModel::Moving(id) => u64::from(id.0) | (1 << 32),
                            AvatarModel::Player => u64::MAX,
                            AvatarModel::PackagedPlayer(index) => u64::from(index) | (2 << 32),
                        };
                        self.motion
                            .history
                            .stage(avatar.id, identity, avatar.position, vec![data]);
                    }
                    instances.push(instance);
                }
                if !caster_only {
                    self.counts[index] = (instances.len() - start) as u32;
                }
            }
            self.shadow_counts[index] = (instances.len() - start) as u32;
        }
        if !instances.is_empty() {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&instances));
        }
    }

    pub(crate) fn ray_targets(&self) -> &super::trace::dynamic::DynamicTargets {
        &self.ray_targets
    }

    pub(crate) fn enable_motion(&mut self, enabled: bool) {
        self.motion.enable(enabled);
        self.characters.motion.enable(enabled);
        self.authored.enable_motion(enabled);
    }

    pub(crate) fn preview_animation_dt(&mut self, dt: f32) {
        self.authored.preview_dt = Some(dt);
    }

    pub(crate) fn prepare_motion(&self, queue: &wgpu::Queue, frame: &motion::Frame) {
        self.motion.prepare(queue, frame);
        self.characters.motion.prepare(queue, frame);
        self.authored.prepare_motion(queue, frame);
    }

    /// Call only after submitting the scene, never from set()/shadow preparation.
    pub(crate) fn submitted(&mut self) {
        self.motion.submitted();
        self.characters.motion.submitted();
        self.authored.submitted();
    }

    pub(crate) fn draw_motion<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        self.characters.draw_motion(pass, &self.camera_group);
        self.authored.draw_motion(pass, &self.camera_group);
        pass.set_bind_group(1, &self.motion.group, &[]);
        self.draw_instances(pass, &self.motion_pipeline, &self.camera_group, false);
    }

    pub(crate) fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) -> usize {
        let character_triangles = self.characters.draw(pass, &self.camera_group);
        character_triangles
            + self.authored.draw(pass, &self.camera_group, false)
            + self.draw_instances(pass, &self.pipeline, &self.camera_group, false)
    }

    pub(crate) fn draw_shadow<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        caster_camera_group: &'a wgpu::BindGroup,
    ) -> usize {
        let character_triangles = self.characters.draw_shadow(pass, caster_camera_group);
        character_triangles
            + self.authored.draw(pass, caster_camera_group, true)
            + self.draw_instances(pass, &self.shadow_pipeline, caster_camera_group, true)
    }

    fn draw_instances<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        pipeline: &'a wgpu::RenderPipeline,
        camera: &'a wgpu::BindGroup,
        shadow: bool,
    ) -> usize {
        let counts = if shadow {
            &self.shadow_counts
        } else {
            &self.counts
        };
        if counts.iter().all(|n| *n == 0) {
            return 0;
        }
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, camera, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_vertex_buffer(1, self.instances.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        let mut start = 0;
        let mut triangles = 0;
        for (((_, indices), count), full_count) in
            self.models.iter().zip(counts).zip(&self.shadow_counts)
        {
            if *count > 0 {
                pass.draw_indexed(indices.clone(), 0, start..start + count);
            }
            start += full_count;
            triangles += (indices.end - indices.start) as usize * *count as usize / 3;
        }
        triangles
    }
}

pub(crate) fn prepare_character_asset() {
    let _ = character_asset::CharacterAsset::builtin();
}

pub(crate) const SHADING_SHADER: &str = include_str!("avatars/shading.wgsl");

pub(super) fn character_shader(catalog: &crate::content::Catalog) -> String {
    motion::shader(
        super::daylight::shader(&format!(
            "{}\n{}\n{}\n{}",
            super::trace::dynamic::DEFORMATION_SHADER,
            super::trace::dynamic::MATERIAL_SHADER,
            SHADING_SHADER,
            include_str!("avatars/character.wgsl")
                .replace("// REGISTERED_PALETTES", &appearance::palettes(catalog))
                .replace(
                    "// FIRST_PERSON_JOINT_OFFSET",
                    &format!(
                        "const FIRST_PERSON_JOINT_OFFSET: u32 = {}u;",
                        MAX_AVATARS * character_asset::JOINT_COUNT
                    ),
                )
        )),
        2,
    )
}

pub(crate) fn ray_palettes(catalog: &crate::content::Catalog) -> String {
    appearance::palettes(catalog)
}
