//! Scalar/vector page decoding must leave complete submitted histories and signed dynamics exact.
use super::*;

fn install_trace(f: &Fixture, gpu: &mut Gpu, source: &str) {
    let shader = f.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("independent scalar/vector complete transport"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let layout = f
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[
                Some(&gpu.layout),
                Some(&f.material_layout),
                Some(&gpu.dynamic.layout),
            ],
            immediate_size: 0,
        });
    gpu.trace = f
        .device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("scalar/vector four MRT parity"),
            layout: Some(&layout),
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
                entry_point: Some("fs_transport"),
                compilation_options: Default::default(),
                targets: &[
                    HDR_FORMAT,
                    HDR_FORMAT,
                    wgpu::TextureFormat::R16Float,
                    HDR_FORMAT,
                ]
                .map(|format| {
                    Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })
                }),
            }),
            multiview_mask: None,
            cache: None,
        });
}

#[test]
fn gpu_vector_lod_preserves_complete_mrts_signed_dynamics_and_history() {
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
    let pages = (0..4)
        .map(|page| {
            let mut triangles = quad(-4.0 - page as f32 * 2.0, layer, [0.5; 2]);
            for triangle in &mut triangles {
                triangle.b[3] = 1.0;
                *triangle = triangle.with_surface(
                    [0.2 + page as f32 * 0.1, 0.6, 0.3, 1.0],
                    surface::LOD
                        | surface::COARSE_COLOR
                        | surface::NO_WIND
                        | (15 << surface::GLOW_SHIFT),
                );
            }
            Scene::build([Arc::new(Chunk {
                triangles,
                ..Default::default()
            })])
        })
        .collect::<Vec<_>>();
    let mut vector =
        Gpu::new_with_lod_mode(&f.device, &scene, &pages, size, &f.material_layout, false);
    let mut scalar =
        Gpu::new_with_lod_mode(&f.device, &scene, &pages, size, &f.material_layout, false);
    let mut near_only =
        Gpu::new_with_lod_mode(&f.device, &scene, &[], size, &f.material_layout, false);
    let scalar_source = super::super::shaders::transport_with_lod_vector(false, false, false);
    install_trace(&f, &mut scalar, &scalar_source);
    install_trace(
        &f,
        &mut vector,
        &super::super::lod::vector::source_for(&scalar_source, true),
    );
    vector.transport_tile_edge = 64;
    scalar.transport_tile_edge = 64;
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
            label: Some("vector equivalence raster baseline"),
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
            label: Some("vector equivalence depth"),
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
    for frame in 0..3 {
        targets.instances[0].world =
            Mat4::from_translation(Vec3::new(frame as f32 * 0.2, 0.0, -2.5));
        ready(&f, &mut vector, &targets);
        ready(&f, &mut scalar, &targets);
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
        submit(&mut vector);
        let vector_hdr = read(&f, &hdr);
        submit(&mut scalar);
        let scalar_hdr = read(&f, &hdr);
        let current = (vector.frame as usize - 1) % 2;
        let exact = |name: &str, a: Vec<[f32; 4]>, b: Vec<[f32; 4]>| {
            assert_eq!(a.len(), b.len());
            for (pixel, (a, b)) in a.iter().zip(&b).enumerate() {
                assert_eq!(
                    a.map(f32::to_bits),
                    b.map(f32::to_bits),
                    "{name} frame{frame} pixel{pixel}: vector{a:?} scalar{b:?}"
                );
            }
        };
        for (name, a, b) in [
            (
                "radiance",
                &vector.history[current],
                &scalar.history[current],
            ),
            (
                "geometry",
                &vector.history_geometry[current],
                &scalar.history_geometry[current],
            ),
            (
                "transmission",
                &vector.primary_transmission[current],
                &scalar.primary_transmission[current],
            ),
            (
                "current dynamic correction",
                &vector.current_correction,
                &scalar.current_correction,
            ),
            ("immutable baseline", &vector.baseline, &scalar.baseline),
            ("filtered radiance", &vector.filtered, &scalar.filtered),
        ] {
            exact(name, read(&f, a), read(&f, b));
        }
        exact("composited HDR", vector_hdr, scalar_hdr);
        if frame == 0 {
            let reference = read(&f, &scalar.history[current]);
            ready(&f, &mut near_only, &targets);
            submit(&mut near_only);
            let without_lod = read(&f, &near_only.history[0]);
            assert!(
                reference.iter().zip(&without_lod).any(|(a, b)| a[..3]
                    .iter()
                    .zip(&b[..3])
                    .any(|(a, b)| a.to_bits() != b.to_bits())),
                "complete fixture never exercised admitted LOD transport"
            );
        }
        dynamic_nonzero |= read(&f, &vector.current_correction)
            .iter()
            .any(|p| p[..3].iter().any(|&v| v != 0.0));
        if frame == 2 {
            assert!(
                read(&f, &vector.history_geometry[current])
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
    // A second diagnostic submission exposes every final RNG bit in four exact
    // u8 lanes of the current target. The preceding submissions compared all
    // unmodified production MRTs/history/HDR; this does not alter integrator work.
    let diagnostic_source = scalar_source.replace(
        "return ray_primary_transport(frag);",
        "var value=ray_primary_transport(frag);value.current=vec4f(f32(ray_rng&255u),f32((ray_rng>>8u)&255u),f32((ray_rng>>16u)&255u),f32(ray_rng>>24u));value.transmission.r=select(0.0,1.0,ray_dynamic_touched);return value;",
    );
    assert_eq!(
        scalar_source
            .matches("return ray_primary_transport(frag);")
            .count(),
        1
    );
    install_trace(&f, &mut scalar, &diagnostic_source);
    install_trace(
        &f,
        &mut vector,
        &super::super::lod::vector::source_for(&diagnostic_source, true),
    );
    let submit_diagnostic = |gpu: &mut Gpu| {
        let mut encoder = f.device.create_command_encoder(&Default::default());
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &hdr,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.2,
                            g: 0.1,
                            b: 0.05,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
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
        assert!(gpu.take_scheduling_error().is_none());
        f.queue.submit([encoder.finish()]);
    };
    submit_diagnostic(&mut scalar);
    submit_diagnostic(&mut vector);
    let rng_a = read(&f, &scalar.current_correction);
    let rng_b = read(&f, &vector.current_correction);
    assert_eq!(rng_a, rng_b, "complete path RNG stream changed");
    assert!(
        rng_a
            .iter()
            .any(|bytes| bytes.iter().any(|byte| *byte != 0.0)),
        "RNG diagnostic was empty"
    );
    let current = (scalar.frame as usize - 1) % 2;
    let touched_a = read(&f, &scalar.primary_transmission[current]);
    let touched_b = read(&f, &vector.primary_transmission[current]);
    assert_eq!(
        touched_a, touched_b,
        "complete path dynamic dependency changed"
    );
    assert!(
        touched_a.iter().any(|pixel| pixel[0] == 1.0),
        "dynamic dependency was not exercised"
    );
}
