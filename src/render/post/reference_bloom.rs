//! Source-default BSL seven-scale bloom atlas. Enhanced bloom is separate.
//! Explicit HDR box mips replace the reference runtime's generated mip chain;
//! RGB8 atlas quantization, dithering, tile layout and reconstruction are retained.
use super::HDR_FORMAT;

pub(super) const COMMON: &str = include_str!("reference_bloom/common.wgsl");
pub(super) const COMPOSITE: &str = include_str!("reference_bloom/composite.wgsl");

pub(super) struct ReferenceBloom {
    mips: Vec<wgpu::TextureView>,
    atlas: wgpu::TextureView,
    groups: Vec<wgpu::BindGroup>,
    pack_group: wgpu::BindGroup,
    pub composite_group: wgpu::BindGroup,
    copy: wgpu::RenderPipeline,
    downsample: wgpu::RenderPipeline,
    pack: wgpu::RenderPipeline,
}

impl ReferenceBloom {
    pub fn new(
        device: &wgpu::Device,
        input: &wgpu::TextureView,
        layout: &wgpu::BindGroupLayout,
        settings: &wgpu::Buffer,
    ) -> Self {
        let size = input.texture().size();
        let mip_count = size.width.max(size.height).ilog2() + 1;
        let texture = |label, format, mip_level_count| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size,
                mip_level_count,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        };
        let pyramid = texture("reference bloom HDR mip chain", HDR_FORMAT, mip_count);
        let mips: Vec<_> = (0..mip_count)
            .map(|level| {
                pyramid.create_view(&wgpu::TextureViewDescriptor {
                    base_mip_level: level,
                    mip_level_count: Some(1),
                    ..Default::default()
                })
            })
            .collect();
        let all_mips = pyramid.create_view(&Default::default());
        let atlas = texture(
            "reference bloom RGB8 atlas",
            wgpu::TextureFormat::Rgba8Unorm,
            1,
        )
        .create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("reference bloom trilinear clamp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let group = |source: &wgpu::TextureView, glow: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("reference bloom inputs"),
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
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: settings.as_entire_binding(),
                    },
                ],
            })
        };
        let mut groups = vec![group(input, input)];
        groups.extend(mips.iter().take(mips.len() - 1).map(|mip| group(mip, mip)));
        let pack_group = group(&all_mips, &all_mips);
        let composite_group = group(input, &atlas);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("reference bloom atlas shader"),
            source: wgpu::ShaderSource::Wgsl(
                format!("{COMMON}\n{}", include_str!("reference_bloom/pack.wgsl")).into(),
            ),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("reference bloom layout"),
            bind_group_layouts: &[Some(layout)],
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
            mips,
            atlas,
            groups,
            pack_group,
            composite_group,
            copy: pipeline("copy", HDR_FORMAT),
            downsample: pipeline("downsample", HDR_FORMAT),
            pack: pipeline("pack", wgpu::TextureFormat::Rgba8Unorm),
        }
    }

    pub fn encode(&self, encoder: &mut wgpu::CommandEncoder) {
        for (level, target) in self.mips.iter().enumerate() {
            draw(
                encoder,
                if level == 0 {
                    &self.copy
                } else {
                    &self.downsample
                },
                &self.groups[level],
                target,
            );
        }
        draw(encoder, &self.pack, &self.pack_group, &self.atlas);
    }
}

fn draw(
    encoder: &mut wgpu::CommandEncoder,
    pipeline: &wgpu::RenderPipeline,
    group: &wgpu::BindGroup,
    target: &wgpu::TextureView,
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("BSL reference bloom"),
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
}

#[cfg(test)]
mod tests;
