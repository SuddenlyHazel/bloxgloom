//! Source WATER_NORMALS=1 against independent bilinear texture sampling.
use glam::{Vec2, Vec3};
use wgpu::util::DeviceExt;

fn green(x: i32, y: i32) -> f32 {
    ((x.rem_euclid(512) * 13 + y.rem_euclid(512) * 7) % 256) as f32 / 255.0
}
fn sample(uv: Vec2) -> f32 {
    let p = uv.fract() * 512.0 - Vec2::splat(0.5);
    let a = p.floor();
    let f = p - a;
    let x = a.x as i32;
    let y = a.y as i32;
    let lo = green(x, y) * (1.0 - f.x) + green(x + 1, y) * f.x;
    let hi = green(x, y + 1) * (1.0 - f.x) + green(x + 1, y + 1) * f.x;
    lo * (1.0 - f.y) + hi * f.y
}
fn height(world: Vec3, offset: Vec2, time: f32) -> f32 {
    let coordinate = Vec2::new(world.x, world.z) + Vec2::splat(world.y * 0.2);
    let wind = Vec2::splat(time * 0.5);
    sample((coordinate - wind) / 256.0 + offset / 256.0) * 0.75
        + sample((coordinate + wind) / 48.0 + offset / 256.0) * 0.25
}
fn normal(world: Vec3, relative: Vec3, time: f32) -> Vec3 {
    let view = Vec2::new(relative.x, relative.z) / relative.length();
    let mut position = world;
    for _ in 0..4 {
        let h = -1.25 * height(position, Vec2::ZERO, time) + 0.25;
        position.x += h * view.x;
        position.z += h * view.y;
    }
    let x = (height(position, Vec2::new(-0.2, 0.0), time)
        - height(position, Vec2::new(0.2, 0.0), time))
        / 0.2;
    let y = (height(position, Vec2::new(0.0, -0.2), time)
        - height(position, Vec2::new(0.0, 0.2), time))
        / 0.2;
    let strength = 0.35
        * (1.0
            - (1.0 + relative.y / relative.length())
                .clamp(0.0, 1.0)
                .powi(8));
    Vec3::new(x * strength, 1.0 - (x * x + y * y) * strength, y * strength).normalize()
}

#[test]
fn gpu_source_water_noise_channel_uv_wind_parallax_and_grazing_normals_match() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("synthetic independently sampled source noise"),
        size: wgpu::Extent3d {
            width: 512,
            height: 512,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let pixels = (0..512)
        .flat_map(|y| (0..512).flat_map(move |x| [211u8, ((x * 13 + y * 7) % 256) as u8, 19, 255]))
        .collect::<Vec<_>>();
    queue.write_texture(
        texture.as_image_copy(),
        &pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(512 * 4),
            rows_per_image: Some(512),
        },
        texture.size(),
    );
    let view = texture.create_view(&Default::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let sky = include_str!("../../sky/camera.wgsl")
        .split("@group")
        .next()
        .unwrap();
    let surface = include_str!("surface.wgsl")
        .split("fn bg_reference_water_surface")
        .next()
        .unwrap();
    let source = format!(
        "{sky}\nstruct WaterReferenceFrame {{sky:SkyCamera,properties:vec4f}};\n@group(0) @binding(0) var bg_reference_noise:texture_2d<f32>;\n@group(0) @binding(1) var bg_reference_noise_sampler:sampler;\n@group(0) @binding(2) var<uniform> water_reference:WaterReferenceFrame;\n@group(0) @binding(3) var<storage,read_write> result:array<vec4f>;\n{surface}\n{}",
        r#"
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id:vec3u) {
 let i=f32(id.x);let world=vec3f(-20.25+i*7.37,24.0+i*0.12,38.75-i*13.19);
 var relative=vec3f(3.0,-8.0,7.0);if id.x>=4u {relative.y=-0.02;}
 result[id.x]=vec4f(bg_reference_water_height(world,vec2f(0.2,-0.2)),bg_reference_water_normal(world,relative,vec3f(0.0,1.0,0.0)));
}"#
    );
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("source water sampling oracle"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 8 * 16,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: output.size(),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    for time in [0.0, 17.35, 96.0] {
        let mut data = [0.0f32; 44];
        data[35] = time;
        data[40] = 1.0;
        let frame = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&data),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: frame.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: output.as_entire_binding(),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(8, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, output.size());
        queue.submit([encoder.finish()]);
        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |r| r.unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        {
            let bytes = slice.get_mapped_range().unwrap();
            let values: &[[f32; 4]] = bytemuck::cast_slice(&bytes);
            for (i, value) in values.iter().enumerate() {
                let k = i as f32;
                let world = Vec3::new(-20.25 + k * 7.37, 24.0 + k * 0.12, 38.75 - k * 13.19);
                let relative = Vec3::new(3.0, if i >= 4 { -0.02 } else { -8.0 }, 7.0);
                let expected = height(world, Vec2::new(0.2, -0.2), time);
                assert!(
                    (value[0] - expected).abs() < 0.001,
                    "noise channel/UV/wind i={i} t={time}: GPU={}, source={expected}",
                    value[0]
                );
                let expected = normal(world, relative, time);
                let actual = Vec3::from_slice(&value[1..]);
                assert!(
                    actual.distance(expected) < 0.008,
                    "source four-step parallax and grazing normal i={i} t={time}: GPU={actual:?}, source={expected:?}"
                );
            }
        }
        readback.unmap();
    }
}
