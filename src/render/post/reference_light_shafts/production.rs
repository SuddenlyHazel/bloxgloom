use super::*;
// The independent oracle uses source double-precision arithmetic and the actual
// RGBA8 encoding, rather than calling the production WGSL helper.
fn oracle(light: [f64; 3], weights: [f64; 3]) -> [f64; 3] {
    let dither = 128.0 / 255.0;
    std::array::from_fn(|i| {
        let radiance = light[i] * 0.25 * weights[i] / 7.0;
        if radiance == 0.0 {
            return 0.0;
        }
        let encoded = ((radiance / 32.0).powf(0.25) + (dither - 0.25) / 128.0) * 255.0;
        32.0 * (encoded.round() / 255.0).powi(4)
    })
}
pub(super) fn clear(
    encoder: &mut wgpu::CommandEncoder,
    color: &wgpu::TextureView,
    rgb: [f64; 3],
    alpha: f64,
    depth: Option<(&wgpu::TextureView, f32)>,
) {
    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("independent source shaft fixture inputs"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: color,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: rgb[0],
                    g: rgb[1],
                    b: rgb[2],
                    a: alpha,
                }),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: depth.map(|(view, value)| {
            wgpu::RenderPassDepthStencilAttachment {
                view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(value),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }
        }),
        ..Default::default()
    });
}
#[test]
fn gpu_source_default_shafts_actual_depth_tint_encoding_and_lifecycle() {
    pollster::block_on(async {
        let adapter = wgpu::Instance::default()
            .request_adapter(&Default::default())
            .await
            .unwrap();
        let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
        let mut shafts = LightShafts::new(&device, 1, 1);
        shafts.gpu.noise = crate::render::sky::ReferenceNoise::fixture(&device, [0, 0, 128, 255]);
        let scene =
            texture(&device, crate::render::post::HDR_FORMAT).create_view(&Default::default());
        let metadata =
            texture(&device, crate::render::post::HDR_FORMAT).create_view(&Default::default());
        let opaque =
            texture(&device, wgpu::TextureFormat::Depth32Float).create_view(&Default::default());
        let front =
            texture(&device, wgpu::TextureFormat::Depth32Float).create_view(&Default::default());
        let shadow =
            texture(&device, wgpu::TextureFormat::Depth32Float).create_view(&Default::default());
        let light = [0.25, 0.5, 1.0];
        let dither = 128.0_f64 / 255.0;
        let water_tint: [f64; 3] = [64.0, 160.0, 255.0]
            .map(|c| 1.0 + (c * c / (255.0 * 255.0) / 0.1225 - 1.0) * 0.7_f64.powf(0.25));
        let water_sum = (0..7)
            .map(|i| (2.0_f64.powf(f64::from(i) + dither) - 0.95) / 256.0)
            .sum::<f64>();
        // map visibility/front depth/opaque depth/tint/medium/master/front supplied.
        let cases = [
            (1.0, 1.0, 1.0, [0.0; 3], false, true, true, [7.0; 3]),
            (0.0, 1.0, 1.0, [0.0; 3], false, true, true, [0.0; 3]),
            (
                1.0,
                0.25,
                1.0,
                [0.2, 0.4, 0.6],
                false,
                true,
                true,
                [6.2, 6.4, 6.6],
            ),
            (1.0, 0.25, 1.0, [0.0; 3], false, true, true, [6.0; 3]),
            (1.0, 1.0, 0.25, [0.0; 3], false, true, true, [6.0; 3]),
            (
                1.0,
                1.0,
                1.0,
                [0.0; 3],
                true,
                true,
                true,
                water_tint.map(|x| x * water_sum),
            ),
            (1.0, 1.0, 1.0, [0.0; 3], true, true, false, [0.0; 3]),
            (1.0, 1.0, 1.0, [0.0; 3], false, false, true, [0.0; 3]),
        ];
        for (index, (map, front_value, opaque_value, tint, water, enabled, has_front, weights)) in
            cases.into_iter().enumerate()
        {
            let mut data = [0.0_f32; 56];
            data[..16].copy_from_slice(
                &glam::Mat4::from_scale(glam::Vec3::new(1.0, 1.0, 256.0)).to_cols_array(),
            );
            let projection = glam::Mat4::from_scale(glam::Vec3::new(0.01, 0.01, 0.001));
            let mut projection = projection.to_cols_array();
            projection[14] = 0.25;
            data[16..32].copy_from_slice(&projection);
            data[35] = 1.0;
            data[38] = 1.0;
            data[42] = 1.0;
            data[43] = 1.0;
            data[44..47].copy_from_slice(&[0.25, 0.5, 1.0]);
            data[47] = 1.0;
            data[49] = 1.0;
            data[51] = 1.0 / 2048.0;
            data[53] = 0.9;
            shafts.input = Some(Input {
                opaque: opaque.clone(),
                shadow: shadow.clone(),
                data,
                enabled: true,
            });
            shafts.medium(water);
            let mut encoder = device.create_command_encoder(&Default::default());
            clear(
                &mut encoder,
                &scene,
                [0.0; 3],
                1.0,
                Some((&opaque, opaque_value)),
            );
            clear(
                &mut encoder,
                &metadata,
                tint,
                if tint == [0.0; 3] { 0.0 } else { -2.0 },
                Some((&front, front_value)),
            );
            clear(
                &mut encoder,
                &shafts.scratch,
                [0.0; 3],
                1.0,
                Some((&shadow, map)),
            );
            shafts.encode(
                &device,
                &queue,
                &mut encoder,
                &scene,
                has_front.then_some(&front),
                &metadata,
                enabled,
            );
            let actual = read_color(&device, &queue, encoder, &scene);
            let expected = oracle(light, weights);
            for (actual, expected) in actual[..3].iter().zip(expected) {
                assert!(
                    (f64::from(*actual) - expected).abs() < 0.002,
                    "case{index}: {actual} vs{expected}"
                );
            }
        }
        // Submission is the only history clock, and resize/discard cannot retain
        // a previously recorded world's shadow/opaque-depth views.
        assert_eq!(shafts.frame, 0);
        shafts.submitted();
        assert_eq!(shafts.frame, 1);
        assert!(shafts.input.is_none());
        shafts.resize(&device, 2, 3);
        assert!(shafts.input.is_none());
        assert_eq!(shafts.frame, 1);
    });
}
