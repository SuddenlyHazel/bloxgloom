//! Compare production transport submissions, including history and signed dynamics.
use super::*;

#[test]
fn gpu_tiled_transport_matches_single_batch_mrts_baseline_and_history() {
    let catalog = crate::content::Catalog::builtins();
    let f = Fixture::new(&catalog);
    let layer = catalog
        .textures()
        .iter()
        .position(|t| t.key.ends_with(":jg_cherry_log"))
        .unwrap() as u32;
    let scene = Scene::build([Arc::new(Chunk {
        triangles: quad(1.0, layer, [0.5; 2]),
        ..Default::default()
    })]);
    // Odd full and transport extents cross both 64-pixel tile boundaries.
    let (width, height) = (269, 141);
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let mut tiled = Gpu::new_with_lod(&f.device, &scene, &[], size, &f.material_layout);
    let mut whole = Gpu::new_with_lod(&f.device, &scene, &[], size, &f.material_layout);
    tiled.transport_tile_edge = 64;
    whole.transport_tile_edge = 0;
    let receivers = (0..height)
        .flat_map(|y| {
            (0..width).map(move |x| {
                let nx = (x as f32 + 0.5) / width as f32 * 2.0 - 1.0;
                let ny = 1.0 - (y as f32 + 0.5) / height as f32 * 2.0;
                [1.0, 1.0, 0.18, (nx * nx + ny * ny + 1.0).sqrt()]
            })
        })
        .collect::<Vec<_>>();
    let normal = texture(&f, width, height, &receivers);
    let response = texture(
        &f,
        width,
        height,
        &vec![[0.0; 4]; (width * height) as usize],
    );
    let indirect = texture(
        &f,
        width,
        height,
        &vec![[0.03, 0.03, 0.03, 1.0]; (width * height) as usize],
    );
    let hdr = f
        .device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("tiled equivalence raster baseline"),
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
            label: Some("tiled equivalence depth"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default());
    let mut targets = DynamicTargets {
        instances: vec![drop_target(layer)],
    };
    let mut dynamic_nonzero = false;
    for frame in 0..6 {
        match frame {
            3 => tiled
                .set_headless_transport_scheduling(Some(64), 1)
                .unwrap(),
            4 => tiled
                .set_headless_transport_scheduling(Some(32), 4)
                .unwrap(),
            5 => tiled
                .set_headless_transport_scheduling(Some(128), 4)
                .unwrap(),
            _ => {}
        }
        targets.instances[0].world =
            Mat4::from_translation(Vec3::new(frame as f32 * 0.2, 0.0, -2.5));
        ready(&f, &mut tiled, &targets);
        ready(&f, &mut whole, &targets);
        let submit = |gpu: &mut Gpu| {
            let mut encoder = f.device.create_command_encoder(&Default::default());
            {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &hdr,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.2 + frame as f64 * 0.03,
                                g: 0.1,
                                b: 0.05,
                                a: 1.0,
                            }),
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
                &indirect,
                &f.materials,
                Mat4::IDENTITY,
                Vec3::ZERO,
                atmosphere,
                0.0,
                None,
            );
            assert!(
                gpu.take_scheduling_error().is_none(),
                "controlled scheduling fixture failed"
            );
            f.queue.submit([encoder.finish()]);
        };
        submit(&mut tiled);
        let tiled_hdr = read(&f, &hdr);
        submit(&mut whole);
        let whole_hdr = read(&f, &hdr);
        let current = (tiled.frame as usize - 1) % 2;
        let exact = |name: &str, a: Vec<[f32; 4]>, b: Vec<[f32; 4]>| {
            assert_eq!(a.len(), b.len());
            for (pixel, (a, b)) in a.iter().zip(&b).enumerate() {
                assert_eq!(
                    a.map(f32::to_bits),
                    b.map(f32::to_bits),
                    "{name} frame{frame} pixel{pixel}: tiled{a:?} whole{b:?}"
                );
            }
        };
        for (name, a, b) in [
            ("radiance", &tiled.history[current], &whole.history[current]),
            (
                "geometry",
                &tiled.history_geometry[current],
                &whole.history_geometry[current],
            ),
            (
                "transmission",
                &tiled.primary_transmission[current],
                &whole.primary_transmission[current],
            ),
            (
                "current dynamic correction",
                &tiled.current_correction,
                &whole.current_correction,
            ),
            ("immutable baseline", &tiled.baseline, &whole.baseline),
            ("filtered radiance", &tiled.filtered, &whole.filtered),
        ] {
            exact(name, read(&f, a), read(&f, b));
        }
        exact("composited HDR", tiled_hdr, whole_hdr);
        dynamic_nonzero |= read(&f, &tiled.current_correction)
            .iter()
            .any(|p| p[..3].iter().any(|&v| v != 0.0));
        if frame == 2 {
            assert!(
                read(&f, &tiled.history_geometry[current])
                    .iter()
                    .any(|p| p[3] >= 3.0),
                "fixture never exercised accumulated history"
            );
        }
    }
    assert!(
        dynamic_nonzero,
        "fixture never exercised paired dynamic correction"
    );
}
