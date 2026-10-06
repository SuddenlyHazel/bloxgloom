//! Quarter-resolution volume integration; full-resolution sky reconstructs RGB/T.
pub(super) struct CloudPass {
    pub(super) view: wgpu::TextureView,
    size: (u32, u32),
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    noise: super::reference_clouds::Noise,
}
impl CloudPass {
    pub(super) fn new(
        device: &wgpu::Device,
        camera: &wgpu::Buffer,
        width: u32,
        height: u32,
    ) -> Self {
        let noise = super::reference_clouds::Noise::new(device);
        let size = Self::dimensions(width, height, noise.available);
        let view = Self::target(device, size);
        let bind = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("volume camera layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("volume camera"),
            layout: &bind,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&noise.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&noise.sampler),
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("volume cloud pipeline layout"),
            bind_group_layouts: &[Some(&bind)],
            immediate_size: 0,
        });
        let source = Self::shader_for(noise.available);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("true volume cloud integration"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("quarter resolution volume clouds"),
            layout: Some(&layout),
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
                entry_point: Some("clouds"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: super::super::post::HDR_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            view,
            size,
            pipeline,
            group,
            noise,
        }
    }
    #[cfg(test)]
    pub(super) fn shader() -> String {
        Self::shader_for(false)
    }
    fn shader_for(available: bool) -> String {
        format!(
            "const BG_REFERENCE_CLOUD_NOISE: bool = {available};\n{}\n{}\n{}\n{}\n{}",
            super::STYLE_SHADER,
            super::CLOUD_SHADER,
            include_str!("camera.wgsl"),
            super::reference_clouds::SHADER,
            include_str!("cloud_pass.wgsl")
        )
    }
    fn dimensions(width: u32, height: u32, reference: bool) -> (u32, u32) {
        if reference {
            (width.max(1), height.max(1))
        } else {
            (width.max(1).div_ceil(4), height.max(1).div_ceil(4))
        }
    }
    fn target(device: &wgpu::Device, size: (u32, u32)) -> wgpu::TextureView {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("volume clouds radiance and transmittance"),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: super::super::post::HDR_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | if cfg!(test) {
                        wgpu::TextureUsages::COPY_SRC
                    } else {
                        wgpu::TextureUsages::empty()
                    },
                view_formats: &[],
            })
            .create_view(&Default::default())
    }
    pub(super) fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) -> bool {
        let size = Self::dimensions(width, height, self.noise.available);
        if size == self.size {
            return false;
        }
        self.view = Self::target(device, size);
        self.size = size;
        true
    }
    pub(super) fn encode(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        timestamp_writes: Option<wgpu::RenderPassTimestampWrites<'_>>,
        integrate: bool,
    ) {
        self.noise.upload(encoder);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("volume cloud density and solar integration"),
            timestamp_writes,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.,
                        g: 0.,
                        b: 0.,
                        a: 1.,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        if !integrate {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.draw(0..3, 0..1);
    }
}
