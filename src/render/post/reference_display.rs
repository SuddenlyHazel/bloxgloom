//! Optional BSL display-stage comparison: gamma/grain, FXAA, display-space TAA.
//! Enhanced HDR temporal accumulation and post processing remain independent.
use super::{HDR_FORMAT, temporal::Temporal};
use wgpu::util::DeviceExt;
mod targets;
#[cfg(test)]
mod tests;
const STAGES: &str = include_str!("reference_display/stages.wgsl");
const FXAA: &str = include_str!("reference_display/fxaa.wgsl");
const TAA: &str = include_str!("reference_display/taa.wgsl");

pub(super) struct ReferenceDisplay {
    pub linear: wgpu::TextureView,
    gamma: wgpu::TextureView,
    fxaa: wgpu::TextureView,
    colors: [wgpu::TextureView; 2],
    depths: [wgpu::TextureView; 2],
    noise: crate::render::sky::ReferenceNoise,
    layout: wgpu::BindGroupLayout,
    temporal_layout: wgpu::BindGroupLayout,
    gamma_pipeline: wgpu::RenderPipeline,
    fxaa_pipeline: wgpu::RenderPipeline,
    temporal_pipeline: wgpu::RenderPipeline,
    present_pipeline: wgpu::RenderPipeline,
    options: wgpu::Buffer,
    sampler: wgpu::Sampler,
    output_srgb: bool,
    index: usize,
    resolved: bool,
    pub depth: Option<wgpu::TextureView>,
}
impl ReferenceDisplay {
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        output: wgpu::TextureFormat,
    ) -> Self {
        let texture = |binding, filterable| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let sampler = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let uniform = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("reference display stages"),
            entries: &[
                texture(0, true),
                texture(1, true),
                sampler(2),
                sampler(3),
                uniform(4),
            ],
        });
        let temporal_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("reference display temporal"),
            entries: &[
                texture(0, true),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                texture(2, true),
                texture(3, false),
                sampler(4),
                uniform(5),
                texture(6, true),
                texture(7, true),
            ],
        });
        let stages = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("source display gamma grain FXAA"),
            source: wgpu::ShaderSource::Wgsl(format!("{STAGES}\n{FXAA}").into()),
        });
        let taa = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("source display Catmull-Rom temporal"),
            source: wgpu::ShaderSource::Wgsl(TAA.into()),
        });
        let targets = targets::Targets::new(device, width, height);
        Self {
            linear: targets.linear,
            gamma: targets.gamma,
            fxaa: targets.fxaa,
            colors: targets.colors,
            depths: targets.depths,
            gamma_pipeline: pipeline(
                device,
                &stages,
                &layout,
                "gamma_grain",
                &[wgpu::TextureFormat::Rgba8Unorm],
            ),
            fxaa_pipeline: pipeline(
                device,
                &stages,
                &layout,
                "antialias",
                &[wgpu::TextureFormat::Rgba8Unorm],
            ),
            temporal_pipeline: pipeline(
                device,
                &taa,
                &temporal_layout,
                "resolve",
                &[
                    wgpu::TextureFormat::Rgba8Unorm,
                    wgpu::TextureFormat::R32Float,
                ],
            ),
            present_pipeline: pipeline(device, &stages, &layout, "present", &[output]),
            noise: crate::render::sky::ReferenceNoise::new(device),
            layout,
            temporal_layout,
            options: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("reference display flags"),
                contents: bytemuck::cast_slice(&[0.0f32; 4]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("reference display clamp linear"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            output_srgb: output.is_srgb(),
            index: 0,
            resolved: false,
            depth: None,
        }
    }
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let t = targets::Targets::new(device, width, height);
        self.linear = t.linear;
        self.gamma = t.gamma;
        self.fxaa = t.fxaa;
        self.colors = t.colors;
        self.depths = t.depths;
        self.reset();
    }
    pub fn reset(&mut self) {
        self.index = 0;
        self.resolved = false;
        self.depth = None;
    }
    pub fn submitted(&mut self) {
        if self.resolved {
            self.index = 1 - self.index;
            self.resolved = false;
        }
        self.depth = None;
    }
    fn group(&self, device: &wgpu::Device, source: &wgpu::TextureView) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("reference display input"),
            layout: &self.layout,
            entries: &[
                texture_entry(0, source),
                texture_entry(1, &self.noise.view),
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.noise.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.options.as_entire_binding(),
                },
            ],
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        output: &wgpu::TextureView,
        temporal: Option<&mut Temporal>,
        reactive: &wgpu::TextureView,
        effects: bool,
    ) {
        let frame = temporal
            .as_ref()
            .and_then(|temporal| temporal.reference_frame());
        let taa = effects && self.depth.is_some() && frame.is_some();
        queue.write_buffer(
            &self.options,
            0,
            bytemuck::cast_slice(&[
                f32::from(taa),
                f32::from(effects),
                f32::from(self.output_srgb),
                f32::from(self.noise.available),
            ]),
        );
        self.noise.upload(encoder);
        draw(
            encoder,
            &self.gamma_pipeline,
            &self.group(device, &self.linear),
            &[&self.gamma],
        );
        draw(
            encoder,
            &self.fxaa_pipeline,
            &self.group(device, &self.gamma),
            &[&self.fxaa],
        );
        let source = if taa {
            let (settings, motion) = frame.unwrap();
            let target = 1 - self.index;
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("source display temporal frame"),
                layout: &self.temporal_layout,
                entries: &[
                    texture_entry(0, &self.fxaa),
                    texture_entry(1, self.depth.as_ref().unwrap()),
                    texture_entry(2, &self.colors[self.index]),
                    texture_entry(3, &self.depths[self.index]),
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: settings.as_entire_binding(),
                    },
                    texture_entry(6, motion),
                    texture_entry(7, reactive),
                ],
            });
            draw(
                encoder,
                &self.temporal_pipeline,
                &group,
                &[&self.colors[target], &self.depths[target]],
            );
            self.resolved = true;
            if let Some(temporal) = temporal {
                temporal.reference_resolved();
            }
            &self.colors[target]
        } else {
            &self.fxaa
        };
        draw(
            encoder,
            &self.present_pipeline,
            &self.group(device, source),
            &[output],
        );
    }
}
fn texture_entry(binding: u32, view: &wgpu::TextureView) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::TextureView(view),
    }
}
fn pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::BindGroupLayout,
    entry: &str,
    formats: &[wgpu::TextureFormat],
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(entry),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    let targets: Vec<_> = formats
        .iter()
        .map(|format| {
            Some(wgpu::ColorTargetState {
                format: *format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })
        })
        .collect();
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(entry),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(entry),
            compilation_options: Default::default(),
            targets: &targets,
        }),
        multiview_mask: None,
        cache: None,
    })
}
fn draw(
    encoder: &mut wgpu::CommandEncoder,
    pipeline: &wgpu::RenderPipeline,
    group: &wgpu::BindGroup,
    targets: &[&wgpu::TextureView],
) {
    let attachments: Vec<_> = targets
        .iter()
        .map(|view| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })
        })
        .collect();
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("BSL reference display"),
        color_attachments: &attachments,
        ..Default::default()
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, group, &[]);
    pass.draw(0..3, 0..1);
}
