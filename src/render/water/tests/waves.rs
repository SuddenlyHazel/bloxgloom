//! Check the actual fragment footprint filter against numerical integration.
use wgpu::util::DeviceExt;

fn gaussian_response(frequency: f64) -> f64 {
    // Integrate the cosine response of a pixel Gaussian, sigma=1/sqrt(12).
    // This is independent of the shader's closed-form exponential.
    let sigma = 1.0 / 12.0f64.sqrt();
    let mut numerator = 0.0;
    let mut denominator = 0.0;
    for i in 0..4096 {
        let x = ((i as f64 + 0.5) / 4096.0 * 16.0 - 8.0) * sigma;
        let weight = (-0.5 * (x / sigma).powi(2)).exp();
        numerator += weight * (frequency * x).cos();
        denominator += weight;
    }
    numerator / denominator
}

#[test]
fn fragment_water_waves_remove_aliases_and_preserve_unresolved_slope_energy() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let waves = include_str!("../waves.wgsl");
    let source = format!(
        "{waves}\n{}",
        r#"
@group(0) @binding(0) var<uniform> step:vec4f;
@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
    let xy=array(vec2f(-1.0,-1.0),vec2f(3.0,-1.0),vec2f(-1.0,3.0));
    return vec4f(xy[i],0.0,1.0);
}
@fragment fn fs(@builtin(position) pixel:vec4f)->@location(0) vec4f {
    let xz=vec2f(0.7,-0.2)+step.xy*pixel.x+step.zw*pixel.y;
    let footprint=vec4f(dpdx(xz),dpdy(xz));
    return vec4f(bg_water_waves(vec3f(xz.x,0.0,xz.y),0.3,footprint),1.0);
}
"#
    );
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("water wave footprint fixture"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba32Float,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    let output = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 16,
            height: 4,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = output.create_view(&Default::default());
    for step in [
        [0.0f32; 4],
        [0.01, 0.0, 0.0, 0.01],
        [0.7, 0.1, -0.1, 0.4],
        [1.5, 0.0, 0.0, 1.5],
        [16.0, 0.0, 0.0, 16.0],
        [0.0, 4.0, 0.0, 0.0],
        [0.8, -0.7, 0.9, 0.6],
    ] {
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&step),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 1024,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                ..Default::default()
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..3, 0..1);
        }
        encoder.copy_texture_to_buffer(
            output.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(4),
                },
            },
            output.size(),
        );
        queue.submit([encoder.finish()]);
        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |result| result.unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let bytes = slice.get_mapped_range().unwrap();
        let actual: &[[f32; 4]] = bytemuck::cast_slice(&bytes);
        // Independent spectral quadrature, including each wave's direction.
        let spectrum = [
            [0.44f64, 0.15, 0.026, 0.2, 0.1],
            [-0.26, 0.34, 0.023, 0.3, 1.8],
            [0.10, 0.66, 0.018, 0.4, 4.1],
            [-0.78, 0.17, 0.014, 0.5, 0.7],
            [0.36, -1.15, 0.012, 0.7, 2.6],
            [1.27, 0.47, 0.009, 0.9, 3.9],
            [-0.48, 1.72, 0.006, 1.2, 1.2],
            [2.18, -0.29, 0.004, 1.5, 5.4],
        ];
        let retained: Vec<_> = spectrum
            .iter()
            .map(|wave| {
                let dx = f64::from(step[0]) * wave[0] + f64::from(step[1]) * wave[1];
                let dy = f64::from(step[2]) * wave[0] + f64::from(step[3]) * wave[1];
                let t =
                    ((dx.abs().max(dy.abs()) - 2.0) / (std::f64::consts::PI - 2.0)).clamp(0.0, 1.0);
                gaussian_response(dx) * gaussian_response(dy) * (1.0 - t * t * (3.0 - 2.0 * t))
            })
            .collect();
        for (index, pixel) in actual.iter().enumerate() {
            let x = 0.7
                + f64::from(step[0]) * (index % 16) as f64
                + f64::from(step[2]) * (index / 16) as f64
                + 0.5 * f64::from(step[0] + step[2]);
            let z = -0.2
                + f64::from(step[1]) * (index % 16) as f64
                + f64::from(step[3]) * (index / 16) as f64
                + 0.5 * f64::from(step[1] + step[3]);
            let mut expected = [0.0; 3];
            for (wave, attenuation) in spectrum.iter().zip(&retained) {
                let length = wave[0].hypot(wave[1]);
                let slope = (wave[0] * x + wave[1] * z + wave[3] * 0.3 + wave[4]).sin()
                    * wave[2]
                    * attenuation;
                expected[0] += slope * wave[0] / length;
                expected[1] += slope * wave[1] / length;
                expected[2] += wave[2].powi(2) * 0.5 * (1.0 - attenuation.powi(2));
            }
            for channel in 0..3 {
                assert!(
                    (f64::from(pixel[channel]) - expected[channel]).abs() < 2e-6,
                    "step={step:?} pixel={index} channel={channel}: {pixel:?} expected={expected:?}"
                );
            }
            assert!((0.0..=0.001002).contains(&pixel[2]));
            if step == [16.0, 0.0, 0.0, 16.0] {
                assert_eq!(
                    &pixel[..2],
                    &[0.0, 0.0],
                    "subpixel waves must not produce distant moire"
                );
            }
        }
    }
}
