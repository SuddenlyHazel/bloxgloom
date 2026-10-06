//! Production filter kernels tested without resampling transport.
use super::*;
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;
fn texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    size: wgpu::Extent3d,
    pixels: &[[f32; 4]],
) -> wgpu::TextureView {
    device
        .create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: None,
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            bytemuck::cast_slice(pixels),
        )
        .create_view(&Default::default())
}
#[test]
fn gpu_cached_water_filter_lobes_preserve_hdr_split_mean_and_optical_edges() {
    let (device, queue) = crate::render::trace::tests::denoise::device();
    let size = wgpu::Extent3d {
        width: 16,
        height: 16,
        depth_or_array_layers: 1,
    };
    let full = wgpu::Extent3d {
        width: 32,
        height: 32,
        ..size
    };
    let inverse =
        (glam::camera::rh::proj::directx::perspective(10f32.to_radians(), 1.0, 0.1, 100.0)
            * glam::camera::rh::view::look_at_mat4(Vec3::ZERO, -Vec3::Y, Vec3::Z))
        .inverse();
    let mut uniform = [0.0f32; 76];
    uniform[..16].copy_from_slice(&inverse.to_cols_array());
    uniform[16..32].copy_from_slice(&Mat4::IDENTITY.to_cols_array());
    uniform[33] = 10.0;
    uniform[62] = 2.0;
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&uniform),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let layout = draw::layout(
        &device,
        &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
        Some(2),
        Some(5),
    );
    let depth = device
        .create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: full,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default());
    let dummy = texture(&device, &queue, full, &vec![[0.0; 4]; 1024]);
    for case in 0..3 {
        let mut totals = Vec::new();
        let mut reflection = Vec::new();
        let mut geometry = Vec::new();
        let mut guides = Vec::new();
        let mut moments = Vec::new();
        for y in 0..16 {
            for x in 0..16 {
                let direction = inverse
                    * glam::Vec4::new(
                        (x as f32 * 2.0 + 1.5) / 32.0 * 2.0 - 1.0,
                        1.0 - (y as f32 * 2.0 + 1.5) / 32.0 * 2.0,
                        1.0,
                        1.0,
                    );
                let direction = (direction.truncate() / direction.w).normalize();
                let distance = -10.0 / direction.y;
                let noise = if (x + y) % 2 == 0 { 1.0 } else { -1.0 };
                let (r, t, n) = match case {
                    0 => (14721.34, 123.0, [0.0, 1.0]),
                    1 => (2.0 + noise, 4.0 - noise, [0.0, 1.0]),
                    _ => (
                        if x < 8 { 1.0 } else { 100.0 },
                        4.0,
                        if x < 8 { [0.0, 1.0] } else { [1.0, 0.0] },
                    ),
                };
                totals.push([r + t, r + t, r + t, distance]);
                reflection.push([r, r, r, 1.0]);
                geometry.push([0.0, 1.0, -3.4, 1.0]);
                guides.push([n[0], n[1], 0.4, 1.0]);
                moments.push(if case == 1 {
                    [2.0, 5.0, 4.0, 17.0]
                } else {
                    [r, r * r, t, t * t]
                });
            }
        }
        let packet = texture(&device, &queue, size, &reflection);
        let geom = texture(&device, &queue, size, &geometry);
        let guide = texture(&device, &queue, size, &guides);
        let moment = texture(&device, &queue, size, &moments);
        let mut outputs = Vec::new();
        for optical in [false, true] {
            for family in 0..3 {
                let pixels = totals
                    .iter()
                    .zip(&reflection)
                    .map(|(t, r)| {
                        let rgb = if optical || family == 0 {
                            t[0]
                        } else if family == 1 {
                            r[0]
                        } else {
                            t[0] - r[0]
                        };
                        [rgb, rgb, rgb, t[3]]
                    })
                    .collect::<Vec<_>>();
                let input = texture(&device, &queue, size, &pixels);
                let pipeline = draw::pipeline(
                    &device,
                    &source::filter(optical, family),
                    "fs_filter",
                    &layout,
                );
                let tex = |binding, view| wgpu::BindGroupEntry {
                    binding,
                    resource: wgpu::BindingResource::TextureView(view),
                };
                let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &layout,
                    entries: &[
                        tex(0, &input),
                        tex(1, &dummy),
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: uniform.as_entire_binding(),
                        },
                        tex(3, &geom),
                        tex(4, &dummy),
                        tex(5, &depth),
                        tex(6, &dummy),
                        tex(7, &packet),
                        tex(8, &dummy),
                        tex(9, &dummy),
                        tex(10, &moment),
                        tex(11, &guide),
                    ],
                });
                let output = draw::target(&device, size);
                let mut encoder = device.create_command_encoder(&Default::default());
                draw::encode(&mut encoder, &pipeline, &group, &output);
                queue.submit([encoder.finish()]);
                outputs.push(draw::read(&device, &queue, &output).unwrap());
            }
        }
        for i in 0..256 {
            for mode in 0..2 {
                for ((r, t), combined) in outputs[mode * 3 + 1][i][..3]
                    .iter()
                    .zip(&outputs[mode * 3 + 2][i][..3])
                    .zip(&outputs[mode * 3][i][..3])
                {
                    let sum = r + t;
                    assert!(
                        (sum - combined).abs() <= 2e-5 * sum.abs().max(1.0),
                        "same-sample split sum case{case} mode{mode}"
                    );
                }
            }
            assert!(
                outputs
                    .iter()
                    .all(|pixels| pixels[i].iter().all(|v| v.is_finite()))
            );
            if case == 0 {
                for (family, expected) in [(0, 14844.34), (1, 14721.34), (2, 123.0)] {
                    assert!(
                        (outputs[3 + family][i][0] - expected).abs() <= 2e-5 * expected,
                        "constant HDR lobe energy"
                    );
                }
            }
            if case == 2 {
                assert!(
                    (outputs[4][i][0] - reflection[i][0]).abs() <= 1e-5,
                    "actual optical normal edge remains sharp pixel{i}: {} != {}",
                    outputs[4][i][0],
                    reflection[i][0]
                );
                assert!(
                    (outputs[5][i][0] - 4.0).abs() <= 1e-5,
                    "constant transmission survives reflection edge"
                );
            }
        }
        if case == 1 {
            for (family, mean) in [(1, 2.0f32), (2, 4.0)] {
                let observed = outputs[3 + family]
                    .iter()
                    .map(|v| f64::from(v[0]))
                    .sum::<f64>()
                    / 256.0;
                let variance = outputs[3 + family]
                    .iter()
                    .map(|v| f64::from(v[0] - mean).powi(2))
                    .sum::<f64>()
                    / 256.0;
                assert!(
                    (observed - f64::from(mean)).abs() < 0.01,
                    "known illumination mean preserved"
                );
                assert!(
                    variance < 0.2,
                    "bounded lobe filtering lowers known checker noise without clipping: {variance}"
                );
            }
        }
    }
}
#[test]
fn cached_water_filter_diagnostic_sources_validate() {
    for optical in [false, true] {
        for family in 0..3 {
            let source = source::filter(optical, family);
            let module = wgpu::naga::front::wgsl::parse_str(&source)
                .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
            wgpu::naga::valid::Validator::new(
                wgpu::naga::valid::ValidationFlags::all(),
                wgpu::naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .unwrap();
        }
    }
}
