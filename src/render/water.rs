//! Depth-tested, two-sided water surfaces; shared by gameplay and previews.
use super::{DEPTH_FORMAT, daylight, post, scene_ao};
use crate::world::{CHUNK_SIZE, ChunkKey};
use glam::Vec3;
use std::time::Instant;
use wgpu::util::DeviceExt;

pub(crate) fn distance(key: ChunkKey, eye: Vec3) -> f32 {
    let center =
        Vec3::new(key.x as f32 + 0.5, key.y as f32 + 0.5, key.z as f32 + 0.5) * CHUNK_SIZE as f32;
    center.distance_squared(eye)
}
pub(crate) struct WaterRenderer {
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    time: wgpu::Buffer,
    started: Instant,
}
impl WaterRenderer {
    pub(crate) fn new(device: &wgpu::Device, camera: &wgpu::Buffer) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("water shader"),
            source: wgpu::ShaderSource::Wgsl(
                daylight::surface_shader(include_str!("water.wgsl")).into(),
            ),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("water uniforms"),
            entries: &[0, 1].map(|binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }),
        });
        let time = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("water time"),
            contents: bytemuck::cast_slice(&[0.0f32; 4]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("water uniforms"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: time.as_entire_binding(),
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("water layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline=device.create_render_pipeline(&wgpu::RenderPipelineDescriptor{
            label:Some("water pipeline"),layout:Some(&pipeline_layout),vertex:wgpu::VertexState {module:&shader,entry_point:Some("vs"),compilation_options:Default::default(),buffers:&[Some(wgpu::VertexBufferLayout {array_stride:(super::mesh::water::FLOATS*4) as u64,step_mode:wgpu::VertexStepMode::Vertex,attributes:&wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x3,2=>Float32x4,3=>Float32x2]})]},
            fragment:Some(wgpu::FragmentState{module:&shader,entry_point:Some("fs"),compilation_options:Default::default(),targets:&scene_ao::color_targets(post::HDR_FORMAT,Some(wgpu::BlendState::ALPHA_BLENDING))}),
            primitive:wgpu::PrimitiveState{cull_mode:None,..Default::default()},depth_stencil:Some(wgpu::DepthStencilState{format:DEPTH_FORMAT,depth_write_enabled:Some(false),depth_compare:Some(wgpu::CompareFunction::LessEqual),stencil:Default::default(),bias:Default::default()}),multisample:Default::default(),multiview_mask:None,cache:None });
        Self {
            pipeline,
            group,
            time,
            started: Instant::now(),
        }
    }
    pub(crate) fn prepare(&self, queue: &wgpu::Queue) {
        queue.write_buffer(
            &self.time,
            0,
            bytemuck::cast_slice(&[
                self.started.elapsed().as_secs_f32() % (std::f32::consts::TAU * 10.0),
                0.0,
                0.0,
                0.0,
            ]),
        );
    }
    pub(crate) fn draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        vertex: &wgpu::Buffer,
        index: &wgpu::Buffer,
        indices: u32,
    ) -> usize {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.set_vertex_buffer(0, vertex.slice(..));
        pass.set_index_buffer(index.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..indices, 0, 0..1);
        indices as usize / 3
    }
}
#[cfg(test)]
mod tests;
