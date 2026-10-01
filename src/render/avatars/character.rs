//! Bounded, instanced authored characters. Only joint matrices change per frame.
//! This is a builtin presentation asset, not an untrusted runtime model importer.
use super::{
    AvatarInstance, AvatarModel, MAX_AVATARS, VisualAvatar, character_asset::CharacterAsset,
};
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;
pub(crate) mod first_person;
mod material;
use material::texture;

const MATERIALS: usize = crate::appearance::HAIR.len();

const JOINTS: usize = super::character_asset::JOINT_COUNT;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
    joint: u32,
    uv: [f32; 2],
    material: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CharacterInstance {
    actor: AvatarInstance,
    recipe: [u8; 4],
    iris: [u8; 4],
}

pub(super) struct CharacterRenderer {
    asset: CharacterAsset,
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    instances: wgpu::Buffer,
    joints: wgpu::Buffer,
    count: u32,
    pub(super) first_person: Option<first_person::View>,
    material_ranges: [std::ops::Range<u32>; MATERIALS],
    style_counts: [u32; MATERIALS],
    preview_clip: Option<(&'static str, f32)>,
}

impl CharacterRenderer {
    pub(super) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        camera_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let asset = CharacterAsset::builtin();
        assert_eq!(asset.joints.len(), JOINTS);
        assert_eq!(super::character_asset::HAIR_PNGS.len() + 1, MATERIALS);
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
            (MAX_AVATARS * JOINTS * 64) as u64,
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
                texture_entry(1, wgpu::TextureViewDimension::D2),
                texture_entry(4, wgpu::TextureViewDimension::D2Array),
                texture_entry(5, wgpu::TextureViewDimension::D2Array),
                texture_entry(6, wgpu::TextureViewDimension::D2Array),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let body = texture(
            device,
            queue,
            super::character_asset::BODY_PNG,
            "character body atlas",
        );
        let hair = material::array(
            device,
            queue,
            &super::character_asset::HAIR_PNGS,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            "native-size hair atlases",
        );
        let face_layers: Vec<_> = std::iter::once(super::character_asset::CLEAN_FACE_PNG)
            .chain(super::character_asset::EYE_PNGS)
            .chain(super::character_asset::MOUTH_PNGS)
            .collect();
        let faces = material::array(
            device,
            queue,
            &face_layers,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            "character face features",
        );
        let masks = material::array(
            device,
            queue,
            &super::character_asset::IRIS_MASK_PNGS,
            wgpu::TextureFormat::Rgba8Unorm,
            "character iris shade masks",
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
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&faces),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&masks),
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("authored character shader"),
            source: wgpu::ShaderSource::Wgsl(
                super::super::fog::shader(include_str!("character.wgsl")).into(),
            ),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("authored character pipeline"),
            bind_group_layouts: &[Some(camera_layout), Some(&layout)],
            immediate_size: 0,
        });
        let vertex_attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Uint32, 8 => Float32x2, 11 => Uint32];
        let mut instance_attributes = wgpu::vertex_attr_array![3 => Float32x3, 4 => Uint8x4, 5 => Uint8x4, 6 => Uint8x4, 7 => Float32x4, 9 => Float32x3, 10 => Uint8x4, 12 => Uint8x4, 13 => Uint8x4];
        // Actor instances also carry a rigid-object quaternion, which this
        // character shader ignores. Recipe bytes follow the entire actor.
        instance_attributes[7].offset = std::mem::offset_of!(CharacterInstance, recipe) as u64;
        instance_attributes[8].offset = std::mem::offset_of!(CharacterInstance, iris) as u64;
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("instanced authored characters"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[
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
                ],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: super::DEPTH_FORMAT,
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
        Self {
            asset,
            pipeline,
            group,
            vertices,
            indices,
            instances,
            joints,
            count: 0,
            first_person: None,
            material_ranges,
            style_counts: [0; MATERIALS],
            preview_clip: None,
        }
    }

    pub(super) fn preview_clip(&mut self, clip: &'static str, time: f32) {
        self.preview_clip = Some((clip, time));
    }

    pub(super) fn set(&mut self, queue: &wgpu::Queue, avatars: &[VisualAvatar], enabled: bool) {
        self.count = 0;
        self.style_counts.fill(0);
        if !enabled {
            return;
        }
        let mut instances = Vec::new();
        let mut joints = Vec::new();
        // Admission stays nearest-first; grouping only reorders already-admitted
        // actors. The GPU processes the body and the selected hair, never the kit.
        for style in 0..MATERIALS {
            let start = instances.len();
            for avatar in avatars.iter().take(MAX_AVATARS).filter(|a| {
                a.model == AvatarModel::Player
                    && a.character_recipe
                        .is_some_and(|recipe| recipe.valid() && usize::from(recipe.hair) == style)
            }) {
                let recipe = avatar.character_recipe.expect("filtered character recipe");
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
                });
                let mut pose = match self.preview_clip {
                    Some((clip, time)) => self.asset.sample(clip, time),
                    None => self.asset.sample_gameplay(
                        avatar.character_pose[1],
                        avatar.character_pose[0],
                        avatar.character_pose[2],
                        avatar.character_crouch,
                        avatar.character_tool,
                    ),
                };
                if let Some(view) = first_person {
                    view.prepare_pose(&mut pose);
                }
                joints.extend(pose.iter().map(|matrix| matrix.to_cols_array()));
            }
            self.style_counts[style] = (instances.len() - start) as u32;
        }
        self.count = instances.len() as u32;
        if self.count != 0 {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&instances));
            queue.write_buffer(&self.joints, 0, bytemuck::cast_slice(&joints));
        }
    }

    fn triangles(&self) -> usize {
        self.material_ranges[0].len() * self.count as usize / 3
            + (1..MATERIALS)
                .map(|style| {
                    self.material_ranges[style].len() * self.style_counts[style] as usize / 3
                })
                .sum::<usize>()
    }

    pub(super) fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        camera: &'a wgpu::BindGroup,
    ) -> usize {
        if self.count == 0 {
            return 0;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, camera, &[]);
        pass.set_bind_group(1, &self.group, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_vertex_buffer(1, self.instances.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(self.material_ranges[0].clone(), 0, 0..self.count);
        let mut start = self.style_counts[0];
        for style in 1..MATERIALS {
            let count = self.style_counts[style];
            if count > 0 {
                pass.draw_indexed(self.material_ranges[style].clone(), 0, start..start + count);
            }
            start += count;
        }
        self.triangles()
    }
}

#[cfg(test)]
mod tests;
