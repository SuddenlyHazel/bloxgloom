//! Size-dependent attachments and bindings; resizing keeps compiled pipelines.
use super::HDR_FORMAT;

pub(super) struct Targets {
    pub scene: wgpu::TextureView,
    pub bloom: [wgpu::TextureView; 2],
    pub groups: [wgpu::BindGroup; 3],
    pub composite_group: wgpu::BindGroup,
}

impl Targets {
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        settings: &wgpu::Buffer,
        effect: Option<&mut crate::render::effects::Effect>,
    ) -> Self {
        let texture = |label, width, height| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: HDR_FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let scene = texture("linear HDR scene", width.max(1), height.max(1));
        let color = if let Some(effect) = effect {
            effect.resize(device, &scene);
            effect.output()
        } else {
            &scene
        };
        let bloom = [
            texture(
                "bloom A",
                width.div_ceil(4).max(1),
                height.div_ceil(4).max(1),
            ),
            texture(
                "bloom B",
                width.div_ceil(4).max(1),
                height.div_ceil(4).max(1),
            ),
        ];
        let group = |source: &wgpu::TextureView, glow: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("post inputs"),
                layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(source),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(glow),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: settings.as_entire_binding(),
                    },
                ],
            })
        };
        // Each filter binds only its source, never the current attachment.
        let groups = [
            group(color, color),
            group(&bloom[0], &bloom[0]),
            group(&bloom[1], &bloom[1]),
        ];
        let composite_group = group(color, &bloom[0]);
        Self {
            scene,
            bloom,
            groups,
            composite_group,
        }
    }
}
