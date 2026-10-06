use super::*;
pub(super) struct Gpu {
    pub noise: crate::render::sky::ReferenceNoise,
    pub options: wgpu::Buffer,
    pub camera: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    sample: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
}
pub(super) fn target(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("source RGB8 ambient visibility"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
        .create_view(&Default::default())
}
impl Gpu {
    pub fn new(device: &wgpu::Device) -> Self {
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty,
            count: None,
        };
        let tex = |sample_type| wgpu::BindingType::Texture {
            sample_type,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        };
        let uniform = wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("source AO inputs"),
            entries: &[
                entry(0, tex(wgpu::TextureSampleType::Depth)),
                entry(1, tex(wgpu::TextureSampleType::Float { filterable: true })),
                entry(2, tex(wgpu::TextureSampleType::Float { filterable: true })),
                entry(
                    3,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
                entry(4, uniform),
                entry(5, uniform),
                entry(
                    6,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
            ],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("source default AO"),
            source: wgpu::ShaderSource::Wgsl(shader().into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry, format, blend| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vertex"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::SrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let buffer = |label, size| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        Self {
            noise: crate::render::sky::ReferenceNoise::new(device),
            options: buffer("source AO projection", 96),
            camera: buffer("source AO fog camera", 320),
            sample: pipeline("sample_ao", wgpu::TextureFormat::Rgba8Unorm, None),
            composite: pipeline("composite_ao", super::super::HDR_FORMAT, Some(blend)),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("source AO clamp filter"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            layout,
        }
    }
    pub fn draw(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        visibility: &wgpu::TextureView,
    ) {
        let group = |source| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("source AO frame"),
                layout: &self.layout,
                entries: &[
                    super::super::reference_display::texture_entry(0, depth),
                    super::super::reference_display::texture_entry(1, source),
                    super::super::reference_display::texture_entry(2, &self.noise.view),
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: self.options.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: self.camera.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::Sampler(&self.noise.sampler),
                    },
                ],
            })
        };
        for (pipeline, group, target, load) in [
            (
                &self.sample,
                group(&self.noise.view),
                visibility,
                wgpu::LoadOp::Clear(wgpu::Color::WHITE),
            ),
            (
                &self.composite,
                group(visibility),
                scene,
                wgpu::LoadOp::Load,
            ),
        ] {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("source default AO"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}
