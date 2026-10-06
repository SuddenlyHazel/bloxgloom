//! Bounded, half-resolution screen-space specular replacement.
//! Misses keep the material's GGX sky/local-light fallback. The receiving
//! surface depth is separate from opaque intersection depth, so water can
//! reflect opaque banks without making transparent water an opaque occluder.
use glam::{Mat4, Vec3};

mod targets;
#[cfg(test)]
mod tests;

pub(crate) struct Reflections {
    pub normal: wgpu::TextureView,
    pub response: wgpu::TextureView,
    targets: targets::Targets,
    gpu: Option<Gpu>,
    eye: Vec3,
    atmosphere: super::daylight::Atmosphere,
    enabled: bool,
    artistic: bool,
}
struct Gpu {
    layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    sampler: wgpu::Sampler,
    trace: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    downsample_layout: wgpu::BindGroupLayout,
    downsample: wgpu::RenderPipeline,
    capture_layout: wgpu::BindGroupLayout,
    capture: wgpu::RenderPipeline,
}
impl Reflections {
    pub(crate) fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let targets = targets::Targets::new(device, width, height);
        Self {
            normal: targets.normal.clone(),
            response: targets.response.clone(),
            targets,
            gpu: super::post::temporal::supported(device).then(|| Gpu::new(device)),
            eye: Vec3::ZERO,
            atmosphere: super::daylight::Atmosphere::at(6000),
            artistic: super::bsl_reference::advanced_materials(),
            enabled: std::env::var("BLOXGLOOM_REFLECTIONS")
                .map_or(true, |value| value.trim() != "0"),
        }
    }
    pub(crate) fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.targets = targets::Targets::new(device, width, height);
        self.normal = self.targets.normal.clone();
        self.response = self.targets.response.clone();
    }
    pub(crate) fn configure(&mut self, eye: Vec3, atmosphere: super::daylight::Atmosphere) {
        self.eye = eye;
        self.atmosphere = atmosphere;
    }
    pub(crate) fn capture_opaque(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
        depth: &wgpu::TextureView,
    ) {
        let Some(gpu) = &self.gpu else {
            return;
        };
        if !self.enabled {
            return;
        }
        let capture_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("opaque geometry radiance capture"),
            layout: &gpu.capture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(scene),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
            ],
        });
        draw(
            encoder,
            "capture geometry reflection radiance",
            &gpu.capture,
            &capture_group,
            &self.targets.levels[0],
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
        );
        for levels in self.targets.levels.windows(2) {
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("reflection radiance downsample"),
                layout: &gpu.downsample_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&levels[0]),
                }],
            });
            draw(
                encoder,
                "reflection HDR mip",
                &gpu.downsample,
                &group,
                &levels[1],
                wgpu::LoadOp::Clear(wgpu::Color::BLACK),
            );
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn resolve(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
        indirect: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        matrix: Mat4,
    ) {
        let Some(gpu) = &self.gpu else {
            return;
        };
        if !self.enabled {
            return;
        }
        let size = scene.texture().size();
        let mut data = [0.0f32; 56];
        data[..16].copy_from_slice(&matrix.inverse().to_cols_array());
        data[16..32].copy_from_slice(&matrix.to_cols_array());
        data[32..35].copy_from_slice(&self.eye.to_array());
        data[36..39].copy_from_slice(&self.atmosphere.horizon.to_array());
        data[40..43].copy_from_slice(&self.atmosphere.zenith.to_array());
        // Match the actual surface camera packing, including the reference
        // moon-strength alias used by the independent water fallback.
        data[43] = self.atmosphere.camera_data(Mat4::IDENTITY, self.eye)[43];
        data[44..46].copy_from_slice(&[size.width as f32, size.height as f32]);
        data[48..51].copy_from_slice(&self.atmosphere.sun.to_array());
        data[51] = self.atmosphere.time_brightness();
        data[52] = self.atmosphere.rain_strength;
        data[53] = self.atmosphere.moon_multiplier();
        data[54] = f32::from(self.artistic);
        queue.write_buffer(&gpu.uniform, 0, bytemuck::cast_slice(&data));
        let group = |replacement: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("scene reflection inputs"),
                layout: &gpu.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(depth),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&self.normal),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&self.response),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&self.targets.pyramid_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(replacement),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: gpu.uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::Sampler(&gpu.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 7,
                        resource: wgpu::BindingResource::TextureView(indirect),
                    },
                ],
            })
        };
        let trace_group = group(&self.response);
        let composite_group = group(&self.targets.delta);
        draw(
            encoder,
            "bounded scene reflection trace",
            &gpu.trace,
            &trace_group,
            &self.targets.delta,
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
        );
        draw(
            encoder,
            "replace confident sky reflections",
            &gpu.composite,
            &composite_group,
            scene,
            wgpu::LoadOp::Load,
        );
    }
}
fn draw(
    encoder: &mut wgpu::CommandEncoder,
    label: &str,
    pipeline: &wgpu::RenderPipeline,
    group: &wgpu::BindGroup,
    target: &wgpu::TextureView,
    load: wgpu::LoadOp<wgpu::Color>,
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
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
    pass.set_bind_group(0, group, &[]);
    pass.draw(0..3, 0..1);
}
impl Gpu {
    fn new(device: &wgpu::Device) -> Self {
        let texture = |sample_type| wgpu::BindingType::Texture {
            sample_type,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        };
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty,
            count: None,
        };
        let float = wgpu::TextureSampleType::Float { filterable: true };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene reflection layout"),
            entries: &[
                entry(0, texture(wgpu::TextureSampleType::Depth)),
                entry(1, texture(float)),
                entry(2, texture(float)),
                entry(3, texture(float)),
                entry(4, texture(float)),
                entry(
                    5,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                entry(
                    6,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
                entry(7, texture(float)),
            ],
        });
        let downsample_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("reflection radiance mip layout"),
            entries: &[entry(0, texture(float))],
        });
        let capture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("opaque reflection capture layout"),
            entries: &[
                entry(0, texture(float)),
                entry(1, texture(wgpu::TextureSampleType::Depth)),
            ],
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("reflection projection and sky"),
            size: 224,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("reflection trilinear clamp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bounded scene reflections"),
            source: wgpu::ShaderSource::Wgsl(shader_source().into()),
        });
        let mip_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("linear HDR reflection mips"),
            source: wgpu::ShaderSource::Wgsl(include_str!("reflections/downsample.wgsl").into()),
        });
        let pipeline =
            |label, shader: &wgpu::ShaderModule, layout: &wgpu::BindGroupLayout, blend| {
                let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some(label),
                    bind_group_layouts: &[Some(layout)],
                    immediate_size: 0,
                });
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(&layout),
                    vertex: wgpu::VertexState {
                        module: shader,
                        entry_point: Some("vs_main"),
                        compilation_options: Default::default(),
                        buffers: &[],
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: shader,
                        entry_point: Some(label),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: super::post::HDR_FORMAT,
                            blend,
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    primitive: Default::default(),
                    depth_stencil: None,
                    multisample: Default::default(),
                    multiview_mask: None,
                    cache: None,
                })
            };
        let additive = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };
        Self {
            trace: pipeline("trace", &shader, &layout, None),
            composite: pipeline("composite", &shader, &layout, Some(additive)),
            downsample: pipeline("downsample", &mip_shader, &downsample_layout, None),
            capture: pipeline("capture", &mip_shader, &capture_layout, None),
            capture_layout,
            layout,
            downsample_layout,
            uniform,
            sampler,
        }
    }
}
fn shader_source() -> String {
    format!(
        "{}\n{}\n{}\n{}\n{}",
        super::sky::STYLE_SHADER,
        super::bsl_reference::REFLECTION_SHADER,
        include_str!("material/pbr.wgsl"),
        include_str!("reflections/normal.wgsl"),
        include_str!("reflections.wgsl")
    )
}
