//! Production sky/cloud/post draw. Set BLOXGLOOM_SKY_QA for reviewable PNGs.
use glam::Vec3;
#[test]
fn gpu_volume_sky_renders_clear_storm_sunset_and_night() {
    let instance = wgpu::Instance::default();
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
        eprintln!("volume sky GPU test skipped: no adapter");
        return;
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let (width, height) = (641, 359);
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("sky regression capture"),
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
    let mut post = crate::render::post::PostProcess::new(&device, width, height, format);
    post.configure(&queue, true, 1.0, 0.12);
    let mut sky = super::super::SkyRenderer::new(&device, 1, 1, crate::render::post::HDR_FORMAT);
    let directory = std::env::var_os("BLOXGLOOM_SKY_QA").map(std::path::PathBuf::from);
    let path = directory.clone().unwrap_or_else(|| {
        std::env::temp_dir().join(format!("bloxgloom-sky-{}", std::process::id()))
    });
    std::fs::create_dir_all(&path).unwrap();
    let direction = Vec3::new(-0.7, 0.35, -0.55).normalize();
    let camera = crate::render::Camera {
        position: Vec3::new(0.0, 100.0, 0.0),
        yaw: direction.z.atan2(direction.x),
        pitch: direction.y.asin(),
        fov_y_radians: 70.0f32.to_radians(),
    };
    let mut averages = Vec::new();
    for (name, phase, coverage) in [
        ("clear", 0.25, 0.0),
        ("storm", 0.25, 1.0),
        ("sunset", 0.475, 0.18),
        ("night", 0.75, 0.0),
        ("new-moon", 4.75, 0.0),
        ("transport-clear", 0.25, 1.0),
    ] {
        let mut atmosphere = crate::render::daylight::Atmosphere::at(
            (phase * crate::daylight::CYCLE_MS as f64) as u64,
        );
        atmosphere.cloud = coverage;
        atmosphere.rain_strength = if name == "storm" || name == "transport-clear" {
            1.0
        } else {
            0.0
        };
        atmosphere.scene_transport = name == "transport-clear";
        sky.configure(atmosphere);
        queue.write_buffer(
            &sky.camera,
            0,
            bytemuck::cast_slice(&super::super::sky_camera_data(
                camera, width, height, atmosphere,
            )),
        );
        let mut encoder = device.create_command_encoder(&Default::default());
        sky.prepare(&device, &mut encoder, width, height);
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sky GPU regression"),
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
        post.encode(&device, &queue, &mut encoder, &view);
        queue.submit([encoder.finish()]);
        if atmosphere.scene_transport {
            assert_skipped_clouds_are_clear(&device, &queue, sky.clouds.view.texture());
        }
        let file = path.join(format!("{name}.png"));
        let pixels =
            crate::preview::capture::save_and_read(&device, &queue, &color, width, height, &file)
                .unwrap();
        assert!(pixels.chunks_exact(4).all(|p| p[3] == 255));
        let average = pixels
            .chunks_exact(4)
            .map(|p| (f64::from(p[0]) + f64::from(p[1]) + f64::from(p[2])) / 3.)
            .sum::<f64>()
            / f64::from(width * height);
        averages.push(average);
        assert!(
            pixels
                .chunks_exact(4)
                .any(|p| p[0] != pixels[0] || p[1] != pixels[1] || p[2] != pixels[2]),
            "nonuniform sky {name}"
        );
    }
    assert!(
        averages[3] < averages[0] * 0.6,
        "night must remain substantially darker: {averages:?}"
    );
    assert!(
        (averages[0] - averages[1]).abs() > 5.,
        "storm volume visibly changes clear sky: {averages:?}"
    );
    assert!(
        averages[4] < averages[3] * 0.8,
        "new moon night must be darker than full moon: {averages:?}"
    );
    if !crate::render::bsl_reference::enabled() {
        assert!(
            (averages[5] - averages[1]).abs() > 3.0,
            "primary transport bypasses raster cloud integration: {averages:?}"
        );
    }
    if directory.is_none() {
        std::fs::remove_dir_all(path).unwrap();
    }
}

// The source rain-cloud contribution can be subtle. Inspect the actual HDR
// attachment rather than requiring an arbitrary displayed RGB difference.
fn assert_skipped_clouds_are_clear(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
) {
    let size = texture.size();
    let stride = (size.width * 8).div_ceil(256) * 256;
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("cloud bypass attachment assertion"),
        size: u64::from(stride) * u64::from(size.height),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &read,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(size.height),
            },
        },
        size,
    );
    queue.submit([encoder.finish()]);
    read.slice(..)
        .map_async(wgpu::MapMode::Read, |r| r.unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let mapped = read.slice(..).get_mapped_range().unwrap();
    for row in mapped.chunks_exact(stride as usize) {
        for pixel in row[..size.width as usize * 8].chunks_exact(8) {
            assert_eq!(
                pixel,
                [0, 0, 0, 0, 0, 0, 0, 60],
                "skipped clouds must retain zero radiance/unit transmittance"
            );
        }
    }
}
