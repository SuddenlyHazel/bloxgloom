//! Explicit headless readback; never part of live rendering or measured frame work.
use super::Gpu;
mod stats;

const SHADER: &str = r#"
@group(0) @binding(0) var geometry:texture_2d<f32>;
@group(0) @binding(1) var<storage,read_write> bins:array<atomic<u32>>;
@group(0) @binding(2) var radiance:texture_2d<f32>;
@group(0) @binding(3) var correction:texture_2d<f32>;
@group(0) @binding(4) var<storage,read_write> samples:array<vec4f>;
@compute @workgroup_size(8,8) fn histogram(@builtin(global_invocation_id) pixel:vec3u) {
    if any(pixel.xy>=textureDimensions(geometry)) {return;}
    let p=vec2i(pixel.xy);let g=textureLoad(geometry,p,0);
    if g.z < -2.0 && g.w>0.0 {
        atomicAdd(&bins[min(u32(g.w),32u)],1u);
        atomicAdd(&bins[69u],u32(g.w));
        atomicMax(&bins[70u],u32(g.w));
        if u32(g.w)>=RAY_DIAGNOSTIC_HISTORY_LIMIT {atomicAdd(&bins[71u],1u);}
        let light=textureLoad(radiance,p,0).rgb+textureLoad(correction,p,0).rgb;
        if any((bitcast<vec3u>(light)&vec3u(0x7f800000u))==vec3u(0x7f800000u)) {
            atomicAdd(&bins[68u],1u);return;
        }
        samples[pixel.x+pixel.y*textureDimensions(geometry).x]=vec4f(light,1.0);
        let peak=max(0.0,max(light.x,max(light.y,light.z)));
        let bucket=select(0u,u32(clamp(floor(log2(max(peak,0.00000001)))+17.0,1.0,33.0)),peak>0.0);
        atomicAdd(&bins[33u+bucket],1u);
        atomicMax(&bins[67u],bitcast<u32>(peak));
    }
}
"#;

impl Gpu {
    pub(in crate::render::trace) fn water_history_diagnostics(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<String, Box<dyn std::error::Error>> {
        if self.frame == 0 {
            return Ok("GI accumulated static water: no submitted trace frames".into());
        }
        let history_index = (self.frame.wrapping_sub(1) as usize) % 2;
        let geometry = &self.history_geometry[history_index];
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("headless water history histogram"),
            source: wgpu::ShaderSource::Wgsl(
                format!(
                    "const RAY_DIAGNOSTIC_HISTORY_LIMIT:u32={}u;\n{SHADER}",
                    self.history_samples.max(32)
                )
                .into(),
            ),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("headless water history histogram"),
            layout: None,
            module: &shader,
            entry_point: Some("histogram"),
            compilation_options: Default::default(),
            cache: None,
        });
        let size = 72 * 4;
        let extent = geometry.texture().size();
        let sample_bytes = u64::from(extent.width) * u64::from(extent.height) * 16;
        let bins = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("water age bins"),
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let read = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("water age readback"),
            size: size + sample_bytes,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let samples = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("headless linear water samples"),
            size: sample_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("water age diagnostics"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(geometry),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: bins.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&self.history[history_index]),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&self.current_correction),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: samples.as_entire_binding(),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(extent.width.div_ceil(8), extent.height.div_ceil(8), 1);
        }
        encoder.copy_buffer_to_buffer(&bins, 0, &read, 0, size);
        encoder.copy_buffer_to_buffer(&samples, 0, &read, size, sample_bytes);
        let submission = queue.submit(Some(encoder.finish()));
        let (tx, rx) = std::sync::mpsc::channel();
        read.map_async(wgpu::MapMode::Read, .., move |result| {
            let _ = tx.send(result);
        });
        device.poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(std::time::Duration::from_secs(30)),
        })?;
        rx.recv()??;
        let mapped = read.get_mapped_range(..)?;
        let data: &[u32] = bytemuck::cast_slice(&mapped);
        let counts = &data[..33];
        let pixels: u32 = counts.iter().sum();
        let mean = f64::from(data[69]) / f64::from(pixels.max(1));
        let light: &[[f32; 4]] = bytemuck::cast_slice(&mapped[size as usize..]);
        let solar = self.previous_atmosphere.map_or(0.0, |atmosphere| {
            super::Atmosphere {
                cloud: 0.0,
                ..atmosphere
            }
            .sun_radiance()
            .dot(super::Vec3::new(0.2126, 0.7152, 0.0722))
        });
        let summary = stats::summarize(light, extent.width, solar);
        let result = format!(
            "GI accumulated static water: trace-frames={} pixels={pixels} mean-age={mean:.3} max-age={} history-limit={} at-limit={} age-bins={counts:?} (age>=32 in final bin); linear-HDR max-channel-peak={:.6} non-finite-pixels={} intensity-bins={:?} (zero, then log2 floor + 17 clamped to 1..33); {summary}",
            self.frame,
            data[70],
            self.history_samples.max(32),
            data[71],
            f32::from_bits(data[67]),
            data[68],
            &data[33..67]
        );
        drop(mapped);
        read.unmap();
        Ok(result)
    }
}
