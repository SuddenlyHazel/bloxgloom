use super::*;

impl Reconstruction {
    pub(in crate::render::trace::gpu) fn encode(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        geometry: &wgpu::TextureView,
        old: usize,
        current: usize,
    ) {
        let textures = [&self.raw, geometry, &self.moments[old], &self.guide[old]];
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("first-water exact primary history pixel"),
            layout: &self.layout,
            entries: &std::array::from_fn::<_, 4, _>(|binding| wgpu::BindGroupEntry {
                binding: binding as u32,
                resource: wgpu::BindingResource::TextureView(textures[binding]),
            }),
        });
        let attachments = [&self.moments[current], &self.guide[current]].map(|view| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("true raw first-water HDR moments"),
            color_attachments: &attachments,
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }
}
