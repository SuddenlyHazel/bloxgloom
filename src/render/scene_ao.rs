//! Scene-wide, world-radius depth AO. Geometry exports *only* indirect energy;
//! the resolve removes its additional occlusion before temporal AA and bloom.
//! Existing local AO is combined as a union, never multiplied a second time.
use glam::Mat4;

#[cfg(test)]
mod tests;

pub(crate) const INDIRECT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// Every HDR scene geometry pipeline shares this contract. Display/UI and
/// depth-only passes retain their own attachments.
pub(crate) fn color_targets(
    format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
) -> Vec<Option<wgpu::ColorTargetState>> {
    let mut targets = vec![Some(wgpu::ColorTargetState {
        format,
        blend,
        write_mask: wgpu::ColorWrites::ALL,
    })];
    if format == super::post::HDR_FORMAT {
        targets.push(Some(wgpu::ColorTargetState {
            format: INDIRECT_FORMAT,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        }));
    }
    targets
}

pub(crate) fn attachments<'a>(
    scene: &'a wgpu::TextureView,
    indirect: &'a wgpu::TextureView,
    clear: wgpu::Color,
) -> [Option<wgpu::RenderPassColorAttachment<'a>>; 2] {
    let attachment = |view, color| {
        Some(wgpu::RenderPassColorAttachment {
            view,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(color),
                store: wgpu::StoreOp::Store,
            },
        })
    };
    [
        attachment(scene, clear),
        attachment(indirect, wgpu::Color::TRANSPARENT),
    ]
}

pub(crate) fn create_indirect(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    texture(
        device,
        width,
        height,
        INDIRECT_FORMAT,
        "unoccluded indirect lighting",
    )
}

fn texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    label: &str,
) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
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

#[derive(Clone, Copy, Debug)]
pub(crate) struct Settings {
    /// Metres/blocks, independent of viewport size and field of view.
    pub radius: f32,
    pub strength: f32,
    /// A cosine bias rejects quantization/planar self-occlusion.
    pub bias: f32,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            radius: 1.5,
            strength: 0.75,
            bias: 0.08,
        }
    }
}
impl Settings {
    pub(crate) fn from_environment() -> Self {
        let mut settings = Self::default();
        let finite = |name: &str| {
            std::env::var(name)
                .ok()
                .and_then(|v| v.parse::<f32>().ok())
                .filter(|v| v.is_finite())
        };
        if let Some(radius) = finite("BLOXGLOOM_AO_RADIUS") {
            settings.radius = radius.clamp(0.1, 5.0);
        }
        if let Some(strength) = finite("BLOXGLOOM_AO") {
            settings.strength = strength.clamp(0.0, 1.0);
        }
        settings
    }
}

pub(crate) struct AmbientOcclusion {
    pub indirect: wgpu::TextureView,
    visibility: wgpu::TextureView,
    gpu: Option<Gpu>,
    pub settings: Settings,
}
struct Gpu {
    layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    horizon: wgpu::RenderPipeline,
    subtract: wgpu::RenderPipeline,
}
impl AmbientOcclusion {
    pub(crate) fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        // Naga's GL backend cannot textureLoad depth; do not even compile that
        // shader there. The lighting split remains valid with AO disabled.
        let supported = super::post::temporal::supported(device);
        if !supported {
            eprintln!("scene AO unavailable on GL backend; indirect lighting remains unoccluded");
        }
        Self {
            indirect: create_indirect(device, width, height),
            visibility: texture(
                device,
                width,
                height,
                wgpu::TextureFormat::R16Float,
                "scene ambient visibility",
            ),
            gpu: supported.then(|| Gpu::new(device)),
            settings: Settings::from_environment(),
        }
    }
    pub(crate) fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.indirect = create_indirect(device, width, height);
        self.visibility = texture(
            device,
            width,
            height,
            wgpu::TextureFormat::R16Float,
            "scene ambient visibility",
        );
    }
    pub(crate) fn resolve(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        matrix: Mat4,
    ) {
        let Some(gpu) = &self.gpu else {
            return;
        };
        if self.settings.strength <= 0.0 {
            return;
        }
        let size = scene.texture().size();
        let mut uniform = [0.0f32; 40];
        uniform[..16].copy_from_slice(&matrix.inverse().to_cols_array());
        uniform[16..32].copy_from_slice(&matrix.to_cols_array());
        uniform[32..36].copy_from_slice(&[
            size.width as f32,
            size.height as f32,
            self.settings.radius,
            self.settings.strength,
        ]);
        uniform[36] = self.settings.bias;
        queue.write_buffer(&gpu.uniform, 0, bytemuck::cast_slice(&uniform));
        let group = |visibility: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("scene ambient inputs"),
                layout: &gpu.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(depth),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&self.indirect),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(visibility),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: gpu.uniform.as_entire_binding(),
                    },
                ],
            })
        };
        // Bind an unrelated view while visibility is a render attachment.
        let horizon_group = group(&self.indirect);
        let subtract_group = group(&self.visibility);
        for (label, pipeline, group, target, load) in [
            (
                "scene horizon occlusion",
                &gpu.horizon,
                &horizon_group,
                &self.visibility,
                wgpu::LoadOp::Clear(wgpu::Color::WHITE),
            ),
            (
                "indirect-only AO union",
                &gpu.subtract,
                &subtract_group,
                scene,
                wgpu::LoadOp::Load,
            ),
        ] {
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
    }
}
impl Gpu {
    fn new(device: &wgpu::Device) -> Self {
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
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene AO layout"),
            entries: &[
                entry(0, tex(wgpu::TextureSampleType::Depth)),
                entry(1, tex(wgpu::TextureSampleType::Float { filterable: false })),
                entry(2, tex(wgpu::TextureSampleType::Float { filterable: false })),
                entry(
                    3,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
            ],
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene AO world radius"),
            size: 160,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene depth AO"),
            source: wgpu::ShaderSource::Wgsl(include_str!("scene_ao.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene AO"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry, format, blend| {
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
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let subtract = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::ReverseSubtract,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };
        Self {
            horizon: pipeline("horizon", wgpu::TextureFormat::R16Float, None),
            subtract: pipeline("subtract_indirect", super::post::HDR_FORMAT, Some(subtract)),
            layout,
            uniform,
        }
    }
}
