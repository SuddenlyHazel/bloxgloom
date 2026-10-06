//! Half-resolution, depth-aware single scattering from the existing sun map.
//! Unknown/occluded volumes contribute zero, preserving sealed cave darkness.
use glam::{Mat4, Vec3};

#[cfg(test)]
mod tests;

pub(crate) struct AtmospherePass {
    scattering: wgpu::TextureView,
    distance: wgpu::TextureView,
    gpu: Option<Gpu>,
    density: f32,
}
struct Gpu {
    layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    sampler: wgpu::Sampler,
    integrate: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
}
impl AtmospherePass {
    pub(crate) fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let density = std::env::var("BLOXGLOOM_ATMOSPHERE")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|v| v.is_finite())
            .unwrap_or(1.0)
            .clamp(0.0, 3.0)
            * 0.0018;
        Self {
            scattering: target(device, width, height, super::post::HDR_FORMAT),
            distance: target(device, width, height, wgpu::TextureFormat::R32Float),
            gpu: super::post::temporal::supported(device).then(|| Gpu::new(device)),
            density,
        }
    }
    pub(crate) fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.scattering = target(device, width, height, super::post::HDR_FORMAT);
        self.distance = target(device, width, height, wgpu::TextureFormat::R32Float);
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn resolve(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        surface: &wgpu::TextureView,
        matrix: Mat4,
        eye: Vec3,
        atmosphere: super::daylight::Atmosphere,
        shadows: &super::sun_shadow::SunShadows,
    ) {
        let Some(gpu) = &self.gpu else {
            return;
        };
        // No directional map is rendered at night or when shadows are off.
        // Never sample its stale contents or substitute an unoccluded sun.
        if self.density <= 0.0 || !shadows.projection.enabled {
            return;
        }
        let mut data = [0.0f32; 52];
        data[..16].copy_from_slice(&matrix.inverse().to_cols_array());
        data[16..32].copy_from_slice(&shadows.projection.matrix.to_cols_array());
        data[32..35].copy_from_slice(&eye.to_array());
        data[35] = atmosphere.fog_exposure.clamp(0.0, 1.0);
        data[36..39].copy_from_slice(&atmosphere.sun.normalize_or_zero().to_array());
        data[40..43].copy_from_slice(&atmosphere.sun_radiance().to_array());
        data[44..47].copy_from_slice(&atmosphere.ambient().1.to_array());
        data[48] = self.density * (1.0 - atmosphere.fog.clamp(0.0, 1.0));
        data[49] = shadows.projection.settings.distance.min(64.0);
        queue.write_buffer(&gpu.uniform, 0, bytemuck::cast_slice(&data));
        let group = |volume, distance| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("shadowed atmosphere inputs"),
                layout: &gpu.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(depth),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&shadows.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&gpu.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: gpu.uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(volume),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::TextureView(distance),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::TextureView(surface),
                    },
                ],
            })
        };
        // Unused integration bindings must not alias its render attachments.
        let integration_group = group(scene, scene);
        let composite_group = group(&self.scattering, &self.distance);
        let attachment = |view| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })
        };
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("half-resolution shadowed air"),
                color_attachments: &[attachment(&self.scattering), attachment(&self.distance)],
                ..Default::default()
            });
            pass.set_pipeline(&gpu.integrate);
            pass.set_bind_group(0, &integration_group, &[]);
            pass.draw(0..3, 0..1);
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("depth-aware atmosphere composite"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: scene,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&gpu.composite);
            pass.set_bind_group(0, &composite_group, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}
fn target(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("half-resolution atmosphere"),
            size: wgpu::Extent3d {
                width: width.div_ceil(2).max(1),
                height: height.div_ceil(2).max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
        .create_view(&Default::default())
}
impl Gpu {
    fn new(device: &wgpu::Device) -> Self {
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty,
            count: None,
        };
        let texture = |sample_type| wgpu::BindingType::Texture {
            sample_type,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("atmosphere layout"),
            entries: &[
                entry(0, texture(wgpu::TextureSampleType::Depth)),
                entry(1, texture(wgpu::TextureSampleType::Depth)),
                entry(
                    2,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                ),
                entry(
                    3,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                entry(
                    4,
                    texture(wgpu::TextureSampleType::Float { filterable: false }),
                ),
                entry(
                    5,
                    texture(wgpu::TextureSampleType::Float { filterable: false }),
                ),
                entry(
                    6,
                    texture(wgpu::TextureSampleType::Float { filterable: false }),
                ),
            ],
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("atmosphere projection"),
            size: 208,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("atmosphere sun visibility"),
            compare: Some(wgpu::CompareFunction::LessEqual),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shadow-aware air scattering"),
            source: wgpu::ShaderSource::Wgsl(include_str!("atmosphere.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("atmosphere"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry, targets: &[Option<wgpu::ColorTargetState>]| {
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
                    targets,
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let target = |format, blend| {
            Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })
        };
        let integrate = pipeline(
            "integrate",
            &[
                target(super::post::HDR_FORMAT, None),
                target(wgpu::TextureFormat::R32Float, None),
            ],
        );
        let blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let composite = pipeline("composite", &[target(super::post::HDR_FORMAT, Some(blend))]);
        Self {
            layout,
            uniform,
            sampler,
            integrate,
            composite,
        }
    }
}
