//! Exact submitted empty→pending→actor→removal→readmission behavior for the optional pipeline.
use super::*;

#[test]
fn gpu_certified_empty_dynamic_preserves_admission_history_mrts_and_rng() {
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
    // Odd full/transport extents include a partial tile beyond the 64-pixel boundary.
    let (width, height) = (129, 69);
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
    let mut specialized =
        Gpu::new_with_lod_mode(&f.device, &scene, &pages, size, &f.material_layout, false);
    let mut generic =
        Gpu::new_with_lod_mode(&f.device, &scene, &pages, size, &f.material_layout, false);
    let generic_source = super::super::shaders::transport_with_lod_vector(false, false, false);
    specialized.empty_trace = Some(super::super::empty_dynamic::pipeline(
        &f.device,
        &super::super::empty_dynamic::source_for(&generic_source, true),
        &specialized.layout,
        &f.material_layout,
        &specialized.dynamic.layout,
        false,
    ));
    generic.empty_trace = None;
    specialized.trace = super::super::empty_dynamic::pipeline(
        &f.device,
        &generic_source,
        &specialized.layout,
        &f.material_layout,
        &specialized.dynamic.layout,
        false,
    );
    generic.trace = super::super::empty_dynamic::pipeline(
        &f.device,
        &generic_source,
        &generic.layout,
        &f.material_layout,
        &generic.dynamic.layout,
        false,
    );
    specialized.transport_tile_edge = 64;
    generic.transport_tile_edge = 64;
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
            label: Some("specialized equivalence raster baseline"),
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
            label: Some("specialized equivalence depth"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default());
    let empty = DynamicTargets::default();
    let mut targets = DynamicTargets {
        instances: vec![drop_target(layer)],
    };
    let mut dynamic_nonzero = false;
    for phase in 0..9 {
        targets.instances[0].world =
            Mat4::from_translation(Vec3::new((phase as f32 - 3.0) * 0.2, 0.0, -2.5));
        if phase == 2 {
            // A not-yet-uploaded asset explicitly clears the current geometry header.
            assert!(!specialized.prepare_dynamic(&f.device, &f.queue, &targets));
            assert!(!generic.prepare_dynamic(&f.device, &f.queue, &targets));
        } else {
            let active = if [3, 4, 7].contains(&phase) {
                &targets
            } else {
                &empty
            };
            ready(&f, &mut specialized, active);
            ready(&f, &mut generic, active);
        }
        let certified = ![3, 4, 7].contains(&phase);
        assert_eq!(
            specialized.dynamic.is_empty(),
            certified,
            "uploaded-node certificate phase{phase}"
        );
        assert_eq!(generic.dynamic.is_empty(), certified);
        assert_eq!(
            specialized.empty_dynamic_active(),
            certified,
            "wrong selected pipeline phase{phase}"
        );
        assert!(!generic.empty_dynamic_active());
        assert_eq!(
            specialized.history_valid, generic.history_valid,
            "admission invalidation phase{phase}"
        );
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
                                r: 0.2 + phase as f64 * 0.03,
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
        submit(&mut specialized);
        let specialized_hdr = read(&f, &hdr);
        submit(&mut generic);
        let generic_hdr = read(&f, &hdr);
        let current = (generic.frame as usize - 1) % 2;
        let exact = |name: &str, a: Vec<[f32; 4]>, b: Vec<[f32; 4]>| {
            assert_eq!(a.len(), b.len());
            for (pixel, (a, b)) in a.iter().zip(&b).enumerate() {
                assert_eq!(
                    a.map(f32::to_bits),
                    b.map(f32::to_bits),
                    "{name} phase{phase} pixel{pixel}: specialized{a:?} generic{b:?}"
                );
            }
        };
        for (name, a, b) in [
            (
                "radiance",
                &specialized.history[current],
                &generic.history[current],
            ),
            (
                "geometry",
                &specialized.history_geometry[current],
                &generic.history_geometry[current],
            ),
            (
                "transmission",
                &specialized.primary_transmission[current],
                &generic.primary_transmission[current],
            ),
            (
                "current signed correction",
                &specialized.current_correction,
                &generic.current_correction,
            ),
            ("baseline", &specialized.baseline, &generic.baseline),
            ("filtered", &specialized.filtered, &generic.filtered),
        ] {
            exact(name, read(&f, a), read(&f, b));
        }
        exact("composited HDR", specialized_hdr, generic_hdr);
        dynamic_nonzero |= read(&f, &generic.current_correction)
            .iter()
            .any(|p| p[..3].iter().any(|v| *v != 0.0));
        if [1, 6].contains(&phase) {
            assert!(
                read(&f, &generic.history_geometry[current])
                    .iter()
                    .any(|p| p[3] >= 2.0),
                "empty scene never accumulated history phase{phase}"
            );
        }
    }
    assert!(
        dynamic_nonzero,
        "nonempty generic fallback never exercised current dynamic correction"
    );
    // Separate final submission exposes all RNG bits after the unmodified four-MRT proof.
    let anchor = "return ray_primary_transport(frag);";
    assert_eq!(generic_source.matches(anchor).count(), 1);
    let diagnostic = generic_source.replace(anchor, "var value=ray_primary_transport(frag);value.current=vec4f(f32(ray_rng&255u),f32((ray_rng>>8u)&255u),f32((ray_rng>>16u)&255u),f32(ray_rng>>24u));value.transmission.r=select(0.0,1.0,ray_dynamic_touched);return value;");
    generic.trace = super::super::empty_dynamic::pipeline(
        &f.device,
        &diagnostic,
        &generic.layout,
        &f.material_layout,
        &generic.dynamic.layout,
        false,
    );
    specialized.empty_trace = Some(super::super::empty_dynamic::pipeline(
        &f.device,
        &super::super::empty_dynamic::source_for(&diagnostic, true),
        &specialized.layout,
        &f.material_layout,
        &specialized.dynamic.layout,
        false,
    ));
    let phase = 9;
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
                            r: 0.2 + phase as f64 * 0.03,
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
    submit(&mut specialized);
    submit(&mut generic);
    let a = read(&f, &specialized.current_correction);
    let b = read(&f, &generic.current_correction);
    assert_eq!(a, b, "empty specialization changed final RNG bits");
    assert!(
        a.iter().any(|p| p.iter().any(|v| *v != 0.0)),
        "RNG diagnostic empty"
    );
    let current = (generic.frame as usize - 1) % 2;
    let a = read(&f, &specialized.primary_transmission[current]);
    let b = read(&f, &generic.primary_transmission[current]);
    assert_eq!(a, b, "empty specialization changed dependency flags");
    assert!(
        a.iter().all(|p| p[0] == 0.0),
        "empty frame touched dynamic geometry"
    );
}
