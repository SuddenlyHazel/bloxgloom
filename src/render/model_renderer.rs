//! Native GLB rendering foundation: static uploads, dynamic joint/color inputs.
//! Scene installation and actor authority are intentionally outside this module.
use super::model_asset::{Appearance, Model, Vertex};
use glam::Mat4;
use wgpu::util::DeviceExt;
mod batches;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct MaterialUniform {
    base: [f32; 4],
    alpha: [f32; 4],
}
struct Mesh {
    index: wgpu::Buffer,
    count: u32,
    material: usize,
    source: batches::Source,
}
struct Material {
    group: wgpu::BindGroup,
    double_sided: bool,
}
pub(crate) struct ModelRenderer {
    pipeline: wgpu::RenderPipeline,
    double_sided: wgpu::RenderPipeline,
    camera: wgpu::BindGroup,
    joints: wgpu::Buffer,
    tints: wgpu::Buffer,
    vertices: wgpu::Buffer,
    meshes: Vec<Mesh>,
    materials: Vec<Material>,
    visible: Vec<bool>,
}
impl ModelRenderer {
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        camera: &wgpu::Buffer,
        model: &Model,
    ) -> Self {
        let joints = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GLB joint palette"),
            size: (model.bindings.len() * 64) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("GLB camera and pose"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let tints = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GLB part color palette"),
            size: (model.primitives.len() * 16) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GLB camera and pose"),
            layout: &camera_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: joints.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: tints.as_entire_binding(),
                },
            ],
        });
        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("GLB material"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let texture = |width, height, data: &[u8]| {
            device
                .create_texture_with_data(
                    queue,
                    &wgpu::TextureDescriptor {
                        label: Some("embedded GLB texture"),
                        size: wgpu::Extent3d {
                            width,
                            height,
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
                    data,
                )
                .create_view(&Default::default())
        };
        let images: Vec<_> = model
            .images
            .iter()
            .map(|i| texture(i.width, i.height, &i.rgba))
            .collect();
        let white = texture(1, 1, &[255; 4]);
        let materials = model
            .materials
            .iter()
            .map(|m| {
                let wrap = |w| match w {
                    gltf::texture::WrappingMode::ClampToEdge => wgpu::AddressMode::ClampToEdge,
                    gltf::texture::WrappingMode::Repeat => wgpu::AddressMode::Repeat,
                    gltf::texture::WrappingMode::MirroredRepeat => wgpu::AddressMode::MirrorRepeat,
                };
                let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                    label: Some("GLB pixel sampler"),
                    address_mode_u: wrap(m.wrap[0]),
                    address_mode_v: wrap(m.wrap[1]),
                    ..Default::default()
                });
                let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("GLB material parameters"),
                    contents: bytemuck::bytes_of(&MaterialUniform {
                        base: m.color,
                        alpha: [m.alpha_cutoff.unwrap_or(-1.0), 0.0, 0.0, 0.0],
                    }),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
                let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("GLB material"),
                    layout: &material_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(
                                m.texture.map_or(&white, |i| &images[i]),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: uniform.as_entire_binding(),
                        },
                    ],
                });
                Material {
                    group,
                    double_sided: m.double_sided,
                }
            })
            .collect();
        let (packed, sources) = batches::pack(model);
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("GLB shared static vertices"),
            contents: bytemuck::cast_slice(&packed),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let meshes = sources
            .into_iter()
            .map(|source| Mesh {
                index: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("GLB visible material indices"),
                    size: (source.index_count() * 4) as u64,
                    usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                count: 0,
                material: source.material,
                source,
            })
            .collect();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("native GLB shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("model_renderer.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("native GLB pipeline"),
            bind_group_layouts: &[Some(&camera_layout), Some(&material_layout)],
            immediate_size: 0,
        });
        let attributes = wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x3,2=>Float32x2,3=>Uint32x4,4=>Float32x4,5=>Uint32];
        let create = |cull_mode| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("native GLB pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &attributes,
                    })],
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode,
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
            })
        };
        Self {
            pipeline: create(Some(wgpu::Face::Back)),
            double_sided: create(None),
            camera,
            joints,
            tints,
            vertices,
            meshes,
            materials,
            visible: Vec::new(),
        }
    }
    pub(crate) fn set(&mut self, queue: &wgpu::Queue, pose: &[Mat4], appearance: Appearance) {
        let pose: Vec<_> = pose.iter().map(|m| m.to_cols_array()).collect();
        queue.write_buffer(&self.joints, 0, bytemuck::cast_slice(&pose));
        queue.write_buffer(&self.tints, 0, bytemuck::cast_slice(&appearance.colors));
        if self.visible != appearance.visible {
            self.visible = appearance.visible;
            for mesh in &mut self.meshes {
                let indices = mesh.source.visible_indices(&self.visible);
                mesh.count = indices.len() as u32;
                if !indices.is_empty() {
                    queue.write_buffer(&mesh.index, 0, bytemuck::cast_slice(&indices));
                }
            }
        }
    }
    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_bind_group(0, &self.camera, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        for mesh in &self.meshes {
            if mesh.count == 0 {
                continue;
            }
            let material = &self.materials[mesh.material];
            pass.set_pipeline(if material.double_sided {
                &self.double_sided
            } else {
                &self.pipeline
            });
            pass.set_bind_group(1, &material.group, &[]);
            pass.set_index_buffer(mesh.index.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.count, 0, 0..1);
        }
    }
    pub(crate) fn draw_calls(&self) -> usize {
        self.meshes.iter().filter(|m| m.count != 0).count()
    }
}
