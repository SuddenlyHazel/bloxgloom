//! Actual draw acceptance: bindings unused by opaque shader code still carry
//! resource hazards. Rebind source snapshots each submitted frame and resize.
use crate::render::{self, lod};
use glam::Vec3;
fn mesh() -> lod::Mesh {
    let mut vertices = vec![];
    for (x, fluid) in [(8.0, false), (6.0, true)] {
        for (y, z) in [(-2.0, -2.0), (2.0, -2.0), (2.0, 2.0), (-2.0, 2.0)] {
            let v = lod::vertex::Vertex::new([x, y, z], 0, -1, [0.4, 0.3, 0.25], 15, 0);
            vertices.push(v.material(lod::surface::Surface {
                color: [0.4, 0.3, 0.25, if fluid { 0.7 } else { 1.0 }],
                layer: None,
                sample_texture: false,
                fluid,
                cutout: false,
            }));
        }
    }
    lod::Mesh {
        key: crate::lod::TileKey {
            level: 0,
            x: 0,
            z: 0,
        },
        revision: 1,
        loading: None,
        ray: None,
        vertices,
        indices: vec![0, 2, 1, 0, 3, 2],
        water_indices: vec![4, 6, 5, 4, 7, 6],
        bounds: Some([Vec3::new(6.0, -2.0, -2.0), Vec3::new(8.0, 2.0, 2.0)]),
        coverage: Box::new(lod::coverage::Coverage {
            intervals: vec![vec![]; 1024],
            occupied: vec![vec![]; 1024],
        }),
    }
}
fn target(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("LOD actualdraw acceptance"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default())
}
#[test]
fn gpu_lod_actual_opaque_and_water_draws_rebind_depth_and_reflections_across_frames_and_resize() {
    let adapter =
        pollster::block_on(wgpu::Instance::default().request_adapter(&Default::default())).unwrap();
    let catalog = crate::content::catalog();
    let (device, queue) = pollster::block_on(
        adapter.request_device(&wgpu::DeviceDescriptor {
            required_limits: render::material_device_limits(
                adapter.limits(),
                render::material_texture_layers(catalog) as usize,
            )
            .unwrap(),
            ..Default::default()
        }),
    )
    .unwrap();
    let (pipeline, _, camera_buffer, _, materials) =
        render::create_voxel_pipeline(&device, &queue, render::post::HDR_FORMAT);
    let mut gpu = lod::Gpu::new(&device, render::post::HDR_FORMAT, &pipeline, &materials);
    let mut admitted = mesh();
    let key = admitted.key;
    admitted.ray = Some(crate::render::lod::ray::extract(&admitted).unwrap());
    let original_ray = admitted.ray.as_ref().unwrap().clone();
    gpu.enqueue(admitted).unwrap();
    assert!(
        gpu.ray_targets().is_empty(),
        "unuploaded geometry is not a ray target"
    );
    assert!(gpu.upload(&device) > 0);
    let targets = gpu.ray_targets();
    assert_eq!(targets.len(), 1);
    assert_eq!((targets[0].0, targets[0].1), (key, 1));
    assert!(std::sync::Arc::ptr_eq(&targets[0].2, &original_ray));
    let mut water = render::water::WaterRenderer::new(&device, &camera_buffer);
    let shader=device.create_shader_module(wgpu::ShaderModuleDescriptor {label:Some("actualdepth and refreshedreflection readback"),source:wgpu::ShaderSource::Wgsl(r#"
@group(0) @binding(0) var original:texture_depth_2d;@group(0) @binding(1) var front:texture_depth_2d;@group(0) @binding(2) var reflection:texture_2d<f32>;@group(0) @binding(3) var rebound:texture_depth_2d;@group(0) @binding(4) var<storage,read_write> result:array<vec4f>;
@compute @workgroup_size(1) fn read(){let center=vec2i(textureDimensions(original)/2u);result[0]=vec4f(textureLoad(original,center,0),textureLoad(front,center,0),textureLoad(rebound,center,0),1.0);result[1]=textureLoad(reflection,vec2i(0),0);}
"#.into())});
    let read_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: Some("read"),
        compilation_options: Default::default(),
        cache: None,
    });
    let receiver_shader=device.create_shader_module(wgpu::ShaderModuleDescriptor {label:Some("actual LOD receiver MRT"),source:wgpu::ShaderSource::Wgsl(r#"
@group(0) @binding(0) var normal:texture_2d<f32>;@group(0) @binding(1) var indirect:texture_2d<f32>;@group(0) @binding(2) var response:texture_2d<f32>;@group(0) @binding(3) var<storage,read_write> result:array<vec4f>;
@compute @workgroup_size(1) fn inspect(){let p=vec2i(textureDimensions(normal)/2u);result[2]=textureLoad(normal,p,0);result[3]=textureLoad(indirect,p,0);result[4]=textureLoad(response,p,0);}
"#.into())});
    let receiver_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &receiver_shader,
        entry_point: Some("inspect"),
        compilation_options: Default::default(),
        cache: None,
    });
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 80,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 80,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    for (frame, (width, height)) in [(32, 24), (32, 24), (48, 32)].into_iter().enumerate() {
        let camera = render::Camera {
            position: Vec3::new(frame as f32 * 0.25, 0.0, 0.0),
            yaw: 0.0,
            pitch: 0.0,
            fov_y_radians: 1.0,
        };
        let atmosphere = render::daylight::Atmosphere::at(crate::daylight::INITIAL_MS);
        let matrix = render::visibility::view_projection(camera, width, height);
        let scene = target(&device, width, height, render::post::HDR_FORMAT);
        let indirect = target(&device, width, height, render::scene_ao::INDIRECT_FORMAT);
        let normal = target(&device, width, height, render::scene_ao::INDIRECT_FORMAT);
        let response = target(&device, width, height, render::scene_ao::INDIRECT_FORMAT);
        let depth = target(&device, width, height, render::DEPTH_FORMAT);
        let clear = [0.125 + frame as f64 * 0.125, 0.25, 0.5];
        let mut encoder = device.create_command_encoder(&Default::default());
        gpu.prepare(
            &queue,
            camera,
            width,
            height,
            atmosphere,
            std::iter::empty(),
        );
        // First opaque image supplies a genuine depth/color snapshot.
        {
            let attachments = render::scene_ao::attachments(
                &scene,
                &indirect,
                &normal,
                &response,
                wgpu::Color {
                    r: clear[0],
                    g: clear[1],
                    b: clear[2],
                    a: 1.0,
                },
            );
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("initial realLOD opaque snapshot"),
                color_attachments: &attachments,
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
            assert_eq!(gpu.draw(&mut pass), 2);
        }
        // Capture the opaque receiver before the translucent water MRT blend.
        let receiver_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &receiver_pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&normal),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&indirect),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&response),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: output.as_entire_binding(),
                },
            ],
        });
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&receiver_pipeline);
            pass.set_bind_group(0, &receiver_group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        water.prepare_frame(
            &queue,
            render::water::Frame {
                atmosphere,
                eye_in_water: false,
                sample: frame as u32,
                camera,
                size: [width, height],
                view_projection: matrix,
            },
        );
        let water_scene = water.begin_frame(&device, &mut encoder, &scene, &depth);
        gpu.set_reference_water_inputs(&device, water.reference_inputs());
        gpu.set_optical_water_inputs(&device, water.optical_inputs());
        // The aggregate group is rebound before the next opaque pass. Even
        // unused original-depth bindings must never accompany attachment WRITE.
        {
            let mut attachments = render::scene_ao::attachments(
                &scene,
                &indirect,
                &normal,
                &response,
                wgpu::Color::BLACK,
            );
            for a in attachments.iter_mut().flatten() {
                a.ops.load = wgpu::LoadOp::Load;
            }
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("realLOD opaque afterwater bindings"),
                color_attachments: &attachments,
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            assert_eq!(gpu.draw(&mut pass), 2);
        }
        let front = water.reference_front_depth().unwrap_or(&depth);
        assert_eq!(front.texture().width(), width);
        assert_eq!(front.texture().height(), height);
        assert_eq!(
            gpu.draw_water_pass(
                &mut encoder,
                &water_scene,
                &indirect,
                &normal,
                &response,
                front
            ),
            2
        );
        water.finish_frame(&device, &mut encoder, &scene);
        let inputs = water.reference_inputs();
        let reflection = inputs.map_or(&scene, |i| &i.reflection);
        let rebound = inputs.map_or(&depth, |i| &i.opaque_depth);
        assert_eq!(reflection.texture().width(), width);
        let texture = |binding, view| wgpu::BindGroupEntry {
            binding,
            resource: wgpu::BindingResource::TextureView(view),
        };
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &read_pipeline.get_bind_group_layout(0),
            entries: &[
                texture(0, &depth),
                texture(1, front),
                texture(2, reflection),
                texture(3, rebound),
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: output.as_entire_binding(),
                },
            ],
        });
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&read_pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &read, 0, 80);
        queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        read.slice(..)
            .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        rx.recv().unwrap().unwrap();
        let pixels =
            bytemuck::cast_slice::<u8, [f32; 4]>(&read.slice(..).get_mapped_range().unwrap())
                .to_vec();
        read.unmap();
        assert!(
            pixels[0][0] < 1.0,
            "actualopaque pixels mustwrite, notjustnonemptydrawcalls: {pixels:?}"
        );
        assert_eq!(
            pixels[0][0], pixels[0][2],
            "originaldepth source bindingmustrefresh everyframe"
        );
        if render::bsl_reference::enabled() {
            assert!(
                pixels[0][1] < pixels[0][0],
                "actualfluid mustwriteprivate nearestdepth: {pixels:?}"
            );
            for (actual, color) in pixels[1][..3].iter().zip(clear) {
                let encoded = (color.powf(0.125) * 0.5 * 1023.0).round() / 1023.0;
                assert!(
                    (f64::from(*actual) - encoded).abs() < 0.002,
                    "snapshot mustrefresh aftersubmit/resize: {pixels:?}"
                );
            }
        } else {
            assert_eq!(
                pixels[2][2], 1.0,
                "non-PBR coarse terrain must be a diffuse GI receiver: {pixels:?}"
            );
            assert!(
                (pixels[2][3] - (8.0 - camera.position.x)).abs() < 0.03,
                "receiver must carry actual world distance: {pixels:?}"
            );
            assert!(
                pixels[3][..3].iter().all(|v| *v > 0.0),
                "retained ambient must match the material baseline: {pixels:?}"
            );
            assert_eq!(pixels[3][3], 1.0);
            assert_eq!(
                pixels[4][..3],
                [0.0; 3],
                "absent PBR must not invent a specular fallback"
            );
            assert_eq!(pixels[4][3], 1.0);
            assert_eq!(
                pixels[0][0], pixels[0][1],
                "enhancedwater leavesopaquedepth unchanged"
            );
        }
    }
    // Selection exposure is independent of frustum preparation and follows
    // admission/removal, rather than a queued replacement's revision.
    gpu.set_horizon(0);
    assert!(gpu.ray_targets().is_empty());
    gpu.set_horizon(512);
    assert_eq!(gpu.ray_targets().len(), 1);
    let mut replacement = mesh();
    replacement.revision = 2;
    replacement.ray = Some(crate::render::lod::ray::extract(&replacement).unwrap());
    let new_ray = replacement.ray.as_ref().unwrap().clone();
    gpu.enqueue(replacement).unwrap();
    assert_eq!(gpu.ray_targets()[0].1, 1);
    assert!(gpu.upload(&device) > 0);
    let updated = gpu.ray_targets();
    assert_eq!(updated[0].1, 2);
    assert!(std::sync::Arc::ptr_eq(&updated[0].2, &new_ray));
    gpu.remove(key);
    assert!(gpu.ray_targets().is_empty());
    let mut reinstated = mesh();
    reinstated.ray = Some(crate::render::lod::ray::extract(&reinstated).unwrap());
    gpu.enqueue(reinstated).unwrap();
    assert!(gpu.upload(&device) > 0);
    assert_eq!(gpu.ray_targets().len(), 1);
    gpu.clear();
    assert!(gpu.ray_targets().is_empty());
}
