//! Bounded, instanced authored characters. Only joint matrices change per frame.
//! This is a builtin presentation asset, not an untrusted runtime model importer.
use super::{
    AvatarInstance, AvatarModel, MAX_AVATARS, VisualAvatar, character_asset::CharacterAsset,
};
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

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

pub(super) struct CharacterRenderer {
    asset: CharacterAsset,
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    instances: wgpu::Buffer,
    joints: wgpu::Buffer,
    count: u32,
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
            (MAX_AVATARS * std::mem::size_of::<AvatarInstance>()) as u64,
            wgpu::BufferUsages::VERTEX,
        );
        let joints = dynamic(
            "bounded character joints",
            (MAX_AVATARS * JOINTS * 64) as u64,
            wgpu::BufferUsages::STORAGE,
        );
        let texture_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
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
                texture_entry(1),
                texture_entry(2),
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
        let hair = texture(
            device,
            queue,
            super::character_asset::HAIR_PNG,
            "character hair atlas",
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
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&hair),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("authored character shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("character.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("authored character pipeline"),
            bind_group_layouts: &[Some(camera_layout), Some(&layout)],
            immediate_size: 0,
        });
        let vertex_attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Uint32, 8 => Float32x2, 11 => Uint32];
        let instance_attributes = wgpu::vertex_attr_array![3 => Float32x3, 4 => Uint8x4, 5 => Uint8x4, 6 => Uint8x4, 7 => Float32x4, 9 => Float32x3, 10 => Uint8x4];
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
                        array_stride: std::mem::size_of::<AvatarInstance>() as u64,
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
            preview_clip: None,
        }
    }

    pub(super) fn preview_clip(&mut self, clip: &'static str, time: f32) {
        self.preview_clip = Some((clip, time));
    }

    pub(super) fn set(&mut self, queue: &wgpu::Queue, avatars: &[VisualAvatar], enabled: bool) {
        self.count = 0;
        if !enabled {
            return;
        }
        let mut instances = Vec::new();
        let mut joints = Vec::new();
        for avatar in avatars
            .iter()
            .take(MAX_AVATARS)
            .filter(|a| a.model == AvatarModel::Player)
        {
            instances.push(AvatarInstance::from(avatar));
            let pose = match self.preview_clip {
                Some((clip, time)) => self.asset.sample(clip, time),
                None => self.asset.sample_blended(
                    avatar.character_pose[1],
                    avatar.character_pose[0],
                    avatar.character_pose[2],
                ),
            };
            joints.extend(pose.iter().map(|matrix| matrix.to_cols_array()));
        }
        self.count = instances.len() as u32;
        if self.count != 0 {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&instances));
            queue.write_buffer(&self.joints, 0, bytemuck::cast_slice(&joints));
        }
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
        pass.draw_indexed(0..self.asset.indices.len() as u32, 0, 0..self.count);
        self.asset.indices.len() * self.count as usize / 3
    }
}

/// Keep native atlas dimensions/UVs and pixel edges. Never use terrain resizing.
fn texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    png: &[u8],
    label: &str,
) -> wgpu::TextureView {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().expect("builtin character PNG header");
    let mut bytes = vec![0; reader.output_buffer_size().expect("bounded builtin PNG")];
    let info = reader
        .next_frame(&mut bytes)
        .expect("builtin character PNG pixels");
    let rgba = match info.color_type {
        png::ColorType::Rgba => bytes[..info.buffer_size()].to_vec(),
        png::ColorType::Rgb => bytes[..info.buffer_size()]
            .chunks_exact(3)
            .flat_map(|rgb| [rgb[0], rgb[1], rgb[2], 255])
            .collect(),
        _ => panic!("builtin character PNG must be RGB/RGBA"),
    };
    let texture = device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: info.width,
                height: info.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        &rgba,
    );
    texture.create_view(&Default::default())
}

#[cfg(test)]
mod tests;
