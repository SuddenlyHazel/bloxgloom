//! Linear HDR scene, quarter-resolution bloom, and fixed-exposure display mapping.
//! Shared by the window renderer, image previews, and GPU benchmark.
use wgpu::util::DeviceExt;

mod targets;

#[cfg(test)]
mod tests;

pub(crate) const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

pub(crate) struct PostProcess {
    pub scene: wgpu::TextureView,
    bloom: [wgpu::TextureView; 2],
    groups: [wgpu::BindGroup; 3],
    composite_group: wgpu::BindGroup,
    extract: wgpu::RenderPipeline,
    horizontal: wgpu::RenderPipeline,
    vertical: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    settings: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    output_srgb: bool,
    exposure: f32,
    enabled: bool,
    bloom_strength: f32,
}

impl PostProcess {
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        output: wgpu::TextureFormat,
    ) -> Self {
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
            label: Some("post processing inputs"),
            entries: &[
                texture_entry(0),
                texture_entry(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
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
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("post linear clamp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let settings = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("exposure and bloom"),
            contents: bytemuck::cast_slice(&[
                1.0f32,
                0.12,
                if output.is_srgb() { 0.0 } else { 1.0 },
                1.0,
            ]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let targets = targets::Targets::new(device, width, height, &layout, &sampler, &settings);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("HDR and bloom shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("post.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("post pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry, format| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
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
            scene: targets.scene,
            bloom: targets.bloom,
            groups: targets.groups,
            extract: pipeline("extract", HDR_FORMAT),
            horizontal: pipeline("horizontal", HDR_FORMAT),
            vertical: pipeline("vertical", HDR_FORMAT),
            composite: pipeline("composite", output),
            settings,
            layout,
            sampler,
            output_srgb: output.is_srgb(),
            exposure: 1.0,
            enabled: true,
            bloom_strength: 0.12,
            composite_group: targets.composite_group,
        }
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let targets = targets::Targets::new(
            device,
            width,
            height,
            &self.layout,
            &self.sampler,
            &self.settings,
        );
        self.scene = targets.scene;
        self.bloom = targets.bloom;
        self.groups = targets.groups;
        self.composite_group = targets.composite_group;
    }

    pub fn configure(
        &mut self,
        queue: &wgpu::Queue,
        enabled: bool,
        exposure: f32,
        bloom_strength: f32,
    ) {
        if self.enabled == enabled
            && self.exposure == exposure
            && self.bloom_strength == bloom_strength
        {
            return;
        }
        self.exposure = exposure;
        self.enabled = enabled;
        self.bloom_strength = bloom_strength;
        queue.write_buffer(
            &self.settings,
            0,
            bytemuck::cast_slice(&[
                exposure,
                bloom_strength,
                if self.output_srgb { 0.0 } else { 1.0 },
                if enabled { 1.0 } else { 0.0 },
            ]),
        );
    }

    pub fn encode(&self, encoder: &mut wgpu::CommandEncoder, output: &wgpu::TextureView) {
        let mut draw = |label,
                        pipeline: &wgpu::RenderPipeline,
                        group: &wgpu::BindGroup,
                        target: &wgpu::TextureView| {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(label),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.draw(0..3, 0..1);
        };
        if self.enabled && self.bloom_strength > 0.0 {
            draw(
                "bloom extract",
                &self.extract,
                &self.groups[0],
                &self.bloom[0],
            );
            draw(
                "bloom horizontal",
                &self.horizontal,
                &self.groups[1],
                &self.bloom[1],
            );
            draw(
                "bloom vertical",
                &self.vertical,
                &self.groups[2],
                &self.bloom[0],
            );
        }
        draw(
            "HDR display mapping",
            &self.composite,
            &self.composite_group,
            output,
        );
    }
}
