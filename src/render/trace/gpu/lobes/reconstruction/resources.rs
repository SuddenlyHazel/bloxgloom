use super::*;

fn target(device: &wgpu::Device, size: wgpu::Extent3d, raw: bool) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some(if raw {
                "current raw first-water samples"
            } else {
                "first-water float32 moments/guide"
            }),
            size: wgpu::Extent3d {
                depth_or_array_layers: if raw { 3 } else { 1 },
                ..size
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | if raw {
                    wgpu::TextureUsages::STORAGE_BINDING
                } else {
                    wgpu::TextureUsages::RENDER_ATTACHMENT
                },
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(if raw {
                wgpu::TextureViewDimension::D2Array
            } else {
                wgpu::TextureViewDimension::D2
            }),
            ..Default::default()
        })
}

impl Reconstruction {
    pub(in crate::render::trace::gpu) fn fits(device: &wgpu::Device, size: wgpu::Extent3d) -> bool {
        capabilities::fits(device, size)
    }
    pub(in crate::render::trace::gpu) fn storage_entry() -> wgpu::BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry {
            binding: 16,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::StorageTexture {
                access: wgpu::StorageTextureAccess::WriteOnly,
                format: wgpu::TextureFormat::Rgba32Float,
                view_dimension: wgpu::TextureViewDimension::D2Array,
            },
            count: None,
        }
    }
    pub(in crate::render::trace::gpu) fn new(
        device: &wgpu::Device,
        size: wgpu::Extent3d,
        filtering: bool,
    ) -> Self {
        assert!(capabilities::fits(device, size));
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("first-water true raw moment inputs"),
            entries: &std::array::from_fn::<_, 4, _>(|binding| wgpu::BindGroupLayoutEntry {
                binding: binding as u32,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: if binding == 0 {
                        wgpu::TextureViewDimension::D2Array
                    } else {
                        wgpu::TextureViewDimension::D2
                    },
                    multisampled: false,
                },
                count: None,
            }),
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("first-water true raw moment integration"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../water/lobes/moments.wgsl").into(),
            ),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let targets: [Option<wgpu::ColorTargetState>; 2] = std::array::from_fn(|_| {
            Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba32Float,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("first-water HDR moment accumulation"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_moments"),
                compilation_options: Default::default(),
                targets: &targets,
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        Self {
            raw: target(device, size, true),
            moments: std::array::from_fn(|_| target(device, size, false)),
            guide: std::array::from_fn(|_| target(device, size, false)),
            layout,
            pipeline,
            filtering,
        }
    }
    pub(in crate::render::trace::gpu) fn resize(
        &mut self,
        device: &wgpu::Device,
        size: wgpu::Extent3d,
    ) {
        // An opted renderer that grows beyond the bounded resource budget retains
        // its validated estimator and legacy filter. One-pixel invalid metadata
        // satisfies the opted layout without allocating unbounded images.
        let size = if capabilities::fits(device, size) {
            size
        } else {
            tracing::warn!("first-water reconstruction resize exceeds256MiB; using legacy filter");
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            }
        };
        self.raw = target(device, size, true);
        self.moments = std::array::from_fn(|_| target(device, size, false));
        self.guide = std::array::from_fn(|_| target(device, size, false));
    }
}
