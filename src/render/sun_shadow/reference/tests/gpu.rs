//! Known-input source equations, independent CPU oracle and actual WGSL compute.
use glam::{Vec2, Vec3};
use wgpu::util::DeviceExt;
const CASES: u32 = 18;
const MAIN: &str = r#"
@group(0) @binding(0) var<storage,read_write> output:array<vec4f>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id:vec3u) {
 let i=id.x;if i>=18u {return;}
 if i<4u {
  let xy=array<vec2f,4>(vec2f(0.0),vec2f(0.5,0.0),vec2f(0.3,0.4),vec2f(-0.2,0.7));
  output[i]=vec4f(bg_bsl_distort_shadow(vec3f(xy[i],f32(i%2u)*2.0-1.0),0.9),1.0);return;
 }
 if i<7u {
  let alpha=array<f32,3>(0.0,0.5,1.0);
  output[i]=vec4f(bg_bsl_shadow_color(alpha[i-4u],vec3f(0.2,0.6,0.9)),1.0);return;
 }
 if i<15u {
  let v=array<f32,4>(0.0,0.25,0.5,1.0);let k=i-7u;
  output[i]=vec4f(bg_bsl_shadow_result(v[k%4u],vec3f(0.2,0.5,0.8),f32(k/4u)),1.0);return;
 }
 let nl=array<f32,3>(1.0,0.6,0.2);
 output[i]=vec4f(bg_bsl_shadow_bias(vec3f(0.15,0.2,0.0),30.0,nl[i-15u],select(0.0,1.0,i==17u),1.0/2048.0,256.0,0.9),0.0,1.0);
}
"#;
fn expected() -> Vec<[f32; 4]> {
    let mut out = Vec::new();
    for (i, xy) in [
        Vec2::ZERO,
        Vec2::new(0.5, 0.0),
        Vec2::new(0.3, 0.4),
        Vec2::new(-0.2, 0.7),
    ]
    .into_iter()
    .enumerate()
    {
        let factor = xy.length() * 0.9 + 0.1;
        let p = Vec3::new(
            xy.x / factor,
            xy.y / factor,
            if i % 2 == 0 { -0.2 } else { 0.2 },
        ) * 0.5
            + Vec3::splat(0.5);
        out.push(p.extend(1.0).to_array());
    }
    for alpha in [0f32, 0.5, 1.0] {
        let col = Vec3::ONE.lerp(Vec3::new(0.2, 0.6, 0.9), 1.0 - (1.0 - alpha).powf(1.5))
            * (1.0 - alpha.powf(96.0));
        out.push(col.extend(1.0).to_array());
    }
    for subsurface in [0f32, 1.0] {
        for visibility in [0f32, 0.25, 0.5, 1.0] {
            let s = visibility * (visibility * (1.0 - subsurface) + subsurface);
            let col = Vec3::new(0.2, 0.5, 0.8);
            out.push(
                (col * col * (1.0 - s) + Vec3::splat(s))
                    .extend(1.0)
                    .to_array(),
            );
        }
    }
    let factor = 0.25 * 0.9 + 0.1;
    for (i, cosine) in [1f32, 0.6, 0.2].into_iter().enumerate() {
        let (offset, bias) = if i == 2 {
            (0.0007 * (0.4 * 1.5 + 1.0), 0.0002)
        } else {
            (
                1.0 / 2048.0,
                (8.0 * factor * factor * (1.0 - cosine * cosine).sqrt() / cosine
                    + 30.0 * 0.005
                    + 0.05)
                    / 2048.0,
            )
        };
        out.push([offset, bias, 0.0, 1.0]);
    }
    out
}
#[test]
fn gpu_source_shadow_rgb_projection_and_bias_goldens() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let source = format!("{}\n{MAIN}", include_str!("../equations.wgsl"));
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("source shadow equation goldens"),
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
    let zeros = vec![[0f32; 4]; CASES as usize];
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&zeros),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: buffer.size(),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(CASES, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&buffer, 0, &readback, 0, buffer.size());
    queue.submit([encoder.finish()]);
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, |r| r.unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let bytes = readback.slice(..).get_mapped_range().unwrap();
    let actual: &[[f32; 4]] = bytemuck::cast_slice(&bytes);
    for (i, (actual, expected)) in actual.iter().zip(expected()).enumerate() {
        assert!(
            actual
                .iter()
                .zip(expected)
                .all(|(a, e)| (a - e).abs() < 0.00002),
            "source shadow row{i}: GPU={actual:?} source={expected:?}"
        );
    }
}
