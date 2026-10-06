//! A constant half-resolution reflection must stay constant across a rippled,
//! geometrically flat ocean, including production binary16 depth rounding.
use super::*;

#[test]
fn gpu_water_reflection_reconstruction_does_not_punch_coplanar_holes() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let reflections = Reflections::new(&device, WIDTH, HEIGHT);
    let Some(gpu) = &reflections.gpu else { return };
    let eye = Vec3::new(0.0, 100.0, 1000.0);
    let matrix = glam::camera::rh::proj::directx::perspective(
        5f32.to_radians(),
        WIDTH as f32 / HEIGHT as f32,
        0.1,
        4000.0,
    ) * glam::camera::rh::view::look_at_mat4(eye, Vec3::ZERO, Vec3::Y);
    let mut settings = [0.0f32; 56];
    settings[..16].copy_from_slice(&matrix.inverse().to_cols_array());
    settings[16..32].copy_from_slice(&matrix.to_cols_array());
    settings[32..35].copy_from_slice(&eye.to_array());
    settings[44..46].copy_from_slice(&[WIDTH as f32, HEIGHT as f32]);
    queue.write_buffer(&gpu.uniform, 0, bytemuck::cast_slice(&settings));
    let source = format!(
        "{}\n{}",
        include_str!("../normal.wgsl"),
        r#"
struct Settings { inverse:mat4x4f, projection:mat4x4f, eye:vec4f, horizon:vec4f,
 zenith:vec4f, size:vec4f, sun:vec4f, climate:vec4f };
@group(0) @binding(0) var<uniform> settings:Settings;
@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
 let p=array(vec2f(-1.0,-1.0),vec2f(3.0,-1.0),vec2f(-1.0,3.0));
 return vec4f(p[i],0.0,1.0);
}
struct Out { @location(0) color:vec4f, @location(1) indirect:vec4f,
 @location(2) normal:vec4f, @location(3) response:vec4f };
@fragment fn fs(@builtin(position) pixel:vec4f)->Out {
 let uv=pixel.xy/settings.size.xy;
 let h=settings.inverse*vec4f(uv*vec2f(2.0,-2.0)+vec2f(-1.0,1.0),1.0,1.0);
 let ray=normalize(h.xyz/h.w-settings.eye.xyz);
 let distance=-settings.eye.y/ray.y;
 // The polygon remains horizontal; only the shading normal oscillates.
 let n=normalize(vec3f(0.08*sin(pixel.x*0.3),1.0,0.07*cos(pixel.y*0.2)));
 return Out(vec4f(0.0,0.0,0.0,1.0),vec4f(0.0,0.0,0.0,-2.0),
  vec4f(bg_reflection_oct_encode(n),0.12,distance),vec4f(0.0));
}
"#
    );
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("distant rippled water reconstruction fixture"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let fixture = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &super::super::super::scene_ao::color_targets(
                super::super::super::post::HDR_FORMAT,
                None,
            ),
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    let fixture_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &fixture.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: gpu.uniform.as_entire_binding(),
        }],
    });
    let scene = reflections.response.clone();
    let indirect = super::super::super::scene_ao::create_indirect(&device, WIDTH, HEIGHT);
    let depth = device
        .create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: super::super::super::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default());
    // The unused response attachment is separate from the output scene.
    let response = super::super::super::scene_ao::create_indirect(&device, WIDTH, HEIGHT);
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &super::super::super::scene_ao::attachments(
                &scene,
                &indirect,
                &reflections.normal,
                &response,
                wgpu::Color::BLACK,
            ),
            ..Default::default()
        });
        pass.set_pipeline(&fixture);
        pass.set_bind_group(0, &fixture_group, &[]);
        pass.draw(0..3, 0..1);
    }
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &reflections.targets.delta,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.5,
                        g: 0.25,
                        b: 0.125,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
    }
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &gpu.layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&depth),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&reflections.normal),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&response),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&reflections.targets.pyramid_view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(&reflections.targets.delta),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: gpu.uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::Sampler(&gpu.sampler),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::TextureView(&indirect),
            },
        ],
    });
    // A negative control uses the former shading-normal/fixed-tolerance test.
    // Prove this scene actually catches missing reflection support rather than
    // passing trivially because the fixture never exercises reconstruction.
    let legacy_source = shader_source()
        .replace(
            "let plane=abs(dot(other_center-center,plane_normal));",
            "let plane=abs(dot(other_center-center,n));",
        )
        .replace(
            "*exp(-plane*plane/(tolerance*tolerance))",
            "*exp(-plane*plane/0.0025)",
        );
    let legacy_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("former water plane rejection negative control"),
        source: wgpu::ShaderSource::Wgsl(legacy_source.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&gpu.layout)],
        immediate_size: 0,
    });
    let legacy_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &legacy_shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &legacy_shader,
            entry_point: Some("composite"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: super::super::super::post::HDR_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    draw(
        &mut encoder,
        "former water plane rejection",
        &legacy_pipeline,
        &group,
        &scene,
        wgpu::LoadOp::Load,
    );
    let row = (WIDTH * 8).div_ceil(256) * 256;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(row * HEIGHT * 2),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        scene.texture().as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(HEIGHT),
            },
        },
        wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
    );
    draw(
        &mut encoder,
        "water reconstruction regression",
        &gpu.composite,
        &group,
        &scene,
        wgpu::LoadOp::Clear(wgpu::Color::BLACK),
    );
    encoder.copy_texture_to_buffer(
        scene.texture().as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: u64::from(row * HEIGHT),
                bytes_per_row: Some(row),
                rows_per_image: Some(HEIGHT),
            },
        },
        wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, |r| r.unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let bytes = readback.slice(..).get_mapped_range().unwrap();
    let (legacy, fixed) = bytes.split_at((row * HEIGHT) as usize);
    let holes = legacy
        .chunks_exact(row as usize)
        .flat_map(|r| r[..WIDTH as usize * 8].chunks_exact(8))
        .filter(|pixel| half(bytemuck::cast_slice::<u8, u16>(pixel)[0]) < 0.49)
        .count();
    assert!(
        holes > (WIDTH * HEIGHT / 8) as usize,
        "negative control did not reproduce reflection holes: {holes}"
    );
    println!(
        "distant rippled plane: former reflection holes={holes}/{}; fixed=0",
        WIDTH * HEIGHT
    );
    for (y, row) in fixed.chunks_exact(row as usize).enumerate() {
        for (x, pixel) in row[..WIDTH as usize * 8].chunks_exact(8).enumerate() {
            let values: &[u16] = bytemuck::cast_slice(pixel);
            for (channel, expected) in [0.5f32, 0.25, 0.125].iter().enumerate() {
                assert!(
                    (half(values[channel]) - expected).abs() < 0.0005,
                    "coplanar water reflection lost support at ({x},{y}), channel {channel}: {}",
                    half(values[channel])
                );
            }
        }
    }
}
