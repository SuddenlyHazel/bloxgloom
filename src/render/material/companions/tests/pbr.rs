//! Read back production packed-material decoding and energy/darkness boundaries.
use wgpu::util::DeviceExt;
const CASES: u32 = 16;
const FIXTURE: &str = r#"
@group(0) @binding(0) var<storage, read_write> result: array<vec4f>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3u) {
    let albedo = vec3f(0.5);
    if id.x < 6u {
        let greens = array<f32,6>(10.0,229.0,230.0,231.0,234.0,255.0);
        let p = bg_decode_pbr(vec4f(0.8,greens[id.x]/255.0,0.0,0.0),albedo,true,true);
        result[id.x] = vec4f(p.f0,p.metal);
    } else if id.x < 9u {
        let blues = array<f32,3>(65.0,255.0,64.0);
        let p = bg_decode_pbr(vec4f(0.8,10.0/255.0,blues[id.x-6u]/255.0,0.0),albedo,true,true);
        result[id.x] = vec4f(p.subsurface,p.porosity,p.emission,1.0);
    } else if id.x < 11u {
        let alpha = select(1.0,254.0/255.0,id.x==10u);
        let p = bg_decode_pbr(vec4f(0.8,10.0/255.0,0.0,alpha),albedo,true,true);
        result[id.x] = vec4f(p.emission,0.0,0.0,1.0);
    } else if id.x < 13u {
        result[id.x] = bg_normal_data(vec4f(128.0/255.0,128.0/255.0,32.0/255.0,1.0),id.x==11u);
    } else {
        let absent = id.x==13u;
        let metal = id.x==14u;
        let p = bg_decode_pbr(vec4f(0.8,select(10.0,231.0,metal)/255.0,0.0,0.0),albedo,true,!absent);
        let up = vec3f(0.0,1.0,0.0);
        if id.x==15u {
            let reflected = bg_pbr_environment(up,up,p,vec3f(1.0),1.0,vec3f(0.0),0.0);
            result[id.x] = vec4f(reflected+vec3f(bg_pbr_diffuse_weight(p,1.0)),1.0);
        } else {
            let reflected = bg_pbr_environment(up,up,p,vec3f(1.0),0.0,vec3f(1.0),0.0)
                +bg_pbr_sun(up,up,vec4f(up,1.0),0.0,1.0,p,vec3f(1.0));
            result[id.x] = vec4f(reflected,bg_pbr_diffuse_weight(p,1.0));
        }
    }
}
"#;
fn source() -> String {
    format!("{}\n{FIXTURE}", include_str!("../../pbr.wgsl"))
}
#[test]
fn packed_material_shader_validates_without_a_gpu() {
    let module = wgpu::naga::front::wgsl::parse_str(&source()).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}
#[test]
fn gpu_lab_pbr_decodes_f0_metals_sss_ao_emission_and_preserves_darkness() {
    let instance = wgpu::Instance::default();
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
        eprintln!("packed material GPU test skipped: no adapter");
        return;
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("packed material decoding regression"),
        source: wgpu::ShaderSource::Wgsl(source().into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let output = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &[0; CASES as usize * 16],
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(CASES * 16),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
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
        pass.dispatch_workgroups(CASES, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, u64::from(CASES * 16));
    queue.submit([encoder.finish()]);
    let slice = readback.slice(..);
    slice.map_async(wgpu::MapMode::Read, |result| result.unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let bytes = slice.get_mapped_range().unwrap();
    let rows: &[[f32; 4]] = bytemuck::cast_slice(&bytes);
    assert!(rows.iter().flatten().all(|v| v.is_finite()));
    for (id, expected) in [(0, 10.0 / 255.0), (1, 229.0 / 255.0)] {
        assert!(rows[id][..3].iter().all(|v| (v - expected).abs() < 1e-6));
        assert_eq!(
            rows[id][3], 0.0,
            "dielectric green is reflectance, not metalness"
        );
    }
    assert!(rows[2][0] > 0.2 && rows[2][0] < 0.4);
    assert!(
        rows[3][0] > rows[3][1] && rows[3][1] > rows[3][2],
        "gold conductor tint"
    );
    assert!(
        rows[4][0] > rows[4][1] && rows[4][1] > rows[4][2],
        "copper conductor tint"
    );
    assert!(rows[2..6].iter().all(|row| row[3] == 1.0));
    assert_eq!(rows[5], [0.5, 0.5, 0.5, 1.0]);
    assert!(rows[6][..3].iter().all(|v| v.abs() < 1e-6));
    assert_eq!(rows[6][3], 1.0);
    assert_eq!(rows[7], [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(rows[8], [0.0, 1.0, 0.0, 1.0]);
    assert_eq!(rows[9][0], 0.0, "255 means no emission");
    assert!((rows[10][0] - 1.0).abs() < 1e-6);
    assert!(
        rows[11][2] > 0.99,
        "AO channel does not invert surface normal"
    );
    assert!((rows[11][3] - 32.0 / 255.0).abs() < 1e-6);
    assert!(rows[12][2] < -0.99, "legacy RGB normal contract preserved");
    assert_eq!(rows[12][3], 1.0);
    assert_eq!(
        rows[13],
        [0.0, 0.0, 0.0, 1.0],
        "missing maps preserve legacy diffuse"
    );
    assert_eq!(
        rows[14], [0.0; 4],
        "sealed darkness has no reflections or metal diffuse"
    );
    assert!(
        rows[15][..3].iter().all(|v| *v >= 0.0 && *v <= 1.0),
        "bounded reflected plus diffuse energy"
    );
}
