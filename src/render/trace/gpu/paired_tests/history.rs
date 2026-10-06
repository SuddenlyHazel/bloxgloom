//! Actual production history must average beyond 32 without changing live defaults.
use super::*;

#[test]
fn gpu_diagnostic_history_averages_64_samples_while_default_retains_32() {
    let catalog = crate::content::Catalog::builtins();
    let f = Fixture::new(&catalog);
    let matte = |z: f32, glow: u32| {
        quad(z, 0, [0.5; 2]).into_iter().map(move |t| {
            t.with_surface(
                [1.0; 4],
                surface::LOD
                    | surface::COARSE_COLOR
                    | surface::NO_WIND
                    | (glow << surface::GLOW_SHIFT),
            )
        })
    };
    let scene = Scene::build([Arc::new(Chunk {
        triangles: matte(1.0, 0).chain(matte(-4.0, 15)).collect(),
        ..Default::default()
    })]);
    let (width, height) = (16, 16);
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let mut progressive = Gpu::new_with_lod(&f.device, &scene, &[], size, &f.material_layout);
    let mut live = Gpu::new_with_lod(&f.device, &scene, &[], size, &f.material_layout);
    let mut fresh = Gpu::new_with_lod(&f.device, &scene, &[], size, &f.material_layout);
    // Avoid process-wide environment mutation; test the actual per-Gpu uniform path.
    progressive.history_samples = 64;
    live.history_samples = 0;
    fresh.history_samples = 0;
    let receivers: Vec<_> = (0..height)
        .flat_map(|y| {
            (0..width).map(move |x| {
                let nx = (x as f32 + 0.5) / width as f32 * 2.0 - 1.0;
                let ny = 1.0 - (y as f32 + 0.5) / height as f32 * 2.0;
                [1.0, 1.0, 0.18, (nx * nx + ny * ny + 1.0).sqrt()]
            })
        })
        .collect();
    let normal = texture(&f, width, height, &receivers);
    let response = texture(
        &f,
        width,
        height,
        &vec![[0.0; 4]; (width * height) as usize],
    );
    let indirect_a = texture(
        &f,
        width,
        height,
        &vec![[0.03, 0.03, 0.03, 1.0]; (width * height) as usize],
    );
    let indirect_b = texture(
        &f,
        width,
        height,
        &vec![[0.015, 0.015, 0.015, 1.0]; (width * height) as usize],
    );
    let hdr = f
        .device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("progressive history baseline"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default());
    let depth = f
        .device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("progressive history depth"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default());
    let submit = |gpu: &mut Gpu, indirect: &wgpu::TextureView| {
        gpu.sample_seed = Some(444);
        let mut encoder = f.device.create_command_encoder(&Default::default());
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &hdr,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
        }
        let mut atmosphere = Atmosphere::at(crate::daylight::INITIAL_MS);
        atmosphere.cloud = 0.0;
        atmosphere.fog = 0.0;
        gpu.resolve(
            &f.device,
            &f.queue,
            &mut encoder,
            &hdr,
            &depth,
            &normal,
            &response,
            indirect,
            &f.materials,
            Mat4::IDENTITY,
            Vec3::ZERO,
            atmosphere,
            0.0,
            None,
        );
        f.queue.submit([encoder.finish()]);
    };
    // Independent unaccumulated controls. Only the normalization basis changes:
    // complete transport/RNG/current targets are identical in both populations.
    submit(&mut fresh, &indirect_a);
    let a = read(&f, &fresh.history[(fresh.frame as usize - 1) % 2]);
    fresh.history_valid = false;
    submit(&mut fresh, &indirect_b);
    let b = read(&f, &fresh.history[(fresh.frame as usize - 1) % 2]);
    for sample in 0..64 {
        let indirect = if sample < 32 {
            &indirect_a
        } else {
            &indirect_b
        };
        submit(&mut progressive, indirect);
        submit(&mut live, indirect);
    }
    let current = (progressive.frame as usize - 1) % 2;
    let progressive_geometry = read(&f, &progressive.history_geometry[current]);
    let live_geometry = read(&f, &live.history_geometry[current]);
    let progressive_light = read(&f, &progressive.history[current]);
    let live_light = read(&f, &live.history[current]);
    let mut discriminating_channels = 0;
    for pixel in 0..a.len() {
        assert_eq!(
            progressive_geometry[pixel][3], 64.0,
            "diagnostic age pixel{pixel}"
        );
        assert_eq!(live_geometry[pixel][3], 32.0, "live age pixel{pixel}");
        for channel in 0..3 {
            let expected_mean = (a[pixel][channel] + b[pixel][channel]) * 0.5;
            if expected_mean <= 0.001 {
                continue;
            }
            let expected_live = b[pixel][channel]
                + (a[pixel][channel] - b[pixel][channel]) * (31.0f32 / 32.0).powi(32);
            // Independent arithmetic/EMA oracles permit repeated HDR16 rounding.
            let tolerance = expected_mean.abs() * 0.02;
            assert!(
                (progressive_light[pixel][channel] - expected_mean).abs() <= tolerance,
                "64-sample mean pixel{pixel} channel{channel}: got{} expected{expected_mean}",
                progressive_light[pixel][channel]
            );
            assert!(
                (live_light[pixel][channel] - expected_live).abs() <= tolerance,
                "default32 EMA pixel{pixel} channel{channel}: got{} expected{expected_live}",
                live_light[pixel][channel]
            );
            if (expected_live - expected_mean).abs() > tolerance * 3.0 {
                discriminating_channels += 1;
                assert!(
                    (progressive_light[pixel][channel] - live_light[pixel][channel]).abs()
                        > tolerance * 2.0
                );
            }
        }
    }
    assert!(
        discriminating_channels >= 12,
        "fixture must distinguish a cumulative mean from32 EMA"
    );
}
