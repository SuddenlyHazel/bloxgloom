use super::{ORACLE, SHADER, source_cpu};
use wgpu::util::DeviceExt;

#[test]
fn gpu_reference_celestial_default_matches_source_and_f64_oracle() {
    let adapter =
        pollster::block_on(wgpu::Instance::default().request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    // Alpha, dark encoded channels, horizon crossing, phase and dawn all have
    // independent coverage; alpha multiplication is after the RGB power.
    let mut queries = Vec::<[f32; 8]>::new();
    for up in [
        -1.0,
        -70.0 / 128.0,
        -68.0 / 128.0,
        -66.0 / 128.0,
        -64.0 / 128.0,
        -62.0 / 128.0,
        -0.03,
        -0.025641026,
        0.0,
        0.001,
        0.02,
        0.1,
        0.5,
        1.0,
    ] {
        for visibility in [0.0, 0.25, 0.5, 1.0] {
            for phase in [0.5, 0.625, 0.75, 0.875, 1.0] {
                for alpha in [0.0, 0.2, 1.0] {
                    for moon in [0.0, 1.0] {
                        queries.push([0.04, 0.37, 0.91, alpha, up, moon, visibility, phase]);
                    }
                }
            }
        }
    }
    let source = format!(
        r#"{SHADER}
{ORACLE}
struct Query {{sampled:vec4f,parameters:vec4f}};
struct Result {{current:vec4f,source:vec4f}};
@group(0) @binding(0) var<storage,read> queries:array<Query>;
@group(0) @binding(1) var<storage,read_write> results:array<Result>;
@compute @workgroup_size(64) fn main(@builtin(global_invocation_id) id:vec3u) {{
 if id.x>=arrayLength(&queries) {{return;}}
 let q=queries[id.x];let p=q.parameters;
 results[id.x]=Result(vec4f(bg_bsl_textured_celestial(q.sampled,p.x,p.y>0.5,p.z,p.w),bg_bsl_reference_star_fade(p.x*128.0,true)),vec4f(source_celestial(q.sampled,p.x,p.y>0.5,p.z,p.w),bg_bsl_reference_star_fade(p.x*128.0,false)));
}}"#
    );
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("BSL textured celestial independent oracle"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&queries),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let bytes = (queries.len() * 32) as u64;
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: bytes,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: bytes,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: input.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: output.as_entire_binding(),
            },
        ],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups((queries.len() as u32).div_ceil(64), 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &read, 0, bytes);
    queue.submit([encoder.finish()]);
    read.slice(..)
        .map_async(wgpu::MapMode::Read, |result| result.unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let mapped = read.slice(..).get_mapped_range().unwrap();
    let values: &[[f32; 8]] = bytemuck::cast_slice(&mapped);
    for (q, result) in queries.iter().zip(values) {
        let source_bedrock = ((f64::from(q[4]) * 128.0 - (-64.0) + 6.0) / 8.0).clamp(0.0, 1.0);
        assert!((f64::from(result[3]) - source_bedrock).abs() < 1e-6);
        assert_eq!(
            result[7], 1.0,
            "enhanced stars must not acquire reference bedrock attenuation"
        );
        let expected = source_cpu([q[0], q[1], q[2], q[3]], q[4], q[5] > 0.5, q[6], q[7]);
        for i in 0..3 {
            let tolerance = 2e-6 + expected[i].abs() * 5e-5;
            assert!(
                (f64::from(result[i]) - expected[i]).abs() < tolerance,
                "query{q:?} channel{i} GPU{} f64{}",
                result[i],
                expected[i]
            );
            assert!(
                (result[i] - result[i + 4]).abs() < tolerance as f32,
                "query{q:?} source/current mismatch {result:?}"
            );
        }
    }
    eprintln!(
        "{} encoded-alpha/horizon/phase/Sun/Moon source comparisons",
        queries.len()
    );
}
