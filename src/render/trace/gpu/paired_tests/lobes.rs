//! Actual opt-in MRT/bind-group/history proof against the untouched default path.
use super::*;
use crate::render::trace::tests::transport::water_paths;

#[test]
fn gpu_opted_first_water_lobes_preserve_full_history_correction_and_packed_transmission() {
    let (catalog, emitter) = water_paths::catalog();
    let f = Fixture::new(&catalog);
    let pool = water_paths::pool(&catalog, emitter, true, false, false);
    let size = wgpu::Extent3d {
        width: 16,
        height: 16,
        depth_or_array_layers: 1,
    };
    let mut old = Gpu::new_with_lod_mode(&f.device, &pool, &[], size, &f.material_layout, false);
    let mut split = Gpu::new_with_lod_mode(&f.device, &pool, &[], size, &f.material_layout, true);
    assert_eq!(
        split.primary_transmission[0].texture().format(),
        wgpu::TextureFormat::Rgba16Float
    );
    assert_eq!(
        old.primary_transmission[0].texture().format(),
        wgpu::TextureFormat::R16Float
    );
    let eye = Vec3::new(8.0, 8.0, 8.0);
    let matrix =
        glam::camera::rh::proj::directx::perspective(28.0f32.to_radians(), 1.0, 0.1, 100.0)
            * glam::camera::rh::view::look_at_mat4(eye, eye - Vec3::Y, Vec3::Z);
    let pixels = (size.width * size.height) as usize;
    let normal = texture(
        &f,
        size.width,
        size.height,
        &vec![[0.0, 0.0, 0.12, 2.0]; pixels],
    );
    let response = texture(&f, size.width, size.height, &vec![[0.0; 4]; pixels]);
    let indirect = texture(
        &f,
        size.width,
        size.height,
        &vec![[0.03, 0.03, 0.03, -2.0]; pixels],
    );
    let hdr = f
        .device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("first-water exact raster cancellation"),
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
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default());
    let id = catalog
        .textures()
        .iter()
        .position(|texture| texture.key.as_ref() == "bloxgloom:jg_cherry_log")
        .unwrap() as u32;
    let mut actor = drop_target(id);
    actor.world = Mat4::from_scale_rotation_translation(
        Vec3::splat(3.0),
        glam::Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
        Vec3::new(8.0, 10.0, 8.0),
    );
    // Admit identical current drop artwork before history, avoiding an unrelated
    // one-time material admission reset during this pose/removal sequence.
    for gpu in [&mut old, &mut split] {
        ready(
            &f,
            gpu,
            &DynamicTargets {
                instances: vec![actor.clone()],
            },
        );
    }
    let mut reflected = 0;
    let mut transmitted = 0;
    let mut dynamic_energy = 0.0f32;
    let mut previous_age = 0.0;
    for frame in 0..10u32 {
        let mut current_actor = actor.clone();
        current_actor.world = Mat4::from_scale_rotation_translation(
            Vec3::splat(3.0),
            glam::Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
            Vec3::new(8.0 + frame as f32 * 0.03, 10.0 + frame as f32 * 0.02, 8.0),
        );
        let targets = DynamicTargets {
            instances: if (3..7).contains(&frame) {
                vec![current_actor]
            } else {
                vec![]
            },
        };
        let mut results = Vec::new();
        for gpu in [&mut old, &mut split] {
            ready(&f, gpu, &targets);
            gpu.sample_seed = Some(1049 + frame * 17);
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
                matrix,
                eye,
                atmosphere,
                0.0,
                None,
            );
            assert!(gpu.take_scheduling_error().is_none());
            f.queue.submit([encoder.finish()]);
            let index = (gpu.frame as usize - 1) % 2;
            results.push((
                read(&f, &gpu.history[index]),
                read(&f, &gpu.history_geometry[index]),
                read(&f, &gpu.current_correction),
                read(&f, &gpu.primary_transmission[index]),
                read(&f, &hdr),
            ));
        }
        let (a, b) = (&results[0], &results[1]);
        for pixel in 0..a.0.len() {
            assert_eq!(
                a.1[pixel].map(f32::to_bits),
                b.1[pixel].map(f32::to_bits),
                "geometry/age frame{frame} pixel{pixel}"
            );
            assert!(
                b.1[pixel][2] < -3.0 && b.1[pixel][3] >= 1.0,
                "real camera-air first water"
            );
            assert_eq!(
                a.3[pixel][0].to_bits(),
                b.3[pixel][3].to_bits(),
                "original primaryT must occupy W"
            );
            for channel in 0..3 {
                for (name, x, y) in [
                    ("full static", a.0[pixel][channel], b.0[pixel][channel]),
                    (
                        "signed current correction",
                        a.2[pixel][channel],
                        b.2[pixel][channel],
                    ),
                ] {
                    assert!(
                        (x - y).abs() <= 2e-5 * x.abs().max(1.0),
                        "{name} frame{frame} pixel{pixel}: {x} != {y}"
                    );
                }
                assert!(
                    b.3[pixel][channel] >= 0.0,
                    "reflection unmodified positive energy"
                );
                assert!(
                    b.0[pixel][channel] - b.3[pixel][channel] >= -2e-5,
                    "remainder unmodified positive energy"
                );
                dynamic_energy += b.2[pixel][channel].abs();
            }
            reflected += usize::from(b.3[pixel][..3].iter().any(|v| *v > 0.0001));
            transmitted += usize::from((0..3).any(|c| b.0[pixel][c] - b.3[pixel][c] > 0.0001));
        }
        for (pixel, (x, y)) in a.4.iter().zip(&b.4).enumerate() {
            for channel in 0..3 {
                assert!(
                    (x[channel] - y[channel]).abs() <= 2e-5 * x[channel].abs().max(1.0),
                    "final HDR/exact raster cancellation frame{frame} pixel{pixel}: {x:?} != {y:?}"
                );
            }
        }
        let age = b.1.iter().map(|v| v[3]).sum::<f32>() / b.1.len() as f32;
        if (4..7).contains(&frame) {
            assert!(
                age > previous_age,
                "moving off-screen drop must not reset receiver history"
            );
        }
        previous_age = age;
        if frame >= 7 {
            assert!(
                b.2.iter().all(|v| v[..3] == [0.0; 3]),
                "current drop removal applies immediately"
            );
        }
    }
    assert!(
        reflected > 0 && transmitted > 0 && dynamic_energy > 0.0001,
        "both optical lobes and current off-screen actor visibility must contribute"
    );
}
