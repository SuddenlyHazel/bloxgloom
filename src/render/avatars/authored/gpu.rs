//! Shared static model meshes and one bounded, cross-model instance palette.
use super::*;
use crate::render::model_asset::Vertex;
use wgpu::util::DeviceExt;
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct Instance {
    pub origin: [f32; 3],
    pub yaw_scale: [f32; 2],
    pub light_levels: [u8; 4],
    pub bounce: [u8; 4],
    pub glow_bounce: [u8; 4],
    pub tint: [f32; 3],
    pub offsets: [u32; 2],
    pub first_person_offset: [f32; 3],
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct Part {
    pub color: [f32; 4],
    pub flags: [u32; 4],
}
struct Mesh {
    indices: wgpu::Buffer,
    count: u32,
    material: usize,
}
pub(super) struct ModelGpu {
    vertices: wgpu::Buffer,
    meshes: Vec<Mesh>,
    materials: Vec<material::Material>,
}
pub(super) struct Gpu {
    pipeline: wgpu::RenderPipeline,
    shadow: wgpu::RenderPipeline,
    double_sided: wgpu::RenderPipeline,
    shadow_double_sided: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    joints: wgpu::Buffer,
    parts: wgpu::Buffer,
    instances: wgpu::Buffer,
    models: Vec<ModelGpu>,
}
impl Gpu {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        camera: &wgpu::BindGroupLayout,
        models: &[Asset],
    ) -> Self {
        let dynamic = |label, size: u64, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: size.max(4),
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let joints = dynamic(
            "bounded creature GLB joint palette",
            (MAX_AVATARS
                * models
                    .iter()
                    .map(|m| m.model.bindings.len())
                    .max()
                    .unwrap_or(1)
                * 64) as u64,
            wgpu::BufferUsages::STORAGE,
        );
        let parts = dynamic(
            "bounded creature GLB appearance palette",
            (MAX_AVATARS
                * models
                    .iter()
                    .map(|m| m.model.primitives.len())
                    .max()
                    .unwrap_or(1)
                * std::mem::size_of::<Part>()) as u64,
            wgpu::BufferUsages::STORAGE,
        );
        let instances = dynamic(
            "bounded creature GLB instances",
            (MAX_AVATARS * std::mem::size_of::<Instance>()) as u64,
            wgpu::BufferUsages::VERTEX,
        );
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("creature GLB instance palettes"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
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
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("creature GLB instance palettes"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: joints.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: parts.as_entire_binding(),
                },
            ],
        });
        let materials = material::layout(device);
        let models = models
            .iter()
            .map(|asset| {
                let model = &asset.model;
                let mut vertices = Vec::new();
                let mut indices: Vec<Vec<u32>> =
                    (0..model.materials.len()).map(|_| Vec::new()).collect();
                for primitive in &model.primitives {
                    let first = vertices.len() as u32;
                    vertices.extend_from_slice(&primitive.vertices);
                    indices[primitive.material].extend(primitive.indices.iter().map(|i| first + i));
                }
                let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("shared creature GLB vertices"),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                let meshes = indices
                    .into_iter()
                    .enumerate()
                    .filter(|(_, i)| !i.is_empty())
                    .map(|(material, indices)| Mesh {
                        indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("shared creature GLB material indices"),
                            contents: bytemuck::cast_slice(&indices),
                            usage: wgpu::BufferUsages::INDEX,
                        }),
                        count: indices.len() as u32,
                        material,
                    })
                    .collect();
                ModelGpu {
                    vertices,
                    meshes,
                    materials: material::upload(device, queue, &materials, model),
                }
            })
            .collect();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("instanced authored creature shader"),
            source: wgpu::ShaderSource::Wgsl(
                crate::render::daylight::shader(include_str!("../authored.wgsl")).into(),
            ),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("instanced authored creature layout"),
            bind_group_layouts: &[Some(camera), Some(&layout), Some(&materials)],
            immediate_size: 0,
        });
        let vertex_attributes = wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x3,2=>Float32x2,3=>Uint32x4,4=>Float32x4,5=>Uint32];
        let instance_attributes = wgpu::vertex_attr_array![6=>Float32x3,7=>Float32x2,8=>Uint8x4,9=>Uint8x4,10=>Uint8x4,11=>Float32x3,12=>Uint32x2,13=>Float32x3];
        let buffers = [
            Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &vertex_attributes,
            }),
            Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Instance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &instance_attributes,
            }),
        ];
        let (pipeline, shadow) = super::super::pipeline::pair_with_cull(
            device,
            &shader,
            &pipeline_layout,
            format,
            &buffers,
            (true, Some(wgpu::Face::Back)),
        );
        let (double_sided, shadow_double_sided) = super::super::pipeline::pair_with_cull(
            device,
            &shader,
            &pipeline_layout,
            format,
            &buffers,
            (true, None),
        );
        Self {
            pipeline,
            shadow,
            double_sided,
            shadow_double_sided,
            group,
            joints,
            parts,
            instances,
            models,
        }
    }
    pub fn set(
        &self,
        queue: &wgpu::Queue,
        instances: &[Instance],
        joints: &[[f32; 16]],
        parts: &[Part],
    ) {
        if !instances.is_empty() {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(instances));
            queue.write_buffer(&self.joints, 0, bytemuck::cast_slice(joints));
            queue.write_buffer(&self.parts, 0, bytemuck::cast_slice(parts));
        }
    }
    pub fn draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        camera: &wgpu::BindGroup,
        ranges: &[std::ops::Range<u32>],
        shadow: bool,
    ) -> usize {
        let mut triangles = 0;
        for (model, range) in self
            .models
            .iter()
            .zip(ranges)
            .filter(|(_, r)| !r.is_empty())
        {
            pass.set_bind_group(0, camera, &[]);
            pass.set_bind_group(1, &self.group, &[]);
            pass.set_vertex_buffer(0, model.vertices.slice(..));
            pass.set_vertex_buffer(1, self.instances.slice(..));
            for mesh in &model.meshes {
                let material = &model.materials[mesh.material];
                pass.set_pipeline(match (shadow, material.double_sided) {
                    (false, false) => &self.pipeline,
                    (false, true) => &self.double_sided,
                    (true, false) => &self.shadow,
                    (true, true) => &self.shadow_double_sided,
                });
                pass.set_bind_group(2, &material.group, &[]);
                pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.count, 0, range.clone());
                triangles += mesh.count as usize * range.len() / 3;
            }
        }
        triangles
    }
}
