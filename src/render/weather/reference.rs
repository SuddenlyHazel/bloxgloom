//! Source default gbuffers_weather transfer and isolated sqrt-space alpha blend.
//! Only color0 is written: translucent water/shaft metadata stays authoritative.
pub(super) fn shader() -> String {
    format!(
        "{}\n{}",
        crate::render::bsl_reference::HANDLIGHT_SHADER,
        include_str!("reference/shader.wgsl")
    )
}
pub(super) struct Reference {
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    vertices: wgpu::Buffer,
    encode: wgpu::RenderPipeline,
    decode: wgpu::RenderPipeline,
    target: Option<wgpu::TextureView>,
    count: u32,
}
impl Reference {
    pub(super) fn new(device: &wgpu::Device, camera: &wgpu::Buffer) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("source rain"),
            source: wgpu::ShaderSource::Wgsl(shader().into()),
        });
        let pipeline=device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
   label:Some("reference rain"),layout:None,
   vertex:wgpu::VertexState{module:&shader,entry_point:Some("vs"),compilation_options:Default::default(),buffers:&[Some(wgpu::VertexBufferLayout{array_stride:40,step_mode:wgpu::VertexStepMode::Vertex,attributes:&wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x2,2=>Float32x4,3=>Float32]})]},
   fragment:Some(wgpu::FragmentState{module:&shader,entry_point:Some("fs"),compilation_options:Default::default(),targets:&[Some(wgpu::ColorTargetState{format:crate::render::post::HDR_FORMAT,blend:Some(wgpu::BlendState{color:wgpu::BlendComponent{src_factor:wgpu::BlendFactor::SrcAlpha,dst_factor:wgpu::BlendFactor::OneMinusSrcAlpha,operation:wgpu::BlendOperation::Add},alpha:wgpu::BlendComponent{src_factor:wgpu::BlendFactor::Zero,dst_factor:wgpu::BlendFactor::One,operation:wgpu::BlendOperation::Add}}),write_mask:wgpu::ColorWrites::ALL})]}),
   primitive:Default::default(),depth_stencil:Some(wgpu::DepthStencilState{format:crate::render::DEPTH_FORMAT,depth_write_enabled:Some(false),depth_compare:Some(wgpu::CompareFunction::LessEqual),stencil:Default::default(),bias:Default::default()}),multisample:Default::default(),multiview_mask:None,cache:None,
  });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera.as_entire_binding(),
            }],
        });
        let vertices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("reference rain lightmapped vertices"),
            size: (super::MAX_STREAKS * 3 * 40) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let composition = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(include_str!("reference/composition.wgsl").into()),
        });
        let make = |entry| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("reference rain sqrt composition"),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &composition,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &composition,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: crate::render::post::HDR_FORMAT,
                        blend: None,
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
        Self {
            pipeline,
            group,
            vertices,
            encode: make("encode"),
            decode: make("decode"),
            target: None,
            count: 0,
        }
    }
    pub(super) fn set(&mut self, queue: &wgpu::Queue, mesh: &[f32], glow: &[f32]) {
        let mut packet = Vec::with_capacity(mesh.len() / 9 * 10);
        for (i, v) in mesh
            .chunks_exact(9)
            .take(super::MAX_STREAKS * 3)
            .enumerate()
        {
            packet.extend_from_slice(v);
            packet.push(
                glow.get(i)
                    .copied()
                    .filter(|v| v.is_finite())
                    .unwrap_or(0.0)
                    .clamp(0.0, 1.0),
            );
        }
        self.count = (packet.len() / 10) as u32;
        if self.count > 0 {
            queue.write_buffer(&self.vertices, 0, bytemuck::cast_slice(&packet));
        }
    }
    pub(super) fn resolve(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
        depth: &wgpu::TextureView,
    ) -> usize {
        if self.count == 0 {
            return 0;
        }
        let size = scene.texture().size();
        if self
            .target
            .as_ref()
            .is_none_or(|v| v.texture().size() != size)
        {
            self.target = Some(
                device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some("reference rain private sqrt target"),
                        size,
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: crate::render::post::HDR_FORMAT,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING
                            | wgpu::TextureUsages::RENDER_ATTACHMENT,
                        view_formats: &[],
                    })
                    .create_view(&Default::default()),
            );
        }
        let target = self.target.as_ref().unwrap();
        Self::copy(device, encoder, &self.encode, scene, target);
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("reference rain source blending"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.group, &[]);
            pass.set_vertex_buffer(0, self.vertices.slice(..));
            pass.draw(0..self.count, 0..1);
        }
        Self::copy(device, encoder, &self.decode, target, scene);
        self.count as usize / 3
    }
    fn copy(
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        pipeline: &wgpu::RenderPipeline,
        source: &wgpu::TextureView,
        target: &wgpu::TextureView,
    ) {
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(source),
            }],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("source rain transfer"),
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
#[cfg(test)]
#[path = "reference/tests.rs"]
mod tests;
