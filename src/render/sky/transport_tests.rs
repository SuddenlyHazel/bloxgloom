//! GPU checks compare the source equations to independent CPU tone goldens,
//! then protect actual volume extinction, solar shadowing and sealed darkness.
use wgpu::util::DeviceExt;
const COUNT: u32 = 23;
const FIXTURE: &str = r#"
@group(0) @binding(0) var<storage,read_write> rows:array<vec4f>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id:vec3u) {
    let i=id.x;
    if i<8u {
        let inputs=array<vec3f,8>(vec3f(0.0),vec3f(0.02),vec3f(0.25),vec3f(1.0),vec3f(8.0),vec3f(0.01,0.4,2.0),vec3f(2.0,0.01,0.2),vec3f(0.15,0.25,0.4));
        rows[i]=vec4f(bg_bsl_tonemap_default(inputs[i]),1.0);
    } else if i==8u {rows[i]=vec4f(bg_cloud_density(vec3f(0.0,191.0,0.0),1.0,vec2f(0.0)),bg_cloud_density(vec3f(0.0,253.0,0.0),1.0,vec2f(0.0)),0.0,1.0);}
    else if i==9u {rows[i]=vec4f(bg_cloud_density(vec3f(10.0,205.0,20.0),0.3,vec2f(0.0)),bg_cloud_density(vec3f(10.0,220.0,20.0),0.3,vec2f(0.0)),bg_cloud_density(vec3f(93.0,220.0,17.0),0.3,vec2f(0.0)),1.0);}
    else if i==10u {rows[i]=vec4f(bg_cloud_transmittance(vec3f(0.0,100.0,0.0),vec3f(0.0,-1.0,0.0),1.0,vec2f(0.0)),bg_cloud_transmittance(vec3f(0.0,100.0,0.0),vec3f(0.0,1.0,0.0),1.0,vec2f(0.0)),0.0,1.0);}
    else if i==11u {rows[i]=bg_cloud_integrate(vec3f(0.0,100.0,0.0),vec3f(0.0,1.0,0.0),vec3f(0.0,1.0,0.0),vec3f(0.0),vec3f(0.0),1.0,vec2f(0.0),8u,4u);}
    else if i==12u {rows[i]=bg_cloud_integrate(vec3f(0.0,100.0,0.0),vec3f(0.0,1.0,0.0),vec3f(0.0,1.0,0.0),vec3f(1.0),vec3f(0.1),1.0,vec2f(0.0),24u,4u);}
    else if i==13u {rows[i]=vec4f(bg_sky_environment(vec3f(0.0,100.0,0.0),vec3f(0.0,-1.0,0.0),vec4f(0.0,1.0,0.0,1.0),vec3f(1.0),vec4f(1.0,0.0,0.0,1.0),vec2f(1.0,1.0),vec3f(1.0),vec3f(1.0)),1.0);}
    else if i==14u {rows[i]=vec4f(bg_bsl_sky_default(normalize(vec3f(0.4,0.5,0.2)),normalize(vec3f(-0.55,0.65,-0.52)),1.0,0.0,1.0),1.0);}
    else if i==15u {rows[i]=vec4f(bg_bsl_sky_default(normalize(vec3f(0.4,0.5,0.2)),normalize(vec3f(-0.55,0.65,-0.52)),1.0,1.0,1.0),1.0);}
    else if i==16u {rows[i]=vec4f(bg_bsl_sky_default(vec3f(0.0,1.0,0.0),vec3f(0.0,-1.0,0.0),0.0,0.0,1.0),1.0);}
    else if i==17u {rows[i]=vec4f(bg_bsl_sky_default(vec3f(0.0,1.0,0.0),vec3f(0.0,-1.0,0.0),0.0,0.0,0.5),1.0);}
    else if i==18u {rows[i]=vec4f(bg_bsl_ambient_default(vec3f(0.0,-1.0,0.0),0.0,0.0,0.5),1.0);}
    else if i==19u {rows[i]=vec4f(bg_bsl_stars(normalize(vec3f(0.1,0.6,0.3)),vec3f(100.0,0.0,100.0),vec3f(0.0,-1.0,0.0),250.0,1.0,0.5),1.0);}
    else if i==20u {rows[i]=vec4f(bg_bsl_stars(vec3f(0.0,-1.0,0.0),vec3f(0.0),vec3f(0.0,-1.0,0.0),250.0,0.0,1.0),1.0);}
    else {rows[i]=vec4f(bg_bsl_fog_default(normalize(vec3f(0.4,0.0,0.2)),150.0,normalize(vec3f(-0.55,0.65,-0.52)),1.0,select(0.0,1.0,i==22u),1.0),1.0);}
}
"#;
#[test]
fn gpu_bsl_tone_matches_default_equations_and_clouds_transport_light() {
    let instance = wgpu::Instance::default();
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
        eprintln!("sky transport GPU regression skipped: no adapter");
        return;
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let source = format!("{}\n{FIXTURE}", super::super::environment_shader());
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("BSL sky and volume transport regression"),
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
    let output = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &[0; COUNT as usize * 16],
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(COUNT * 16),
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
        pass.dispatch_workgroups(COUNT, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &read, 0, u64::from(COUNT * 16));
    queue.submit([encoder.finish()]);
    read.slice(..)
        .map_async(wgpu::MapMode::Read, |r| r.unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let mapped = read.slice(..).get_mapped_range().unwrap();
    let rows: &[[f32; 4]] = bytemuck::cast_slice(&mapped);
    assert!(rows.iter().flatten().all(|v| v.is_finite()));
    let inputs = [
        [0.; 3],
        [0.02; 3],
        [0.25; 3],
        [1.; 3],
        [8.; 3],
        [0.01, 0.4, 2.],
        [2., 0.01, 0.2],
        [0.15, 0.25, 0.4],
    ];
    for (row, input) in rows.iter().zip(inputs) {
        let expected = tone_reference(input);
        for channel in 0..3 {
            assert!(
                (row[channel] - expected[channel]).abs() < 2e-6,
                "BSL tone {input:?}: {row:?} vs {expected:?}"
            );
        }
    }
    assert_eq!(
        rows[8],
        [0., 0., 0., 1.],
        "volume does not exist outside the slab"
    );
    assert!(rows[9][..3].iter().all(|v| *v > 0. && *v <= 0.085));
    assert!(
        (rows[9][0] - rows[9][1]).abs() > 1e-5 && (rows[9][1] - rows[9][2]).abs() > 1e-5,
        "density varies in all three dimensions"
    );
    assert_eq!(rows[10][0], 1.);
    assert!(
        rows[10][1] > 0. && rows[10][1] < 0.8,
        "cloud solar extinction"
    );
    assert_eq!(
        rows[11][..3],
        [0.; 3],
        "zero input radiance has no invented cloud illumination"
    );
    assert!(
        (rows[11][3] - rows[10][1]).abs() < 1e-6,
        "same Beer-Lambert extinction in lighting and view rays"
    );
    assert!(rows[12][..3].iter().all(|v| *v > 0.));
    assert!(rows[12][3] > 0. && rows[12][3] < 1.);
    for (full, phase) in rows[16][..3].iter().zip(&rows[17][..3]) {
        assert!(
            (phase - full * 0.25).abs() < 1e-7,
            "BSL phase scales squared night radiance"
        );
    }
    assert_eq!(rows[19][..3], [0.0; 3], "BSL stars suppressed by rain");
    assert_eq!(rows[20][..3], [0.0; 3], "BSL stars absent below horizon");
    let ambient = [96.0f32, 192.0, 255.0].map(|v| (v * 0.60 * 0.3 / 255.0 * 0.5).powi(2));
    for (channel, expected) in ambient.into_iter().enumerate() {
        assert!(
            (rows[18][channel] - expected).abs() < 1e-7,
            "night cloud ambient uses moon phase"
        );
    }
    assert_eq!(
        rows[13][..3],
        [0.; 3],
        "downward rays cannot get synthetic sky bounce"
    );
    // Goldens independently evaluated from provided BSL GetSkyColor and
    // default settings: full moon, reference directions, fogDensity=1.
    for (row, expected) in [
        (14, [0.012180346, 0.033132915, 0.08298851]),
        (15, [0.025442103, 0.041212, 0.05340821]),
        (21, [0.10907954, 0.245_475_9, 0.51199362]),
        (22, [0.04011068, 0.06497268, 0.08420058]),
    ] {
        for channel in 0..3 {
            assert!(
                (rows[row][channel] - expected[channel]).abs() < 2e-6,
                "BSL sky oracle row{row}: {:?}",
                rows[row]
            );
        }
    }
}
fn tone_reference(input: [f32; 3]) -> [f32; 3] {
    let input = input.map(f64::from);
    let matrix = [[0.96, 0.03, 0.01], [0.03, 0.94, 0.03], [0.01, 0.03, 0.96]];
    let inverse = [
        [
            1. / 3. + 1. / (2. * 0.95) + 1. / (6. * 0.91),
            1. / 3. - 2. / (6. * 0.91),
            1. / 3. - 1. / (2. * 0.95) + 1. / (6. * 0.91),
        ],
        [
            1. / 3. - 2. / (6. * 0.91),
            1. / 3. + 4. / (6. * 0.91),
            1. / 3. - 2. / (6. * 0.91),
        ],
        [
            1. / 3. - 1. / (2. * 0.95) + 1. / (6. * 0.91),
            1. / 3. - 2. / (6. * 0.91),
            1. / 3. + 1. / (2. * 0.95) + 1. / (6. * 0.91),
        ],
    ];
    let value = matrix
        .map(|row| {
            row.into_iter()
                .zip(input)
                .map(|(m, v)| m * v * 4.)
                .sum::<f64>()
        })
        .map(|v| v / (v * v + 1.).sqrt());
    inverse.map(|row| {
        row.into_iter()
            .zip(value)
            .map(|(m, v)| m * v)
            .sum::<f64>()
            .clamp(0., 1.) as f32
    })
}
