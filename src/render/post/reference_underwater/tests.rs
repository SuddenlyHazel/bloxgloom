use super::*;
#[test]
fn source_underwater_fog_and_distorted_composite_validate() {
    for source in [FOG.to_owned(), composite_source()] {
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
fn source_default_water_fog_palette_goldens_keep_eye_brightness_explicit() {
    let mut atmosphere = crate::render::daylight::Atmosphere::at(0);
    atmosphere.sun = glam::Vec3::Y;
    atmosphere.time_angle = 0.25;
    atmosphere.rain_strength = 0.0;
    atmosphere.reference_shadow_fade = 1.0;
    for (exposure, expected) in [
        (
            1.0,
            [
                0.0023469953419796162_f64,
                0.01631552912753962,
                0.047598576343318676,
            ],
        ),
        (
            0.25,
            [
                0.0007211397674304535,
                0.004912159291395808,
                0.01402371969650598,
            ],
        ),
        (
            0.0,
            [
                0.00017914082513612777,
                0.0011196301571007984,
                0.00284390433458904,
            ],
        ),
    ] {
        atmosphere.fog_exposure = exposure;
        let actual = fog_color(atmosphere);
        for (actual, expected) in actual.to_array().into_iter().zip(expected) {
            assert!((f64::from(actual) - expected).abs() < 1e-8);
        }
    }
}
#[test]
fn gpu_reference_underwater_source_fog_distortion_and_actual_front_depth() {
    pollster::block_on(async {
        let adapter = wgpu::Instance::default()
            .request_adapter(&Default::default())
            .await
            .unwrap();
        let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
        let shader=device.create_shader_module(wgpu::ShaderModuleDescriptor {label:Some("source independent underwater fog goldens"),source:wgpu::ShaderSource::Wgsl(format!(r#"{FOG}
 @group(0) @binding(4) var<storage,read_write> output:array<vec4f>;
 @compute @workgroup_size(8) fn math(@builtin(global_invocation_id) id:vec3u) {{
 if id.x>=6u {{return;}}
 let distances=array<f32,6>(0.0,1.0,8.0,32.0,64.0,128.0);
 output[id.x]=vec4f(bg_reference_underwater(vec3f(0.25,0.5,1.0),distances[id.x],vec3f(0.0023469953,0.016315529,0.047598576)),1.0);
 }}"#).into())});
        let out = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 96,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: None,
            module: &shader,
            entry_point: Some("math"),
            compilation_options: Default::default(),
            cache: None,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 4,
                resource: out.as_entire_binding(),
            }],
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        let actual = read_buffer(&device, &queue, encoder, &out, 96);
        let expected = [
            [
                0.11693392596998489_f64,
                0.2543444088972942,
                0.5275986213239097,
            ],
            [0.11083950096432511, 0.24279185528353509, 0.5051293365993511],
            [0.07674661597633729, 0.1772608067974601, 0.37693320792520646],
            [
                0.024467897724589488,
                0.07090143859391768,
                0.16412481903383247,
            ],
            [
                0.007773611788861945,
                0.03193320240560325,
                0.08233850238910527,
            ],
            [
                0.0028967673593519767,
                0.01812518442432776,
                0.05174659685852456,
            ],
        ];
        for (row, expected) in bytemuck::cast_slice::<u8, [f32; 4]>(&actual)
            .iter()
            .zip(expected)
        {
            for (actual, expected) in row[..3].iter().zip(expected) {
                assert!((f64::from(*actual) - expected).abs() < 1e-6);
            }
        }
        // Distortion source uses GL UV/time with strict boundary rejection. Storage
        // output tests exact double-precision reference equations, including corners.
        let shader=device.create_shader_module(wgpu::ShaderModuleDescriptor {label:Some("source underwater distortion goldens"),source:wgpu::ShaderSource::Wgsl(format!(r#"{DISTORTION}
 @group(0) @binding(5) var<storage,read_write> output:array<vec4f>;
 @compute @workgroup_size(8) fn distort(@builtin(global_invocation_id) id:vec3u) {{
 let uv=array<vec2f,8>(vec2f(0.5),vec2f(0.2,0.8),vec2f(0.0,0.5),vec2f(1.0,0.5),vec2f(0.5,0.0),vec2f(0.5,1.0),vec2f(0.99999,0.3),vec2f(0.3,0.00001));
 output[id.x]=vec4f(bg_reference_underwater_uv(uv[id.x]),0.0,1.0);
 }}"#).into())});
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&[0.0_f32; 28]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let mut data = [0.0_f32; 28];
        data[24] = 1.5;
        queue.write_buffer(&uniform, 0, bytemuck::cast_slice(&data));
        let out = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 128,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: None,
            module: &shader,
            entry_point: Some("distort"),
            compilation_options: Default::default(),
            cache: None,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: out.as_entire_binding(),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        let actual = read_buffer(&device, &queue, encoder, &out, 128);
        for (row, [x, y]) in bytemuck::cast_slice::<u8, [f32; 4]>(&actual).iter().zip([
            [0.5_f64, 0.5],
            [0.2, 0.8],
            [0.0, 0.5],
            [1.0, 0.5],
            [0.5, 0.0],
            [0.5, 1.0],
            [0.99999, 0.3],
            [0.3, 0.00001],
        ]) {
            let gl_y = 1.0 - y;
            let dx = x + (gl_y * 32.0 + 4.5).cos() * 0.0005;
            let dy = gl_y + (x * 32.0 + 2.55).sin() * 0.0005;
            let expected = if dx > 0.0 && dx < 1.0 && dy > 0.0 && dy < 1.0 {
                [dx, 1.0 - dy]
            } else {
                [x, y]
            };
            for (actual, expected) in row[..2].iter().zip(expected) {
                assert!(
                    (f64::from(*actual) - expected).abs() < 1e-7,
                    "{row:?} vs{expected}"
                );
            }
        }
        // Real production depth reconstruction and HDR fog/copyback. No supplied
        // front depth must remain exact; depth.5 transformed128 => surface64m.
        let mut underwater = Underwater::new(&device, 1, 1);
        underwater.water = true;
        underwater.data[..16].copy_from_slice(
            &glam::Mat4::from_scale(glam::Vec3::new(1.0, 1.0, 128.0)).to_cols_array(),
        );
        underwater.data[20..23].copy_from_slice(&[0.0023469953, 0.016_315_53, 0.047598576]);
        let scene = texture(&device, HDR_FORMAT);
        let scene_view = scene.create_view(&Default::default());
        let depth = texture(&device, wgpu::TextureFormat::Depth32Float);
        let depth_view = depth.create_view(&Default::default());
        for (front, enabled, expected) in [
            (false, true, [0.25_f64, 0.5, 1.0]),
            (
                true,
                true,
                [
                    0.007773611788861945,
                    0.03193320240560325,
                    0.08233850238910527,
                ],
            ),
            (true, false, [0.25, 0.5, 1.0]),
        ] {
            underwater.depth = front.then(|| depth_view.clone());
            let mut encoder = device.create_command_encoder(&Default::default());
            {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &scene_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.25,
                                g: 0.5,
                                b: 1.0,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(0.5),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    ..Default::default()
                });
            }
            underwater.resolve(&device, &queue, &mut encoder, &scene_view, enabled);
            let row = read_color(&device, &queue, encoder, &scene_view);
            for (actual, expected) in row[..3].iter().zip(expected) {
                assert!(
                    (f64::from(*actual) - expected).abs() < 1e-5 + expected * 0.003,
                    "{row:?} vs{expected}"
                );
            }
        }
        // Actual reference composite draws with both scene and atlas bound:
        // constant gray reduces independent source tone/bloom to scalar math.
        let bloom = texture(&device, HDR_FORMAT);
        let bloom = bloom.create_view(&Default::default());
        let mapped = texture(&device, HDR_FORMAT);
        let mapped = mapped.create_view(&Default::default());
        let settings = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&[1.0_f32, 1.0, 0.0, 1.0]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        for strength in [0.0_f32, 1.0] {
            queue.write_buffer(
                &settings,
                0,
                bytemuck::cast_slice(&[1.0_f32, strength, 0.0, 1.0]),
            );
            let mut encoder = device.create_command_encoder(&Default::default());
            for view in [&scene_view, &bloom] {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.25,
                                g: 0.25,
                                b: 0.25,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
            }
            assert!(underwater.composite(
                &device,
                &mut encoder,
                &scene_view,
                &bloom,
                &settings,
                &mapped
            ));
            let row = read_color(&device, &queue, encoder, &mapped);
            let scene = if strength > 0.0 { 0.225_f64 } else { 0.25 };
            let exposed = scene
                * if crate::render::sky::style_enabled() {
                    4.0
                } else {
                    1.0
                };
            let expected = exposed / (exposed * exposed + 1.0).sqrt();
            for actual in &row[..3] {
                assert!(
                    (f64::from(*actual) - expected).abs() < 0.001,
                    "{row:?} vs{expected}"
                );
            }
        }
    });
}
fn texture(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}
fn read_buffer(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    mut encoder: wgpu::CommandEncoder,
    source: &wgpu::Buffer,
    size: u64,
) -> Vec<u8> {
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    encoder.copy_buffer_to_buffer(source, 0, &read, 0, size);
    queue.submit(Some(encoder.finish()));
    let (tx, rx) = std::sync::mpsc::channel();
    read.slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    read.slice(..).get_mapped_range().unwrap().to_vec()
}
fn read_color(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    mut encoder: wgpu::CommandEncoder,
    source: &wgpu::TextureView,
) -> [f32; 4] {
    let shader=device.create_shader_module(wgpu::ShaderModuleDescriptor {label:None,source:wgpu::ShaderSource::Wgsl("@group(0) @binding(0) var input:texture_2d<f32>; @group(0) @binding(1) var<storage,read_write> output:array<vec4f>; @compute @workgroup_size(1) fn read(){output[0]=textureLoad(input,vec2i(0),0);}".into())});
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: Some("read"),
        compilation_options: Default::default(),
        cache: None,
    });
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 16,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            super::super::reference_display::texture_entry(0, source),
            wgpu::BindGroupEntry {
                binding: 1,
                resource: output.as_entire_binding(),
            },
        ],
    });
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(1, 1, 1);
    }
    let data = read_buffer(device, queue, encoder, &output, 16);
    *bytemuck::from_bytes(&data)
}
