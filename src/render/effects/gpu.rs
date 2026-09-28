use super::{Prepared, Stage};
use wgpu::util::DeviceExt;

pub(crate) struct Effect {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    time: wgpu::Buffer,
    started: std::time::Instant,
}

impl Effect {
    /// GPU compilation is also off the window thread. No partial pipeline is
    /// installed on a validation failure; the error retains package ownership.
    pub fn prepare(device: &wgpu::Device, prepared: &Prepared) -> Result<Self, String> {
        std::thread::scope(|scope| {
            scope
                .spawn(|| Self::compile(device, prepared))
                .join()
                .map_err(|_| format!("{}: GPU preparation worker panicked", prepared.owner))?
        })
    }

    fn compile(device: &wgpu::Device, prepared: &Prepared) -> Result<Self, String> {
        let Stage::SceneColor = prepared.descriptor.stage;
        let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(&prepared.owner),
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
                        min_binding_size: wgpu::BufferSize::new(16),
                    },
                    count: None,
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(&prepared.owner),
            source: wgpu::ShaderSource::Wgsl(prepared.source.as_str().into()),
        });
        let vertex = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("renderer-owned effect triangle"), source: wgpu::ShaderSource::Wgsl(
                "@vertex fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f { let p = array<vec2f,3>(vec2f(-1.,-1.),vec2f(3.,-1.),vec2f(-1.,3.)); return vec4f(p[i],0.,1.); }".into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(&prepared.owner),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&prepared.owner),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vertex,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: crate::render::post::HDR_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        if let Some(error) = pollster::block_on(error_scope.pop()) {
            return Err(format!("{}: shader pipeline: {error}", prepared.owner));
        }
        Ok(Self {
            pipeline,
            layout,
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            time: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("effect time/size"),
                contents: bytemuck::cast_slice(&[0f32; 4]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }),
            started: std::time::Instant::now(),
        })
    }

    pub fn encode(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::TextureView,
        target: &wgpu::TextureView,
    ) {
        let size = source.texture().size();
        queue.write_buffer(
            &self.time,
            0,
            bytemuck::cast_slice(&[
                self.started.elapsed().as_secs_f32(),
                0.,
                size.width as f32,
                size.height as f32,
            ]),
        );
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("effect scene inputs"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.time.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("package scene-color effect"),
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
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }
}
