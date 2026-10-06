use super::*;
#[test]
fn gpu_actual_reference_sky_adds_source_underwater_solar_only_in_water() {
    pollster::block_on(async {
        let adapter = wgpu::Instance::default()
            .request_adapter(&Default::default())
            .await
            .unwrap();
        let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
        let source = format!(
            "{}\n{}\n{}",
            crate::render::sky::environment_shader(),
            include_str!("../../sky/camera.wgsl"),
            include_str!("../../sky/sky.wgsl")
        );
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("actual reference sky SunGlare consumer"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&[0.0_f32; 40]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let targets: Vec<_> = (0..4)
            .map(|_| {
                texture(&device, crate::render::post::HDR_FORMAT).create_view(&Default::default())
            })
            .collect();
        let cloud =
            texture(&device, crate::render::post::HDR_FORMAT).create_view(&Default::default());
        let art =
            texture(&device, crate::render::post::HDR_FORMAT).create_view(&Default::default());
        let sampler = device.create_sampler(&Default::default());
        let target_states: Vec<_> = (0..4)
            .map(|_| {
                Some(wgpu::ColorTargetState {
                    format: crate::render::post::HDR_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })
            })
            .collect();
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &target_states,
            }),
            multiview_mask: None,
            cache: None,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                crate::render::post::reference_display::texture_entry(1, &cloud),
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                crate::render::post::reference_display::texture_entry(3, &art),
                crate::render::post::reference_display::texture_entry(4, &art),
            ],
        });
        let mut baseline = None;
        for (medium, reference) in [(0.0, 1.0), (-2.0, 1.0), (-2.0, 0.0)] {
            let mut data = [0.0_f32; 40];
            data[2] = 1.0;
            data[14] = 1.0;
            data[15] = medium;
            data[24..28].copy_from_slice(&[0.25, 0.5, 1.0, 1.0]);
            data[29] = 80.0;
            data[31] = 1.0;
            data[33] = 1.0;
            data[36] = 1.0;
            data[39] = reference;
            queue.write_buffer(&uniform, 0, bytemuck::cast_slice(&data));
            let mut encoder = device.create_command_encoder(&Default::default());
            super::production::clear(&mut encoder, &cloud, [0.0; 3], 1.0, None);
            super::production::clear(&mut encoder, &art, [0.0; 3], 1.0, None);
            crate::render::post::reference_display::draw(
                &mut encoder,
                &pipeline,
                &group,
                &targets.iter().collect::<Vec<_>>(),
            );
            let actual = read_color(&device, &queue, encoder, &targets[0]);
            if medium == 0.0 {
                baseline = Some(actual);
                continue;
            }
            let baseline = baseline.unwrap();
            for i in 0..3 {
                let expected = if reference > 0.5 {
                    [0.0625, 0.125, 0.25][i]
                } else {
                    0.0
                };
                assert!(
                    (actual[i] - baseline[i] - expected).abs() < 0.001,
                    "medium{medium} reference{reference}: {actual:?} vs{baseline:?}"
                );
            }
        }
    });
}
