//! Bounded, instanced authored characters. Only joint matrices change per frame.
//! This is a builtin presentation asset, not an untrusted runtime model importer.
use super::{
    AvatarInstance, AvatarModel, MAX_AVATARS, VisualAvatar, character_asset::CharacterAsset,
};
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;
pub(crate) mod first_person;
mod material;

const MATERIALS: usize = super::character_asset::MATERIAL_COUNT;
const STYLES: usize = crate::appearance::HAIR.len();
const GROUPS: usize = STYLES * 2;

const JOINTS: usize = super::character_asset::JOINT_COUNT;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
    joint: u32,
    uv: [f32; 2],
    material: u32,
    surface: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CharacterInstance {
    actor: AvatarInstance,
    recipe: [u8; 4],
    iris: [u8; 4],
    hair_color_body: [u8; 4],
}

pub(super) struct CharacterRenderer {
    ray_assets: Vec<std::sync::Arc<crate::render::trace::dynamic::DynamicAsset>>,
    pub(super) ray_targets: crate::render::trace::dynamic::DynamicTargets,
    asset: &'static CharacterAsset,
    pipeline: wgpu::RenderPipeline,
    shadow_pipeline: wgpu::RenderPipeline,
    motion_pipeline: wgpu::RenderPipeline,
    pub(super) motion: super::motion::Palette,
    group: wgpu::BindGroup,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    instances: wgpu::Buffer,
    joints: wgpu::Buffer,
    count: u32,
    pub(super) first_person: Option<first_person::View>,
    material_ranges: [std::ops::Range<u32>; MATERIALS],
    style_counts: [u32; GROUPS],
    preview_clip: Option<(&'static str, f32)>,
}

impl CharacterRenderer {
    pub(super) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        camera_layout: &wgpu::BindGroupLayout,
        catalog: &crate::content::Catalog,
    ) -> Self {
        let asset = CharacterAsset::builtin();
        let ray_assets = if crate::render::trace::dynamic::enabled() {
            (0..GROUPS)
                .map(|group| {
                    super::ray_targets::character(
                        asset,
                        if group / STYLES == 0 { 0 } else { 14 },
                        (group % STYLES) as u32,
                    )
                })
                .collect()
        } else {
            Vec::new()
        };
        assert_eq!(asset.animation.nodes.len(), JOINTS);

        let mut material_ranges: [std::ops::Range<u32>; MATERIALS] = std::array::from_fn(|_| 0..0);
        for (index, triangle) in asset.indices.chunks_exact(3).enumerate() {
            let material = asset.vertices[triangle[0] as usize].material as usize;
            let range = &mut material_ranges[material];
            if range.start == range.end {
                range.start = (index * 3) as u32;
            }
            range.end = (index * 3 + 3) as u32;
        }
        let vertices: Vec<_> = asset
            .vertices
            .iter()
            .map(|v| Vertex {
                position: v.position,
                normal: v.normal,
                joint: v.joint as u32,
                uv: v.uv,
                material: v.material,
                surface: v.surface | (255 << 8) | (v.texture << 16),
            })
            .collect();
        let buffer = |label, contents, usage| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents,
                usage,
            })
        };
        let vertices = buffer(
            "authored character vertices",
            bytemuck::cast_slice(&vertices),
            wgpu::BufferUsages::VERTEX,
        );
        let indices = buffer(
            "authored character indices",
            bytemuck::cast_slice(&asset.indices),
            wgpu::BufferUsages::INDEX,
        );
        let dynamic = |label, size, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let instances = dynamic(
            "bounded character instances",
            (MAX_AVATARS * std::mem::size_of::<CharacterInstance>()) as u64,
            wgpu::BufferUsages::VERTEX,
        );
        let joints = dynamic(
            "bounded character joints",
            // One extra rig keeps camera framing out of the world shadow.
            ((MAX_AVATARS + 1) * JOINTS * 64) as u64,
            wgpu::BufferUsages::STORAGE,
        );
        let texture_entry = |binding, dimension| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: dimension,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("character materials and joints"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                texture_entry(1, wgpu::TextureViewDimension::D2Array),
                texture_entry(4, wgpu::TextureViewDimension::D2Array),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let body = material::array(
            device,
            queue,
            &asset.images[..1],
            "embedded player body atlas",
        );
        let hair = material::array(
            device,
            queue,
            &asset.images[1..],
            "embedded player hair atlases",
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("nearest character pixels"),
            ..Default::default()
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("character materials and joints"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: joints.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&body),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&hair),
                },
            ],
        });
        let motion = super::motion::Palette::new(device, MAX_AVATARS * JOINTS);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("authored character shader"),
            source: wgpu::ShaderSource::Wgsl(super::character_shader(catalog).into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("authored character pipeline"),
            bind_group_layouts: &[Some(camera_layout), Some(&layout)],
            immediate_size: 0,
        });
        let vertex_attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Uint32, 8 => Float32x2, 11 => Uint32, 15 => Uint32];
        let mut instance_attributes = wgpu::vertex_attr_array![3 => Float32x3, 4 => Uint8x4, 5 => Uint32x2, 6 => Uint8x4, 7 => Float32x4, 9 => Float32x3, 10 => Uint8x4, 12 => Uint8x4, 13 => Uint8x4, 14 => Uint8x4];
        // Actor instances also carry a rigid-object quaternion, which this
        // character shader ignores. Recipe bytes follow the entire actor.
        instance_attributes[7].offset = std::mem::offset_of!(CharacterInstance, recipe) as u64;
        instance_attributes[8].offset = std::mem::offset_of!(CharacterInstance, iris) as u64;
        instance_attributes[9].offset =
            std::mem::offset_of!(CharacterInstance, hair_color_body) as u64;
        let buffers = [
            Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &vertex_attributes,
            }),
            Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<CharacterInstance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &instance_attributes,
            }),
        ];
        let (pipeline, shadow_pipeline) =
            super::pipeline::pair(device, &shader, &pipeline_layout, format, &buffers, true);
        let motion_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("character motion layout"),
            bind_group_layouts: &[Some(camera_layout), Some(&layout), Some(&motion.layout)],
            immediate_size: 0,
        });
        let motion_pipeline =
            super::motion::pipeline(device, &shader, &motion_layout, &buffers, None);
        Self {
            ray_assets,
            ray_targets: Default::default(),
            asset,
            pipeline,
            shadow_pipeline,
            motion_pipeline,
            motion,
            group,
            vertices,
            indices,
            instances,
            joints,
            count: 0,
            first_person: None,
            material_ranges,
            style_counts: [0; GROUPS],
            preview_clip: None,
        }
    }

    pub(super) fn preview_clip(&mut self, clip: &'static str, time: f32) {
        self.preview_clip = Some((clip, time));
    }

    pub(super) fn set(&mut self, queue: &wgpu::Queue, avatars: &[VisualAvatar]) {
        self.ray_targets.clear();
        self.count = 0;
        self.motion.history.clear_pending();
        self.style_counts.fill(0);
        let mut instances = Vec::new();
        let mut joints = Vec::new();
        let mut first_person_joints = None;
        // Admission stays nearest-first; grouping only reorders already-admitted
        // actors. The GPU processes the body and the selected hair, never the kit.
        for group in 0..GROUPS {
            let style = group % STYLES;
            let body = group / STYLES;
            let start = instances.len();
            for avatar in avatars.iter().take(MAX_AVATARS).filter(|a| {
                a.model == AvatarModel::Player && {
                    let recipe = a.character_recipe.unwrap_or_default();
                    recipe.valid()
                        && usize::from(recipe.hair) == style
                        && usize::from(recipe.body) == body
                }
            }) {
                let recipe = avatar.character_recipe.unwrap_or_default();
                let iris = recipe
                    .iris
                    .map_or([0; 4], |rgb| [rgb[0], rgb[1], rgb[2], 1]);
                let first_person = self.first_person.filter(|view| view.id == avatar.id);
                let mut actor = AvatarInstance::from(avatar);
                if let Some(view) = first_person {
                    actor.pose[3] = view.eye_height;
                }
                instances.push(CharacterInstance {
                    actor,
                    recipe: [
                        recipe.eyes,
                        recipe.mouth,
                        recipe.hair,
                        u8::from(first_person.is_some()),
                    ],
                    iris,
                    hair_color_body: [
                        recipe.hair_color[0],
                        recipe.hair_color[1],
                        recipe.hair_color[2],
                        recipe.body,
                    ],
                });
                let mut pose = match self.preview_clip {
                    Some((clip, time)) => self.asset.sample(clip, time),
                    None => self.asset.sample_gameplay_look(
                        avatar.character_pose[1],
                        avatar.character_pose[0],
                        avatar.character_pose[2],
                        avatar.character_pose[3],
                        avatar.character_crouch,
                        avatar.character_tool,
                        avatar.character_look,
                    ),
                };
                joints.extend(pose.iter().map(|matrix| matrix.to_cols_array()));
                if let Some(asset) = self.ray_assets.get(group) {
                    let world = glam::Mat4::from_translation(avatar.position)
                        * glam::Mat4::from_rotation_y(avatar.pose[0]);
                    self.ray_targets
                        .instances
                        .push(super::ray_targets::instance(
                            asset.clone(),
                            avatar,
                            world,
                            crate::render::trace::dynamic::Deformation::Character,
                            pose.to_vec(),
                            Vec::new(),
                        ));
                    self.ray_targets.instances.last_mut().unwrap().skip_primary =
                        first_person.is_some();
                }
                if let Some(view) = first_person {
                    view.prepare_pose(&mut pose, avatar.character_tool);
                    first_person_joints = Some(pose.map(|matrix| matrix.to_cols_array()));
                }
                if self.motion.enabled {
                    let world = glam::Mat4::from_translation(avatar.position)
                        * glam::Mat4::from_rotation_y(avatar.pose[0]);
                    let identity = super::motion::fingerprint((
                        recipe,
                        avatar.cosmetics,
                        first_person.is_some(),
                    ));
                    self.motion.history.stage(
                        avatar.id,
                        identity,
                        avatar.position,
                        pose.iter().map(|matrix| world * *matrix).collect(),
                    );
                }
            }
            self.style_counts[group] = (instances.len() - start) as u32;
        }
        self.count = instances.len() as u32;
        if self.count != 0 {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&instances));
            queue.write_buffer(&self.joints, 0, bytemuck::cast_slice(&joints));
            if let Some(pose) = first_person_joints {
                queue.write_buffer(
                    &self.joints,
                    (MAX_AVATARS * JOINTS * 64) as u64,
                    bytemuck::cast_slice(&pose),
                );
            }
        }
    }

    fn triangles(&self) -> usize {
        (0..GROUPS)
            .map(|group| {
                let body = if group / STYLES == 0 { 0 } else { 14 };
                let style = group % STYLES;
                let indices = self.material_ranges[body].len()
                    + if style == 0 {
                        0
                    } else {
                        self.material_ranges[style].len()
                    };
                indices * self.style_counts[group] as usize / 3
            })
            .sum()
    }

    pub(super) fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        camera: &'a wgpu::BindGroup,
    ) -> usize {
        self.draw_instances(pass, camera, &self.pipeline)
    }

    pub(super) fn draw_motion<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        camera: &'a wgpu::BindGroup,
    ) {
        pass.set_bind_group(2, &self.motion.group, &[]);
        self.draw_instances(pass, camera, &self.motion_pipeline);
    }

    pub(super) fn draw_shadow<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        camera: &'a wgpu::BindGroup,
    ) -> usize {
        self.draw_instances(pass, camera, &self.shadow_pipeline)
    }

    fn draw_instances<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        camera: &'a wgpu::BindGroup,
        pipeline: &'a wgpu::RenderPipeline,
    ) -> usize {
        if self.count == 0 {
            return 0;
        }
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, camera, &[]);
        pass.set_bind_group(1, &self.group, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_vertex_buffer(1, self.instances.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        let mut start = 0;
        for group in 0..GROUPS {
            let count = self.style_counts[group];
            let body = if group / STYLES == 0 { 0 } else { 14 };
            let style = group % STYLES;
            if count > 0 {
                pass.draw_indexed(self.material_ranges[body].clone(), 0, start..start + count);
                if style > 0 {
                    pass.draw_indexed(self.material_ranges[style].clone(), 0, start..start + count);
                }
            }
            start += count;
        }
        self.triangles()
    }
}

#[cfg(test)]
mod tests;
