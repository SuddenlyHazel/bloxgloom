//! Deform ray geometry once per frame rather than at every ray intersection.
pub(super) struct Deformation {
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
}

impl Deformation {
    pub fn new(device: &wgpu::Device, materials: &wgpu::BindGroupLayout) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ray foliage deformation"),
            source: wgpu::ShaderSource::Wgsl(shader().into()),
        });
        let entries = [
            (0, wgpu::BufferBindingType::Uniform),
            (1, wgpu::BufferBindingType::Storage { read_only: true }),
            (2, wgpu::BufferBindingType::Storage { read_only: false }),
        ]
        .map(|(binding, ty)| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ray deformation inputs"),
            entries: &entries,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ray deformation layout"),
            bind_group_layouts: &[Some(&layout), Some(materials)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("deform foliage triangles once"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("deform"),
            compilation_options: Default::default(),
            cache: None,
        });
        Self { pipeline, layout }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn encode(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        uniform: &wgpu::Buffer,
        source: &wgpu::Buffer,
        output: &wgpu::Buffer,
        materials: &wgpu::BindGroup,
        timestamps: Option<wgpu::ComputePassTimestampWrites<'_>>,
    ) {
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ray deformation frame"),
            layout: &self.layout,
            entries: &[(0, uniform), (1, source), (2, output)].map(|(binding, buffer)| {
                wgpu::BindGroupEntry {
                    binding,
                    resource: buffer.as_entire_binding(),
                }
            }),
        });
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("ray foliage geometry"),
            timestamp_writes: timestamps,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.set_bind_group(1, materials, &[]);
        pass.dispatch_workgroups((source.size() / 96).div_ceil(64) as u32, 1, 1);
    }
}

fn shader() -> String {
    format!(
        "{}\n{}",
        include_str!("../material/foliage.wgsl"),
        include_str!("deformation.wgsl")
    )
}

#[cfg(test)]
mod tests;
