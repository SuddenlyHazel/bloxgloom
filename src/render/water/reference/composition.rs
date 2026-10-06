//! Private sqrt-water target; the rest of the frame always remains linear HDR.
pub(super) struct Composition {
    encode: wgpu::RenderPipeline,
    decode: wgpu::RenderPipeline,
    target: Option<wgpu::TextureView>,
    pub(super) reflection: Option<wgpu::TextureView>,
    pub(super) front_depth: Option<wgpu::TextureView>,
    depth_copy: wgpu::RenderPipeline,
    mips: super::mips::Mips,
}
impl Composition {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("reference water blend encoding"),
            source: wgpu::ShaderSource::Wgsl(include_str!("composition.wgsl").into()),
        });
        let pipeline = |entry| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("reference water blend conversion"),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &if entry == "encode" {
                        vec![
                            Some(wgpu::ColorTargetState {
                                format: crate::render::post::HDR_FORMAT,
                                blend: None,
                                write_mask: wgpu::ColorWrites::ALL,
                            }),
                            Some(wgpu::ColorTargetState {
                                format: super::mips::FORMAT,
                                blend: None,
                                write_mask: wgpu::ColorWrites::ALL,
                            }),
                        ]
                    } else {
                        vec![Some(wgpu::ColorTargetState {
                            format: crate::render::post::HDR_FORMAT,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        })]
                    },
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let depth_copy = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("source water front depth initialization"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("copy_depth"),
                compilation_options: Default::default(),
                targets: &[],
            }),
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: crate::render::DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        Self {
            encode: pipeline("encode"),
            decode: pipeline("decode"),
            target: None,
            reflection: None,
            front_depth: None,
            depth_copy,
            mips: super::mips::Mips::new(device),
        }
    }
    pub(super) fn begin(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
        opaque_depth: &wgpu::TextureView,
    ) -> wgpu::TextureView {
        let size = scene.texture().size();
        if self
            .target
            .as_ref()
            .is_none_or(|target| target.texture().size() != size)
        {
            self.reflection = Some(Self::texture(
                device,
                size,
                super::mips::FORMAT,
                "source encoded opaque reflections",
            ));
            self.front_depth = Some(Self::texture(
                device,
                size,
                crate::render::DEPTH_FORMAT,
                "source nearest translucent depth",
            ));
            self.target = Some(
                device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some("reference water private sqrt HDR"),
                        size,
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: crate::render::post::HDR_FORMAT,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                            | wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    })
                    .create_view(&Default::default()),
            );
        }
        let target = self.target.as_ref().unwrap();
        let reflection_base =
            self.reflection
                .as_ref()
                .unwrap()
                .texture()
                .create_view(&wgpu::TextureViewDescriptor {
                    base_mip_level: 0,
                    mip_level_count: Some(1),
                    ..Default::default()
                });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("source water opaque scene snapshot"),
            layout: &self.encode.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(scene),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(opaque_depth),
                },
            ],
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("source water sqrt and reflection initialization"),
                color_attachments: &[
                    Some(Self::attachment(target)),
                    Some(Self::attachment(&reflection_base)),
                ],
                ..Default::default()
            });
            pass.set_pipeline(&self.encode);
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.mips
            .generate(device, encoder, self.reflection.as_ref().unwrap().texture());
        let depth_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("source water opaque depth snapshot"),
            layout: &self.depth_copy.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(opaque_depth),
            }],
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("source water front depth initialization"),
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: self.front_depth.as_ref().unwrap(),
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&self.depth_copy);
            pass.set_bind_group(0, &depth_group, &[]);
            pass.draw(0..3, 0..1);
        }
        target.clone()
    }
    pub(super) fn finish(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
    ) {
        if let Some(target) = &self.target {
            Self::draw(device, encoder, &self.decode, target, scene);
        }
    }
    fn texture(
        device: &wgpu::Device,
        size: wgpu::Extent3d,
        format: wgpu::TextureFormat,
        label: &str,
    ) -> wgpu::TextureView {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size,
                mip_level_count: if format == super::mips::FORMAT {
                    32 - size.width.max(size.height).leading_zeros()
                } else {
                    1
                },
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
    fn attachment(view: &wgpu::TextureView) -> wgpu::RenderPassColorAttachment<'_> {
        wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        }
    }
    fn draw(
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        pipeline: &wgpu::RenderPipeline,
        source: &wgpu::TextureView,
        target: &wgpu::TextureView,
    ) {
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("reference water blend source"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(source),
            }],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("reference water blend conversion"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
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
