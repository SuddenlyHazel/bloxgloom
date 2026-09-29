//! Renderer-owned execution and cached inputs for a verified effect graph.
use crate::render::parameters::Value;
mod pass;
mod targets;
use pass::GpuPass;
use targets::target_sizes;

pub(crate) struct Effect {
    passes: Vec<GpuPass>,
    targets: Vec<wgpu::TextureView>,
    groups: Vec<wgpu::BindGroup>,
    final_index: usize,
    started: std::time::Instant,
}
impl Effect {
    pub fn prepare(device: &wgpu::Device, prepared: &super::Prepared) -> Result<Self, String> {
        std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    let passes = prepared
                        .passes
                        .iter()
                        .map(|p| GpuPass::compile(device, p))
                        .collect::<Result<_, _>>()?;
                    Ok(Self {
                        passes,
                        targets: Vec::new(),
                        groups: Vec::new(),
                        final_index: prepared.final_index,
                        started: std::time::Instant::now(),
                    })
                })
                .join()
                .map_err(|_| "effect graph GPU preparation worker panicked".to_string())?
        })
    }
    pub fn resize(&mut self, device: &wgpu::Device, scene: &wgpu::TextureView) {
        let size = scene.texture().size();
        let sizes = target_sizes(
            size.width,
            size.height,
            device.limits().max_texture_dimension_2d,
            &self
                .passes
                .iter()
                .map(|p| p.prepared.descriptor.scale)
                .collect::<Vec<_>>(),
        );
        self.targets = self
            .passes
            .iter()
            .zip(&sizes)
            .map(|(pass, &(width, height))| {
                device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some(&pass.prepared.owner),
                        size: wgpu::Extent3d {
                            width,
                            height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: crate::render::post::HDR_FORMAT,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                            | wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    })
                    .create_view(&Default::default())
            })
            .collect();
        self.groups = self
            .passes
            .iter()
            .map(|pass| {
                let input = |index: usize| {
                    let key = &pass.prepared.descriptor.inputs
                        [index.min(pass.prepared.descriptor.inputs.len() - 1)];
                    if key == super::SCENE {
                        scene
                    } else {
                        &self.targets[self
                            .passes
                            .iter()
                            .position(|p| p.prepared.descriptor.output.as_ref() == Some(key))
                            .expect("verified graph input")]
                    }
                };
                let mut entries = vec![
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(input(0)),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&pass.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: pass.time.as_entire_binding(),
                    },
                ];
                if pass.prepared.descriptor.version == 2 {
                    entries.push(wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(input(1)),
                    });
                }
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(&pass.prepared.owner),
                    layout: &pass.layout,
                    entries: &entries,
                })
            })
            .collect();
    }
    pub fn output(&self) -> &wgpu::TextureView {
        &self.targets[self.final_index]
    }
    pub fn set(&mut self, resource: &str, name: &str, value: &Value) -> Result<bool, String> {
        let Some(pass) = self
            .passes
            .iter_mut()
            .find(|p| p.prepared.owner == resource)
        else {
            return Ok(false);
        };
        let (slot, definition) = pass
            .prepared
            .parameters()
            .iter()
            .enumerate()
            .find(|(_, p)| p.name == name)
            .ok_or_else(|| format!("{resource}: unknown parameter {name}"))?;
        pass.data.parameters[slot] = definition.pack(value)?;
        Ok(true)
    }
    pub fn encode(&self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder) {
        for ((pass, target), group) in self.passes.iter().zip(&self.targets).zip(&self.groups) {
            let size = target.texture().size();
            let mut data = pass.data;
            data.frame = [
                self.started.elapsed().as_secs_f32(),
                0.0,
                size.width as f32,
                size.height as f32,
            ];
            queue.write_buffer(&pass.time, 0, bytemuck::bytes_of(&data));
            let mut render = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(&pass.prepared.owner),
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
            render.set_pipeline(&pass.pipeline);
            render.set_bind_group(0, group, &[]);
            render.draw(0..3, 0..1);
        }
    }
}
