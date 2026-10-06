use super::*;

#[test]
fn atmosphere_shader_validates_without_a_gpu() {
    let module = wgpu::naga::front::wgsl::parse_str(include_str!("../atmosphere.wgsl")).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}

#[test]
fn gpu_occluded_air_and_disabled_shadow_map_add_no_exterior_light() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    if !super::super::post::temporal::supported(&device) {
        return;
    }
    let width = 7;
    let height = 5;
    let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 208,
        usage: wgpu::BufferUsages::UNIFORM,
        mapped_at_creation: false,
    });
    let mut atmosphere = super::super::daylight::Atmosphere::at(crate::daylight::INITIAL_MS);
    atmosphere.sun = Vec3::Y;
    let camera = super::super::Camera {
        position: Vec3::ZERO,
        yaw: 0.0,
        pitch: 0.0,
        fov_y_radians: 1.0,
    };
    let mut shadows = super::super::sun_shadow::SunShadows::new(
        &device,
        &camera_buffer,
        crate::config::SunShadowQuality::Medium,
    );
    shadows.update(&queue, camera, atmosphere);
    let mut pass = AtmospherePass::new(&device, width, height);
    pass.resize(&device, height, width);
    pass.resize(&device, width, height);
    // Tests use an explicit density, independent of preview tuning environment.
    pass.density = 0.0018;
    let make = |format| {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width,
                    height,
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
    let scene = make(super::super::post::HDR_FORMAT);
    let depth = make(super::super::DEPTH_FORMAT);
    let surface = make(super::super::post::HDR_FORMAT);
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 256 * 3,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let matrix = Mat4::from_scale(Vec3::new(1.0, 1.0, 1.0 / 80.0));
    let mut captures = Vec::new();
    for (shadow_depth, receiving_distance, roughness) in [
        (0.0, 0.0, 0.0),
        (1.0, 0.0, 0.0),
        (1.0, 2.0, 0.12),
        (1.0, 80.0, 0.12),
        (1.0, 2.0, 0.0),
    ] {
        let mut encoder = device.create_command_encoder(&Default::default());
        for (view, clear) in [(&depth, 0.5), (&shadows.view, shadow_depth)] {
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("atmosphere regression depth"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
        }
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("zero surface metadata"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &surface,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.0,
                        g: 0.0,
                        b: roughness,
                        a: receiving_distance,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &scene,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.resolve(
            &device,
            &queue,
            &mut encoder,
            &scene,
            &depth,
            &surface,
            matrix,
            Vec3::ZERO,
            atmosphere,
            &shadows,
        );
        let size = pass.scattering.texture().size();
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: pass.scattering.texture(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(size.height),
                },
            },
            size,
        );
        queue.submit([encoder.finish()]);
        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |r| r.unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let bytes = slice.get_mapped_range().unwrap();
        captures.push(bytes.to_vec());
        drop(bytes);
        readback.unmap();
    }
    assert!(
        captures[0].iter().all(|b| *b == 0),
        "blocked air introduced exterior light"
    );
    assert!(
        captures[1].iter().any(|b| *b != 0),
        "lit air did not scatter sunlight"
    );
    assert!(
        captures[2].iter().all(|b| *b == 0),
        "air scattered beyond the near water surface"
    );
    assert_eq!(
        captures[3], captures[1],
        "receiver behind opaque wall extended the air ray"
    );
    assert_eq!(
        captures[4], captures[1],
        "invalid receiver metadata replaced opaque depth"
    );
    // Disabling the map must skip rather than use its preceding lit contents.
    shadows.projection.enabled = false;
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: None,
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &pass.scattering,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    });
    pass.resolve(
        &device,
        &queue,
        &mut encoder,
        &scene,
        &depth,
        &surface,
        matrix,
        Vec3::ZERO,
        atmosphere,
        &shadows,
    );
    let size = pass.scattering.texture().size();
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: pass.scattering.texture(),
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(size.height),
            },
        },
        size,
    );
    queue.submit([encoder.finish()]);
    let slice = readback.slice(..);
    slice.map_async(wgpu::MapMode::Read, |r| r.unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let bytes = slice.get_mapped_range().unwrap();
    assert!(
        bytes.iter().all(|b| *b == 0),
        "disabled map reused stale sunlight"
    );
}
