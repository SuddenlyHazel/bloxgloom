use super::*;
use crate::render::trace::scene::Triangle;
use wgpu::util::DeviceExt;

#[test]
fn wind_compute_matches_independent_caster_positions_and_preserves_material_fields() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 5,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let flags = [0u32, 64, 64 | 8, 64 | 8 | 128, 64];
    let metadata: Vec<_> = flags.into_iter().map(|f| [f, 0]).collect();
    let metadata_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&metadata),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let materials = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 5,
            resource: metadata_buffer.as_entire_binding(),
        }],
    });
    let triangles: Vec<_> = (0..5)
        .map(|id| Triangle {
            a: [-9.0, 33.0, 4.0, id as f32],
            b: [-8.0, 33.0, 4.0, 15.0],
            c: [-9.0, 34.0, 4.0, 1.0],
            uv_ab: [0.0, 1.0, 1.0, 1.0],
            uv_c: [0.0; 2],
            surface_color: 0,
            surface_flags: 0,
            normal: if id == 1 {
                [0.0, 0.0, 1.0, 0.0]
            } else if id == 4 {
                // The slanted cap cards must sway as leaves even though their
                // normals are almost vertical. They are never plant roots.
                glam::Vec3::new(0.0, 1.0, 0.14)
                    .normalize()
                    .extend(0.0)
                    .to_array()
            } else {
                [0.0, 1.0, 0.0, 0.0]
            },
        })
        .collect();
    let source = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&triangles),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: source.size(),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let mut frame = [0.0f32; 36];
    frame[35] = 17.25;
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&frame),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: source.size(),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let deformation = Deformation::new(&device, &layout);
    let mut encoder = device.create_command_encoder(&Default::default());
    deformation.encode(
        &device,
        &mut encoder,
        &uniform,
        &source,
        &output,
        &materials,
        None,
    );
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, source.size());
    queue.submit([encoder.finish()]);
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, |result| result.unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let bytes = readback.slice(..).get_mapped_range().unwrap();
    let result: &[Triangle] = bytemuck::cast_slice(&bytes);
    for (id, (before, after)) in triangles.iter().zip(result).enumerate() {
        assert_eq!(after.uv_ab, before.uv_ab);
        assert_eq!(after.uv_c, before.uv_c);
        assert_eq!(after.normal, before.normal);
        for (p, actual, uv) in [
            (before.a, after.a, [0.0, 1.0]),
            (before.b, after.b, [1.0, 1.0]),
            (before.c, after.c, [0.0, 0.0]),
        ] {
            assert_eq!(p[3], actual[3], "material/sky/cutout identity is unchanged");
            let expected = expected_wind(p, before.normal, uv, flags[id], frame[35]);
            for axis in 0..3 {
                assert!(
                    (actual[axis] - expected[axis]).abs() < 0.00002,
                    "triangle {id} axis {axis}: {actual:?} vs {expected:?}"
                );
            }
        }
    }
}

fn expected_wind(p: [f32; 4], n: [f32; 4], uv: [f32; 2], flags: u32, seconds: f32) -> [f32; 3] {
    if flags & 64 == 0 {
        return [p[0], p[1], p[2]];
    }
    let mut v = uv[1];
    if flags & 8 != 0 {
        v = if flags & 128 != 0 {
            v * 0.5
        } else {
            0.5 + v * 0.5
        };
    }
    let phase = p[0] * 0.48 + p[2] * 0.31;
    let time = seconds * (std::f32::consts::TAU / 128.0);
    let gust = (time * 17.0 + phase).sin() * 0.72 + (time * 29.0 + phase * 1.37).sin() * 0.28;
    let amplitude = if n[1] > 0.9999 {
        0.11 * (1.0 - v).clamp(0.0, 1.0).powi(2)
    } else {
        0.035
    };
    [
        p[0] + gust * amplitude,
        p[1] + 0.12 * gust * amplitude,
        p[2] + (time * 17.0 + phase + 1.2).sin() * 0.65 * amplitude,
    ]
}
