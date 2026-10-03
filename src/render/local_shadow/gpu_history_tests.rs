//! GPU evaluation of the shared reactivity contract, independent of tone mapping.
use super::*;

#[test]
fn gpu_local_shadow_history_rejects_only_attributed_lit_surfaces() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let camera = zero_uniform(&device, 256, "history test camera");
    let mut local = LocalShadows::new_with_settings(
        &device,
        &camera,
        Settings {
            count: 1,
            resolution: 64,
            range: 12.0,
            updates: 1,
        },
    );
    let source = Source {
        position: Vec3::ZERO,
        range: 12.0,
        color: [1.0, 0.63, 0.31],
    };
    let code = format!(
        r#"
struct Camera {{ ambient_lower:vec4f }};
@group(0) @binding(0) var<uniform> camera:Camera;
{}
@group(1) @binding(0) var<storage,read_write> result:array<f32>;
@compute @workgroup_size(1) fn main() {{
 let world=vec3f(0.0,-4.0,0.0);
 let normal=vec3f(0.0,1.0,0.0);
 let rgb=vec3f(1.0,0.63,0.31);
 result[0]=bg_local_history_sign(world,normal,rgb,normal);
 result[1]=bg_local_history_sign(world,normal,vec3f(0.0),normal);
 result[2]=bg_local_history_sign(world,normal,vec3f(0.0,0.0,1.0),normal);
 result[3]=bg_local_history_sign(world,-normal,rgb,normal);
 result[4]=bg_local_history_sign(vec3f(0.0,-40.0,0.0),normal,rgb,normal);
}}
"#,
        include_str!("../local_shadow.wgsl")
    );
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("production local history sign"),
        source: wgpu::ShaderSource::Wgsl(code.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let directionality = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&[0.0f32, 0.0, 0.0, 1.0]),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let lights = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: directionality.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: local.uniform.as_entire_binding(),
            },
        ],
    });
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 20,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 20,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let out = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(1),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: output.as_entire_binding(),
        }],
    });
    for enabled in [true, false] {
        for _ in 0..2 {
            local.update(
                &queue,
                Vec3::ZERO,
                if enabled {
                    std::slice::from_ref(&source)
                } else {
                    &[]
                },
                0.25,
            );
        }
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &lights, &[]);
            pass.set_bind_group(1, &out, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, 20);
        queue.submit([encoder.finish()]);
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, |r| r.unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let bytes = readback.slice(..).get_mapped_range().unwrap();
        let signs: &[f32] = bytemuck::cast_slice(&bytes);
        assert_eq!(
            signs,
            &[if enabled { -1.0 } else { 1.0 }, 1.0, 1.0, 1.0, 1.0],
            "only a potentially moving attributed local shadow rejects temporal history"
        );
        drop(bytes);
        readback.unmap();
    }
}
