//! Matching color and sun-caster pipelines for the bounded actor mesh layouts.

pub(super) fn pair(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    buffers: &[Option<wgpu::VertexBufferLayout<'_>>],
    alpha_cutout: bool,
) -> (wgpu::RenderPipeline, wgpu::RenderPipeline) {
    let targets = [Some(wgpu::ColorTargetState {
        format,
        blend: None,
        write_mask: wgpu::ColorWrites::ALL,
    })];
    let create = |shadow| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(if shadow {
                "actor sun casters"
            } else {
                "instanced actors"
            }),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some(if shadow { "vs_shadow" } else { "vs_main" }),
                compilation_options: Default::default(),
                buffers,
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: if alpha_cutout {
                    None
                } else {
                    Some(wgpu::Face::Back)
                },
                ..Default::default()
            },
            depth_stencil: Some(if shadow {
                super::super::sun_shadow::depth_state()
            } else {
                wgpu::DepthStencilState {
                    format: super::DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: Default::default(),
                    bias: Default::default(),
                }
            }),
            multisample: Default::default(),
            fragment: (!shadow || alpha_cutout).then_some(wgpu::FragmentState {
                module: shader,
                entry_point: Some(if shadow { "fs_shadow" } else { "fs_main" }),
                compilation_options: Default::default(),
                targets: if shadow { &[] } else { &targets },
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    (create(false), create(true))
}
