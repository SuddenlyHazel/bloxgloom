//! Depth-tested, two-sided water surfaces; shared by gameplay and previews.
use super::{DEPTH_FORMAT, daylight, post, scene_ao};
use crate::world::{CHUNK_SIZE, ChunkKey};
use glam::Vec3;
use wgpu::util::DeviceExt;
pub(crate) mod reference;

pub(crate) fn distance(key: ChunkKey, eye: Vec3) -> f32 {
    let center =
        Vec3::new(key.x as f32 + 0.5, key.y as f32 + 0.5, key.z as f32 + 0.5) * CHUNK_SIZE as f32;
    center.distance_squared(eye)
}
#[derive(Clone, Copy)]
pub(crate) struct Frame {
    pub(crate) atmosphere: daylight::Atmosphere,
    pub(crate) eye_in_water: bool,
    pub(crate) sample: u32,
    pub(crate) camera: super::Camera,
    pub(crate) size: [u32; 2],
    pub(crate) view_projection: glam::Mat4,
}
pub(crate) fn depth_state(reference: bool) -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: DEPTH_FORMAT,
        depth_write_enabled: Some(reference),
        depth_compare: Some(wgpu::CompareFunction::LessEqual),
        stencil: Default::default(),
        bias: Default::default(),
    }
}
pub(crate) struct WaterRenderer {
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    camera_group: wgpu::BindGroup,
    time: wgpu::Buffer,
    reference: Option<reference::Reference>,
}
impl WaterRenderer {
    pub(crate) fn new(device: &wgpu::Device, camera: &wgpu::Buffer) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("water shader"),
            source: wgpu::ShaderSource::Wgsl(shader(include_str!("water.wgsl")).into()),
        });
        let reference = super::bsl_reference::enabled().then(|| reference::Reference::new(device));
        let mut entries = vec![wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }];
        if reference.is_some() {
            entries.extend(reference::Inputs::layout_entries(1));
        }
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("water uniforms"),
            entries: &entries,
        });
        let time = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("water time"),
            contents: bytemuck::cast_slice(&[0.0f32; 4]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let mut resources = vec![wgpu::BindGroupEntry {
            binding: 0,
            resource: time.as_entire_binding(),
        }];
        if let Some(reference) = &reference {
            resources.extend(reference.inputs.entries(1));
        }
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("water uniforms"),
            layout: &layout,
            entries: &resources,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("water layout"),
            bind_group_layouts: &[
                Some(&super::sun_shadow::camera_layout(device)),
                Some(&layout),
            ],
            immediate_size: 0,
        });
        let pipeline=device.create_render_pipeline(&wgpu::RenderPipelineDescriptor{
            label:Some("water pipeline"),layout:Some(&pipeline_layout),vertex:wgpu::VertexState {module:&shader,entry_point:Some("vs"),compilation_options:Default::default(),buffers:&[Some(wgpu::VertexBufferLayout {array_stride:(super::mesh::water::FLOATS*4) as u64,step_mode:wgpu::VertexStepMode::Vertex,attributes:&wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x3,2=>Float32x4,3=>Float32x2]})]},
            fragment:Some(wgpu::FragmentState{module:&shader,entry_point:Some("fs"),compilation_options:Default::default(),targets:&scene_ao::color_targets(post::HDR_FORMAT,Some(wgpu::BlendState::ALPHA_BLENDING))}),
            primitive:wgpu::PrimitiveState{cull_mode:None,..Default::default()},depth_stencil:Some(depth_state(reference.is_some())),multisample:Default::default(),multiview_mask:None,cache:None });
        Self {
            pipeline,
            group,
            camera_group: super::sun_shadow::fallback_camera_group(device, camera),
            time,
            reference,
        }
    }
    pub(crate) fn reference_inputs(&self) -> Option<&reference::Inputs> {
        self.reference.as_ref().map(|reference| &reference.inputs)
    }
    pub(crate) fn begin_frame(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
        opaque_depth: &wgpu::TextureView,
    ) -> wgpu::TextureView {
        let Some(reference) = self.reference.as_mut() else {
            return scene.clone();
        };
        let target = reference.begin(device, encoder, scene, opaque_depth);
        let resources = [wgpu::BindGroupEntry {
            binding: 0,
            resource: self.time.as_entire_binding(),
        }];
        let mut entries = resources.to_vec();
        entries.extend(reference.inputs.entries(1));
        self.group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("source water opaque snapshot inputs"),
            layout: &self.pipeline.get_bind_group_layout(1),
            entries: &entries,
        });
        target
    }
    pub(crate) fn reference_front_depth(&self) -> Option<&wgpu::TextureView> {
        self.reference
            .as_ref()
            .and_then(reference::Reference::front_depth)
    }
    pub(crate) fn finish_frame(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
    ) {
        if let Some(reference) = &self.reference {
            reference.finish(device, encoder, scene);
        }
    }
    #[cfg(test)]
    pub(crate) fn prepare_frame(&self, queue: &wgpu::Queue, frame: Frame) {
        self.prepare_frame_at(queue, frame, time());
    }
    /// The frame owner captures the wave clock once for near, LOD and ray water.
    pub(crate) fn prepare_frame_at(&self, queue: &wgpu::Queue, frame: Frame, water_time: f32) {
        self.prepare_at(queue, water_time);
        if let Some(reference) = &self.reference {
            reference.inputs.prepare(queue, frame);
        }
    }
    pub(crate) fn set_camera_group(&mut self, group: wgpu::BindGroup) {
        self.camera_group = group;
    }
    fn prepare_at(&self, queue: &wgpu::Queue, water_time: f32) {
        queue.write_buffer(
            &self.time,
            0,
            bytemuck::cast_slice(&[water_time, 0.0, 0.0, 0.0]),
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
        pass.set_bind_group(0, &self.camera_group, &[]);
        pass.set_bind_group(1, &self.group, &[]);
        pass.set_vertex_buffer(0, vertex.slice(..));
        pass.set_index_buffer(index.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..indices, 0, 0..1);
        indices as usize / 3
    }
}
pub(crate) fn time() -> f32 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    (START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_secs_f64()
        % (std::f64::consts::TAU * 10.0)) as f32
}
pub(super) fn reference_source(source: &str, lod: bool) -> String {
    reference_source_for(source, lod, super::bsl_reference::enabled())
}
pub(super) fn reference_source_for(source: &str, lod: bool, reference: bool) -> String {
    reference::source(source, lod, reference)
}
pub(super) fn shader(source: &str) -> String {
    let source = reference_source(source, false);
    daylight::surface_shader(&format!(
        "{}\n{}\n{}\n{}\n{}\n{source}",
        include_str!("material/pbr.wgsl"),
        super::sun_shadow::SHADER,
        super::sun_shadow::reference_shader(),
        include_str!("water/waves.wgsl"),
        include_str!("water_surface.wgsl")
    ))
}
pub(super) fn lod_shader(source: &str) -> String {
    let source = reference_source(source, true);
    daylight::surface_shader(&format!(
        "{}\n{}\n{}\n{}\n{}\n{source}",
        include_str!("material/pbr.wgsl"),
        super::sun_shadow::SHADER.replace("@group(0)", "@group(3)"),
        super::sun_shadow::reference_shader(),
        include_str!("water/waves.wgsl"),
        include_str!("water_surface.wgsl")
    ))
}
#[cfg(test)]
mod tests;
