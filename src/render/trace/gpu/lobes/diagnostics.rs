//! Six filter-only views of one immutable primary history, with no new paths.
use super::super::Gpu;
mod composite;
mod draw;
mod source;
mod stats;
#[cfg(test)]
mod tests;

pub(in crate::render::trace) struct Snapshot {
    pub width: u32,
    pub height: u32,
    pub outputs: Vec<Vec<[f32; 4]>>,
    pub guide: Vec<[f32; 4]>,
    pub full_frame: Option<[wgpu::TextureView; 2]>,
}
impl Gpu {
    pub(in crate::render::trace) fn cached_water_filters(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        views: [&wgpu::TextureView; 4],
        full_frame: bool,
    ) -> Result<Option<Snapshot>, Box<dyn std::error::Error>> {
        let Some(reconstruction) = &self.water_reconstruction else {
            return Ok(None);
        };
        if self.frame == 0 {
            return Ok(None);
        }
        let current = (self.frame as usize - 1) % 2;
        let size = self.history[current].texture().size();
        if reconstruction.guide[current].texture().size() != size {
            return Ok(None);
        }
        let [depth, normal, response, indirect] = views;
        let family_layout = draw::layout(device, &[0, 1], None, None);
        let filter_layout = draw::layout(
            device,
            &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
            Some(2),
            Some(5),
        );
        let mut encoder = device.create_command_encoder(&Default::default());
        let mut family_views = vec![self.history[current].clone()];
        for rgb in ["r.rgb", "t.rgb-r.rgb"] {
            let output = draw::target(device, size);
            let pipeline = draw::pipeline(
                device,
                &source::FAMILY.replace("FAMILY_RGB", rgb),
                "fs_family",
                &family_layout,
            );
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&self.history[current]),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(
                            &self.primary_transmission[current],
                        ),
                    },
                ],
            });
            draw::encode(&mut encoder, &pipeline, &group, &output);
            family_views.push(output);
        }
        let mut outputs = Vec::new();
        for optical in [false, true] {
            for (family, family_view) in family_views.iter().enumerate() {
                let pipeline = draw::pipeline(
                    device,
                    &source::filter(optical, family),
                    "fs_filter",
                    &filter_layout,
                );
                let layout = pipeline.get_bind_group_layout(0);
                let binding = |binding, resource| wgpu::BindGroupEntry { binding, resource };
                let entries = [
                    binding(
                        0,
                        wgpu::BindingResource::TextureView(if optical {
                            &self.history[current]
                        } else {
                            family_view
                        }),
                    ),
                    binding(1, wgpu::BindingResource::TextureView(normal)),
                    binding(2, self.uniform.as_entire_binding()),
                    binding(
                        3,
                        wgpu::BindingResource::TextureView(&self.history_geometry[current]),
                    ),
                    binding(4, wgpu::BindingResource::TextureView(indirect)),
                    binding(5, wgpu::BindingResource::TextureView(depth)),
                    binding(6, wgpu::BindingResource::TextureView(response)),
                    binding(
                        7,
                        wgpu::BindingResource::TextureView(&self.primary_transmission[current]),
                    ),
                    binding(8, wgpu::BindingResource::TextureView(&self.baseline)),
                    binding(
                        9,
                        wgpu::BindingResource::TextureView(&self.current_correction),
                    ),
                    binding(
                        10,
                        wgpu::BindingResource::TextureView(&reconstruction.moments[current]),
                    ),
                    binding(
                        11,
                        wgpu::BindingResource::TextureView(&reconstruction.guide[current]),
                    ),
                ];
                let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &layout,
                    entries: &entries,
                });
                let output = draw::target(device, size);
                draw::encode(&mut encoder, &pipeline, &group, &output);
                outputs.push(output);
            }
        }
        let full_frame =
            full_frame.then(|| composite::encode(self, device, &mut encoder, views, &outputs));
        queue.submit([encoder.finish()]);
        let outputs = outputs
            .iter()
            .map(|view| draw::read(device, queue, view))
            .collect::<Result<Vec<_>, _>>()?;
        let guide = draw::read(device, queue, &reconstruction.guide[current])?;
        Ok(Some(Snapshot {
            width: size.width,
            height: size.height,
            outputs,
            guide,
            full_frame,
        }))
    }
}
