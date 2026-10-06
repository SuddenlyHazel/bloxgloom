use super::*;
#[test]
fn reference_lens_production_visibility_validates() {
    let source = format!("{SHADER}\n{}", include_str!("visibility.wgsl"));
    let module = wgpu::naga::front::wgsl::parse_str(&source).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}

#[test]
fn lens_projection_uses_source_clip_z_fov_and_shared_lunar_direction() {
    let eye = glam::Vec3::new(-200.0, 80.0, 35.0);
    let projection =
        glam::camera::rh::proj::opengl::perspective(70.0_f32.to_radians(), 16.0 / 9.0, 0.1, 600.0);
    let view = glam::camera::rh::view::look_at_mat4(eye, eye + glam::Vec3::NEG_Z, glam::Vec3::Y);
    let fix = glam::Mat4::from_cols(
        glam::Vec4::X,
        glam::Vec4::Y,
        glam::Vec4::new(0.0, 0.0, 0.5, 0.0),
        glam::Vec4::new(0.0, 0.0, 0.5, 1.0),
    );
    let mut atmosphere = crate::render::daylight::Atmosphere::at(0);
    atmosphere.sun = glam::Vec3::new(0.2, 0.5, -0.8).normalize();
    let data = frame(fix * projection * view, eye, atmosphere, 1.0 / 60.0, false);
    let source = projection * (atmosphere.sun * 100.0).extend(1.0);
    let light = source.truncate().truncate() / source.z * 0.5;
    assert!((data[0] - light.x).abs() < 1e-5 && (data[1] - light.y).abs() < 1e-5);
    assert_eq!(data[2], -1.0);
    assert!((data[3] - projection.y_axis.y / 1.3737387).abs() < 1e-6);
    atmosphere.sun = -atmosphere.sun;
    atmosphere.moon_phase = 4;
    let moon = frame(fix * projection * view, eye, atmosphere, 1.0 / 60.0, false);
    assert_eq!(moon[2], 1.0);
    assert!((moon[10] - 0.15).abs() < 1e-6);
    assert_eq!(moon[4], 0.0);
    assert_eq!(moon[5], 1.0);
}

#[test]
fn gpu_reference_lens_source_ghost_goldens_and_submitted_visibility() {
    pollster::block_on(async {
        let adapter = wgpu::Instance::default()
            .request_adapter(&Default::default())
            .await
            .unwrap();
        let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
        // Independent double-precision source oracle constants; not engine outputs.
        let expected: [[f64; 3]; 8] = [
            [0.1370190872, 0.0671542737, 0.0492166348],
            [0.8946005961, 0.6162745662, 0.0749923921],
            [0.9175071130, 0.2071636907, 0.3442143986],
            [0.1497539909, 0.2546579715, 0.3500760574],
            [0.3051290727, 0.1790748494, 0.6236825063],
            [0.0122589492, 0.0203802204, 0.0262680276],
            [0.0074996655, 0.0108616530, 0.0136261801],
            [0.0, 0.0, 0.0],
        ];
        let shader=device.create_shader_module(wgpu::ShaderModuleDescriptor {label:Some("source lens independent RGB goldens"),source:wgpu::ShaderSource::Wgsl(format!(r#"{SHADER}
@group(0) @binding(0) var<storage,read_write> output:array<vec4f>;
@compute @workgroup_size(8) fn main(@builtin(global_invocation_id) id:vec3u) {{
    let i=id.x;let uv=array<vec2f,8>(vec2f(0.5),vec2f(0.61,0.445),vec2f(0.414,0.543),vec2f(0.7,0.4),vec2f(0.35,0.6),vec2f(0.5),vec2f(0.5),vec2f(0.5));
    var d=BgReferenceLens(vec4f(0.2,-0.1,-1.0,1.0),vec4f(1.0,1.0,0.0,0.0),vec4f(vec3f(96.0,192.0,255.0)*0.3/255.0,1.0),vec4f(0.0));
    var aspect=16.0/9.0;
    if i==4u {{aspect=1.0;d.optical.w=1.7;}}
    if i==5u||i==6u {{d.optical.z=1.0;}}
    if i==6u {{d.night=vec4f(d.night.rgb*0.5,1.0);}}
    if i==7u {{d.optical=vec4f(0.8,0.2,-1.0,1.0);}}
    output[i]=vec4f(bg_lens_flare(uv[i],aspect,d,1.0),1.0);
}}
"#).into())});
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let out = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 128,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
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
        let read = read_buffer(&device, &queue, encoder, &out, 128);
        for (row, expected) in bytemuck::cast_slice::<u8, [f32; 4]>(&read)
            .iter()
            .zip(expected)
        {
            for (actual, expected) in row[..3].iter().zip(expected) {
                assert!(
                    (f64::from(*actual) - expected).abs() < 3e-5,
                    "{row:?} vs{expected}"
                );
            }
        }
        // Production one-pixel visibility pass, source temporal rate, rain,
        // blocker rejection, and submitted-only history progression.
        let mut lens = Lens::new(&device);
        lens.data = [
            0.0,
            0.0,
            -1.0,
            1.0,
            1.0,
            0.0,
            0.0,
            1.0 / 60.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
        ];
        lens.data[13] = 1.0;
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth = depth.create_view(&Default::default());
        let retained = 2.0_f64.powf(-12.5 / 60.0);
        for (sky, rain, expected, submit) in [
            (1.0, 0.0, 1.0 - retained, false),
            (1.0, 0.0, 1.0 - retained, true),
            (0.0, 0.0, (1.0 - retained) * retained, true),
            (1.0, 1.0, (1.0 - retained) * retained * retained, true),
        ] {
            lens.configured = true;
            lens.data[6] = rain;
            let mut encoder = device.create_command_encoder(&Default::default());
            {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &depth,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(sky),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    ..Default::default()
                });
            }
            lens.prepare(&device, &queue, &mut encoder, Some(&depth), true);
            let actual =
                read_visibility(&device, &queue, encoder, &lens.visibility[1 - lens.index]);
            assert!(
                (f64::from(actual) - expected).abs() < 1e-6,
                "{actual} vs{expected}"
            );
            if submit {
                lens.submitted();
            }
        }
        // Real reference display consumer: after tone, flare mixes toward white,
        // then source gamma/grain/FXAA/presentation. Fixed source oracle at
        // center/aspect1, previous visibility1 isolates the exact factor/order.
        let mut display =
            super::super::ReferenceDisplay::new(&device, 1, 1, wgpu::TextureFormat::Rgba8Unorm);
        display.noise.available = false;
        display.lens.data = [
            0.2, -0.1, -1.0, 1.0, 1.0, 0.0, 0.0, 60.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0,
        ];
        display.lens.configured = true;
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[],
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
        display
            .lens
            .prepare(&device, &queue, &mut encoder, Some(&depth), true);
        queue.submit(Some(encoder.finish()));
        display.lens.submitted();
        display.depth = Some(depth.clone());
        let final_texture =
            super::super::tests::output(&device, 1, 1, wgpu::TextureFormat::Rgba8Unorm);
        let final_view = final_texture.create_view(&Default::default());
        for effects in [true, false] {
            display.lens.configured = true;
            let mut encoder = device.create_command_encoder(&Default::default());
            super::super::tests::clear(
                &mut encoder,
                &display.linear,
                wgpu::Color {
                    r: 0.2,
                    g: 0.2,
                    b: 0.2,
                    a: 1.0,
                },
            );
            display.encode(
                &device,
                &queue,
                &mut encoder,
                &final_view,
                None,
                &display.linear.clone(),
                effects,
            );
            let actual = super::super::tests::read(&device, &queue, encoder, &final_texture);
            let source_center = [
                0.29026874684977166_f64,
                0.06105066725564021,
                0.09292076537664536,
            ];
            let factor = (0.2_f64 * 3.0_f64.sqrt() * 0.25 + 0.25).powi(2);
            for (actual, flare) in actual[..3].iter().zip(source_center) {
                let encoded = if effects {
                    (0.2 + 0.8 * flare * factor).powf(1.0 / 2.2)
                } else {
                    1.055 * 0.2_f64.powf(1.0 / 2.4) - 0.055
                };
                assert!(
                    (f64::from(*actual) - (encoded * 255.0).round()).abs() <= 1.0,
                    "gamma/order effects{effects}: {actual} vs{encoded}"
                );
            }
        }
        lens.reset();
        assert!(!lens.valid);
        let mut encoder = device.create_command_encoder(&Default::default());
        lens.configured = true;
        lens.prepare(&device, &queue, &mut encoder, Some(&depth), false);
        assert_eq!(lens.data[11], 0.0);
        assert!(!lens.resolved);
    });
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
    map(device, queue, encoder, &read)
}
fn map(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    encoder: wgpu::CommandEncoder,
    read: &wgpu::Buffer,
) -> Vec<u8> {
    queue.submit(Some(encoder.finish()));
    let (tx, rx) = std::sync::mpsc::channel();
    read.slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    read.slice(..).get_mapped_range().unwrap().to_vec()
}
fn read_visibility(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    mut encoder: wgpu::CommandEncoder,
    view: &wgpu::TextureView,
) -> f32 {
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 256,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        view.texture().as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &read,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(1),
            },
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    let data = map(device, queue, encoder, &read);
    f32::from_le_bytes(data[..4].try_into().unwrap())
}
