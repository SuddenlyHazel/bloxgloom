use super::*;
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

#[test]
fn particle_medium_shader_validates_without_gpu() {
    let module = wgpu::naga::front::wgsl::parse_str(&format!("{}\n{COMPUTE}", shader())).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}

const COMPUTE: &str = r#"
@group(0) @binding(1) var<storage,read_write> result:array<vec4f>;
@compute @workgroup_size(1) fn sample(@builtin(global_invocation_id) id:vec3u) {
    let i=id.x;
    let sigma=select(0.0,0.0004,i>0u);
    let coverage=select(0.0,1.0,i==3u);
    let height=select(0.0,200.0,i==3u);
    var receiver=vec3f(0.125,height-0.125,100.0);
    if i==2u {receiver.y=100.0;}
    result[i]=vec4f(bg_particle_medium_transmittance(vec3f(0.0,height,0.0),receiver,sigma,vec3f(coverage,0.0,0.0)));
}
"#;

#[test]
fn gpu_particle_extinction_preserves_emission_fallback_and_resolved_background() {
    let instance = wgpu::Instance::default();
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
        return;
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("particle transport production helper regression"),
        source: wgpu::ShaderSource::Wgsl(format!("{}\n{COMPUTE}", shader()).into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &module,
        entry_point: Some("sample"),
        compilation_options: Default::default(),
        cache: None,
    });
    let result = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 1,
            resource: result.as_entire_binding(),
        }],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(4, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&result, 0, &read, 0, 64);
    queue.submit([encoder.finish()]);
    read.slice(..)
        .map_async(wgpu::MapMode::Read, |r| r.unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let mapped = read.slice(..).get_mapped_range().unwrap();
    let values: &[[f32; 4]] = bytemuck::cast_slice(&mapped);
    // Independently evaluated analytic air integrals and a double-precision
    // implementation of the actual 3D cloud density at its 32 sample positions.
    let expected: [f64; 4] = [1.0, 0.960789379103, 0.959546029689, 0.002722630444];
    for (row, expected) in values.iter().zip(expected) {
        assert!(
            (f64::from(row[0]) - expected).abs() < 0.00001,
            "{row:?} vs {expected}"
        );
    }
    let cloud_transmission = values[3][0];
    drop(mapped);
    let fallback = render_particle(&device, &queue, false, false, 0.5);
    assert_eq!(
        fallback[0],
        [2.25, 1.125, 0.5625, 1.0],
        "fallback must preserve original straight-alpha emission exactly"
    );
    let air = render_particle(&device, &queue, true, false, 0.5);
    let cloud = render_particle(&device, &queue, true, true, 0.5);
    let invisible = render_particle(&device, &queue, true, true, 0.0);
    let background = [0.5, 0.25, 0.125, 1.0];
    let emission = [4.0, 2.0, 1.0];
    for (pixels, transmission) in [
        (&fallback, 1.0),
        (&air, expected[1] as f32),
        (&cloud, cloud_transmission),
    ] {
        for (channel, source) in emission.into_iter().enumerate() {
            let expected =
                background[channel] + (source - background[channel]) * 0.5 * transmission;
            assert!(
                (pixels[0][channel] - expected).abs() < 0.004,
                "{pixels:?} vs {expected}"
            );
        }
        assert_eq!(
            pixels[1], background,
            "unrelated resolved scene must not be extinguished again"
        );
    }
    assert_eq!(
        invisible, [background; 2],
        "invisible particle preserves resolved background"
    );
    assert!(
        cloud[0][0] > background[0],
        "fire remains emissive through the medium"
    );
}

fn render_particle(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    transport: bool,
    cloud: bool,
    alpha: f32,
) -> [[f32; 4]; 2] {
    let height = if cloud { 200.0 } else { 0.0 };
    let matrix = Mat4::from_scale(Vec3::new(1.0, 1.0, 0.005))
        * Mat4::from_translation(Vec3::new(0.0, -height, 0.0));
    let mut atmosphere = crate::render::daylight::Atmosphere::at(crate::daylight::INITIAL_MS);
    atmosphere.scene_transport = transport;
    atmosphere.cloud = f32::from(cloud);
    let camera = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(
            &atmosphere.camera_data(matrix, Vec3::new(0.0, height, 0.0)),
        ),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let mut renderer = FireRenderer::new(device, &camera);
    let mut vertices = Vec::new();
    for p in [
        [-1.0, height - 1.0, 100.0],
        [3.0, height - 1.0, 100.0],
        [-1.0, height + 3.0, 100.0],
    ] {
        vertices.extend_from_slice(&p);
        vertices.extend_from_slice(&[0.5, 0.0, 4.0, 2.0, 1.0, alpha]);
    }
    renderer.set_mesh(queue, &vertices);
    let texture = |format| {
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
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
            .create_view(&Default::default())
    };
    let scene = texture(post::HDR_FORMAT);
    let indirect = texture(post::HDR_FORMAT);
    let normal = texture(post::HDR_FORMAT);
    let response = texture(post::HDR_FORMAT);
    let depth = texture(DEPTH_FORMAT);
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 256,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let attachments = crate::render::scene_ao::attachments(
            &scene,
            &indirect,
            &normal,
            &response,
            wgpu::Color {
                r: 0.5,
                g: 0.25,
                b: 0.125,
                a: 1.0,
            },
        );
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("actual post-transport particle composition"),
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
        pass.set_scissor_rect(0, 0, 7, 8);
        renderer.draw(&mut pass);
    }
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: scene.texture(),
            mip_level: 0,
            origin: wgpu::Origin3d { x: 4, y: 4, z: 0 },
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &read,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(1),
            },
        },
        wgpu::Extent3d {
            width: 4,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    read.slice(..)
        .map_async(wgpu::MapMode::Read, |r| r.unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let mapped = read.slice(..).get_mapped_range().unwrap();
    let channels: &[u16] = bytemuck::cast_slice(&mapped);
    [
        std::array::from_fn(|i| half(channels[i])),
        std::array::from_fn(|i| half(channels[12 + i])),
    ]
}

fn half(bits: u16) -> f32 {
    let sign = if bits & 0x8000 == 0 { 1.0 } else { -1.0 };
    let exponent = (bits >> 10) & 31;
    let mantissa = f32::from(bits & 1023) / 1024.0;
    if exponent == 0 {
        sign * mantissa * 2.0f32.powi(-14)
    } else {
        sign * (1.0 + mantissa) * 2.0f32.powi(i32::from(exponent) - 15)
    }
}
