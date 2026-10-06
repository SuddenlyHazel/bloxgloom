#[test]
fn reference_rain_actual_shader_and_private_composition_validate() {
    for source in [super::shader(), include_str!("composition.wgsl").to_owned()] {
        let module = wgpu::naga::front::wgsl::parse_str(&source).unwrap();
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
    }
}
#[test]
fn gpu_reference_rain_matches_source_transfer_and_sqrt_alpha_depth_composition() {
    use wgpu::util::DeviceExt;
    let adapter =
        pollster::block_on(wgpu::Instance::default().request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut camera = [0.0f32; 80];
    camera[..16].copy_from_slice(&glam::Mat4::IDENTITY.to_cols_array());
    camera[39] = 0.75;
    camera[48..51].copy_from_slice(&[0.1, 0.2, 0.3]);
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&camera),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let mut renderer = super::Reference::new(&device, &uniform);
    let target = |format| {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: 8,
                    height: 8,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
            .create_view(&Default::default())
    };
    let scene = target(crate::render::post::HDR_FORMAT);
    let depth = target(crate::render::DEPTH_FORMAT);
    for (z, expected_visible, layers) in [(0.2, true, 1), (0.8, false, 1), (0.2, true, 2)] {
        let mut mesh = Vec::new();
        for _ in 0..layers {
            for [x, y] in [[-1.0, -1.0], [3.0, -1.0], [-1.0, 3.0]] {
                mesh.extend_from_slice(&[x, y, z, 0.5, 0.0, 0.2, 0.4, 0.7, 0.34]);
            }
        }
        renderer.set(&queue, &mesh, &vec![0.6; 3 * layers]);
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &scene,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.25,
                            g: 0.49,
                            b: 0.81,
                            a: 0.75,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.5),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
        }
        renderer.resolve(&device, &mut encoder, &scene, &depth);
        let read = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 8 * 256,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            scene.texture().as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &read,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(8),
                },
            },
            scene.texture().size(),
        );
        queue.submit([encoder.finish()]);
        read.slice(..)
            .map_async(wgpu::MapMode::Read, |r| r.unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let encoded = [0.2f64, 0.4, 0.7].map(|v| {
            if v <= 0.0031308 {
                12.92 * v
            } else {
                1.055 * v.powf(1.0 / 2.4) - 0.055
            }
        });
        let alpha = 0.34
            * 0.35
            * 0.75
            * (encoded.into_iter().map(|v| (v / 3.0).powi(2)).sum::<f64>()).sqrt()
            * 1.4;
        let mut expected = [0.5f64, 0.7, 0.9];
        if expected_visible {
            for _ in 0..layers {
                for (c, value) in expected.iter_mut().enumerate() {
                    let palette = [255.0f64, 212.0, 160.0][c] * 0.85 / 255.0;
                    let source = (encoded[c].sqrt()
                        * ([0.1, 0.2, 0.3][c] + 0.6 * 0.6 * palette * palette))
                        .sqrt();
                    *value = *value * (1.0 - alpha) + source * alpha;
                }
            }
        }
        let bytes = read.slice(..).get_mapped_range().unwrap();
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 8;
                let actual: Vec<f64> = bytes[offset..offset + 8]
                    .chunks_exact(2)
                    .map(|v| decode_half(u16::from_le_bytes([v[0], v[1]])))
                    .collect();
                for (actual, expected_channel) in actual.iter().take(3).zip(expected) {
                    assert!(
                        (actual - expected_channel * expected_channel).abs() < 0.003,
                        "z{z} layers{layers} actual{actual:?} expected{expected:?}"
                    );
                }
                assert_eq!(actual[3], 0.75);
            }
        }
    }
}

fn decode_half(bits: u16) -> f64 {
    let sign = if bits & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exponent = i32::from((bits >> 10) & 31);
    let fraction = f64::from(bits & 1023) / 1024.0;
    if exponent == 0 {
        sign * fraction * 2.0f64.powi(-14)
    } else {
        sign * (1.0 + fraction) * 2.0f64.powi(exponent - 15)
    }
}

#[test]
fn gpu_reference_weather_thresholds_raw_lightmap_and_night_match_independent_source() {
    use wgpu::util::DeviceExt;
    let adapter =
        pollster::block_on(wgpu::Instance::default().request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let source = format!(
        "{}\n{}",
        super::shader(),
        r#"
@group(0) @binding(1) var<storage,read_write> output:array<vec4f>;
@compute @workgroup_size(1) fn oracle(@builtin(global_invocation_id) id:vec3u) {
 let i=id.x;let coverage=array<f32,6>(0.0,0.001,0.1,0.1001,0.34,1.0);
 let encoded=vec3f(0.25,0.5,0.9);let rain=f32((i/6u)%4u)/3.0;
 let glow=f32(i/24u)*0.9333;let ambient=vec3f(0.1,0.2,0.3)*select(1.0,0.0,i>=48u);
 output[i]=bg_reference_rain_color(encoded,coverage[i%6u],rain,glow,ambient);
}"#
    );
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: Some("oracle"),
        compilation_options: Default::default(),
        cache: None,
    });
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &[0; 72 * 16],
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 72 * 16,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 1,
            resource: buffer.as_entire_binding(),
        }],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(72, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&buffer, 0, &read, 0, 72 * 16);
    queue.submit([encoder.finish()]);
    read.slice(..)
        .map_async(wgpu::MapMode::Read, |r| r.unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let bytes = read.slice(..).get_mapped_range().unwrap();
    let rows: &[[f32; 4]] = bytemuck::cast_slice(&bytes);
    for (i, row) in rows.iter().enumerate() {
        let alpha = [0.0f64, 0.001, 0.1, 0.1001, 0.34, 1.0][i % 6];
        let rain = ((i / 6) % 4) as f64 / 3.0;
        let glow = (i / 24) as f64 * 0.9333;
        let rgb = [0.25f64, 0.5, 0.9];
        let length = (rgb.into_iter().map(|v| (v / 3.0).powi(2)).sum::<f64>()).sqrt();
        let expected_alpha = if alpha > 0.1 {
            alpha * 0.35 * rain * length * 1.4
        } else {
            0.0
        };
        let mut expected = [0.0f64; 4];
        expected[3] = expected_alpha;
        for (c, value) in expected[..3].iter_mut().enumerate() {
            let ambient = if i >= 48 { 0.0 } else { [0.1, 0.2, 0.3][c] };
            let block = [255.0f64, 212.0, 160.0][c] * 0.85 / 255.0;
            *value = (rgb[c].sqrt() * (ambient + glow * glow * block * block)).sqrt();
        }
        for (actual, expected) in row.iter().zip(expected) {
            assert!(
                (f64::from(*actual) - expected).abs() < 2e-6,
                "case{i} actual{row:?}"
            );
        }
    }
}
