use super::*;
#[test]
fn gpu_source_shafts_phase_and_underwater_solar_math_goldens() {
    pollster::block_on(async {
        let adapter = wgpu::Instance::default()
            .request_adapter(&Default::default())
            .await
            .unwrap();
        let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
        let shader=device.create_shader_module(wgpu::ShaderModuleDescriptor {label:Some("independent BSL phase and underwater SunGlare goldens"),source:wgpu::ShaderSource::Wgsl(format!(r#"{}
 @group(0) @binding(0) var<storage,read_write> output:array<vec4f>;
 @compute @workgroup_size(8) fn math(@builtin(global_invocation_id) id:vec3u) {{
 let inputs=array<vec4f,7>(vec4f(1.0,1.0,1.0,0.0),vec4f(0.5,1.0,1.0,0.0),vec4f(0.0,1.0,1.0,0.0),vec4f(-1.0,1.0,1.0,0.0),vec4f(0.5,0.0,0.0,0.0),vec4f(0.5,1.0,1.0,1.0),vec4f(0.5,1.0,1.0,0.5));
 if id.x<7u {{let p=inputs[id.x];let e=select(1.0,select(0.25,0.0,id.x==3u),id.x==3u||id.x==6u);output[id.x]=vec4f(bg_bsl_shaft_falloff(p.x,p.y,p.z,p.w,e),0.0,0.0,1.0);}}
 else {{output[7]=vec4f(bg_bsl_underwater_sun_glare(vec3f(0.0,0.0,1.0),vec3f(0.0,0.0,1.0),vec3f(0.25,0.5,1.0),1.0,1.0,0.0,1.0,1.0,80.0),1.0);}}
 }}"#,crate::render::sky::STYLE_SHADER).into())});
        let output = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 128,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: None,
            module: &shader,
            entry_point: Some("math"),
            compilation_options: Default::default(),
            cache: None,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: output.as_entire_binding(),
            }],
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        let bytes = read_buffer(&device, &queue, encoder, &output, 128);
        let rows = bytemuck::cast_slice::<u8, [f32; 4]>(&bytes);
        // Independent double precision evaluation of the checked-in phase formula.
        for (row, expected) in rows.iter().zip([
            1.0_f64,
            0.014563106796116498,
            0.0,
            0.03125,
            0.11739130434782606,
            0.18145161290322576,
            0.12668639867841405,
        ]) {
            assert!(
                (f64::from(row[0]) - expected).abs() < 1e-6,
                "{row:?} vs{expected}"
            );
        }
        assert_eq!(rows[7], [0.0625, 0.125, 0.25, 1.0]);
    });
}
