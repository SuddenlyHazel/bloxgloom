//! One fixed-size GPU binding for session-owned material inputs and parameters.
use super::{MAX_MATERIALS, Prepared};
use crate::render::parameters::{self, Value};
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct MaterialData {
    parameters: [[f32; 4]; parameters::MAX_PARAMETERS],
    textures: [u32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Data {
    frame: [f32; 4],
    materials: [MaterialData; MAX_MATERIALS],
}

pub(crate) struct Gpu {
    pub layout: wgpu::BindGroupLayout,
    pub group: wgpu::BindGroup,
    buffer: wgpu::Buffer,
    data: Data,
    prepared: Prepared,
    started: std::time::Instant,
}
impl Gpu {
    pub(crate) fn new(device: &wgpu::Device, prepared: &Prepared) -> Self {
        let mut data = Data::zeroed();
        for (index, material) in prepared.materials.iter().enumerate() {
            data.materials[index].parameters =
                parameters::defaults(&material.parameters).expect("verified defaults");
            data.materials[index].textures[..material.textures.len()]
                .copy_from_slice(&material.textures);
        }
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("visual material contract v2"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<Data>() as u64),
                },
                count: None,
            }],
        });
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("bounded visual material parameters"),
            contents: bytemuck::bytes_of(&data),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("visual material parameters"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        Self {
            layout,
            group,
            buffer,
            data,
            prepared: prepared.clone(),
            started: std::time::Instant::now(),
        }
    }
    pub(crate) fn set(
        &mut self,
        resource: &str,
        name: &str,
        value: &Value,
    ) -> Result<bool, String> {
        let Some((index, material)) = self
            .prepared
            .materials
            .iter()
            .enumerate()
            .find(|(_, m)| m.owner == resource)
        else {
            return Ok(false);
        };
        let (slot, definition) = material
            .parameters
            .iter()
            .enumerate()
            .find(|(_, p)| p.name == name)
            .ok_or_else(|| format!("{resource}: unknown parameter {name}"))?;
        self.data.materials[index].parameters[slot] = definition.pack(value)?;
        Ok(true)
    }
    pub(crate) fn padding(&self) -> f32 {
        self.prepared
            .materials
            .iter()
            .map(|m| m.vertex_offset)
            .fold(0.0, f32::max)
    }
    pub(crate) fn update(&mut self, queue: &wgpu::Queue) {
        self.data.frame[0] = self.started.elapsed().as_secs_f32();
        queue.write_buffer(&self.buffer, 0, bytemuck::bytes_of(&self.data));
    }
}
