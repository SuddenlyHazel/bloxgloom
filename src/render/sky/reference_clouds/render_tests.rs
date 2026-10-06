//! Optional local-input frame comparison, with the production cloud and TAA
//! passes. Always-on synthetic input goldens cover CI without BSL artwork.
#[test]
fn gpu_local_reference_clouds_accumulate_submitted_sky_samples() {
    if !crate::render::bsl_reference::enabled() {
        eprintln!("local BSL cloud capture requires explicit reference mode");
        return;
    }
    let source = std::env::var_os("BLOXGLOOM_BSL_NOISE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "external/shaders/tex/noise.png".into());
    if super::super::Noise::read(&source).is_err() {
        eprintln!("local BSL cloud capture skipped: no valid local input");
        return;
    }
    let instance = wgpu::Instance::default();
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
        return;
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    if device.adapter_info().backend == wgpu::Backend::Gl {
        return;
    }
    let (width, height) = (641, 359);
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("reference sky temporal capture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = color.create_view(&Default::default());
    let depth = crate::render::visibility::create_depth(&device, width, height);
    let direction = glam::Vec3::new(-0.7, 0.35, -0.55).normalize();
    let camera = crate::render::Camera {
        position: glam::Vec3::new(0., 100., 0.),
        yaw: direction.z.atan2(direction.x),
        pitch: direction.y.asin(),
        fov_y_radians: 70f32.to_radians(),
    };
    let atmosphere = crate::render::daylight::Atmosphere::at(crate::daylight::CYCLE_MS / 4);
    let mut sky =
        crate::render::SkyRenderer::new(&device, width, height, crate::render::post::HDR_FORMAT);
    sky.configure(atmosphere);
    let requested = std::env::var_os("BLOXGLOOM_BSL_CLOUD_QA").map(std::path::PathBuf::from);
    let directory = requested.clone().unwrap_or_else(|| {
        std::env::temp_dir().join(format!("bloxgloom-cloud-taa-{}", std::process::id()))
    });
    std::fs::create_dir_all(&directory).unwrap();
    let mut captures = Vec::new();
    for (enabled, name) in [(false, "raw"), (true, "temporal")] {
        let mut post = crate::render::post::PostProcess::new(&device, width, height, format);
        post.configure(&queue, true, 1., 0.12);
        post.enable_temporal(&device, enabled);
        for sample in if enabled { 0..24 } else { 23..24 } {
            post.prepare_temporal(&queue, camera);
            queue.write_buffer(
                &sky.camera,
                0,
                bytemuck::cast_slice(&crate::render::sky_camera_data_at_sample(
                    camera, width, height, atmosphere, sample,
                )),
            );
            let mut encoder = device.create_command_encoder(&Default::default());
            sky.prepare(&device, &mut encoder, width, height);
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &crate::render::scene_ao::attachments(
                        &post.scene,
                        &post.ambient.indirect,
                        &post.reflections.normal,
                        &post.reflections.response,
                        wgpu::Color::BLACK,
                    ),
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &depth,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    ..Default::default()
                });
                pass.set_pipeline(&sky.pipeline);
                pass.set_bind_group(0, &sky.group, &[]);
                pass.draw(0..3, 0..1);
            }
            post.draw_motion(&queue, &mut encoder, &depth, None);
            post.resolve_temporal(&device, &mut encoder, &depth);
            post.encode(&device, &queue, &mut encoder, &view);
            queue.submit([encoder.finish()]);
            post.submitted();
        }
        captures.push(
            crate::preview::capture::save_and_read(
                &device,
                &queue,
                &color,
                width,
                height,
                &directory.join(format!("{name}.png")),
            )
            .unwrap(),
        );
    }
    let detail = |pixels: &[u8]| {
        let mut sum = 0f64;
        for y in 1..height - 1 {
            for x in 1..width - 1 {
                let i = ((y * width + x) * 4) as usize;
                for c in 0..3 {
                    let lap = 4. * f64::from(pixels[i + c])
                        - f64::from(pixels[i - 4 + c])
                        - f64::from(pixels[i + 4 + c])
                        - f64::from(pixels[i - width as usize * 4 + c])
                        - f64::from(pixels[i + width as usize * 4 + c]);
                    sum += lap * lap;
                }
            }
        }
        sum
    };
    assert!(
        detail(&captures[1]) < detail(&captures[0]) * 0.8,
        "reference sky source dither did not accumulate: {} vs {}",
        detail(&captures[1]),
        detail(&captures[0])
    );
    if requested.is_none() {
        std::fs::remove_dir_all(directory).unwrap();
    }
}
