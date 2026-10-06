//! Production pure reference shader is compared to independent source goldens.
use wgpu::util::DeviceExt;
const FIXTURE: &str = r#"
@group(0) @binding(0) var<storage,read_write> rows:array<vec4f>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id:vec3u) {
    let i=id.x;
    if i>=10u {
        let cosine=select(1.0,0.5,i==11u);
        rows[i]=vec4f(bg_direct_light(vec3f(0.0,1.0,0.0),
            vec4f(sqrt(1.0-cosine*cosine),cosine,0.0,1.0),0.75),1.0);
        return;
    }
    var sky=1.0;var block=0.0;var shadow=1.0;var smooth_factor=1.0;
    var normal=vec3f(0.0,1.0,0.0);var emission=0.0;var basic=0.0;
    var rain=0.0;var visible=1.0;var moon=1.0;
    var light=pow(vec3f(196.0,220.0,255.0)*(1.4/255.0),vec3f(2.0));
    var ambient=pow(vec3f(120.0,172.0,255.0)*(0.6/255.0),vec3f(2.0));
    if i==1u {shadow=0.0;}
    if i==2u {normal=vec3f(1.0,0.0,0.0);}
    if i==3u {sky=0.0;}
    if i==4u {sky=0.0;block=1.0;}
    if i==5u {smooth_factor=0.5;}
    if i==6u {emission=1.0;sky=0.0;}
    if i==7u {rain=1.0;}
    if i==8u {normal=vec3f(0.0,-1.0,0.0);basic=1.0;}
    if i==9u {
        visible=0.0;moon=0.5;
        let night=vec3f(96.0,192.0,255.0)*(0.3*moon/255.0);
        light=night*night;ambient=light*0.36;
    }
    let frame=BgBslReferenceFrame(light,ambient,vec3f(0.0,1.0,0.0),visible,rain,1.0,moon);
    rows[i]=vec4f(bg_bsl_default_surface(vec3f(0.2,0.4,0.1),normal,vec3f(0.0,0.0,1.0),
        vec2f(block,sky),smooth_factor,basic,emission,shadow,frame),1.0);
}
"#;

#[test]
fn gpu_default_reference_lighting_matches_source_goldens() {
    let instance = wgpu::Instance::default();
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
        return;
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("BSL default source lighting regression"),
        source: wgpu::ShaderSource::Wgsl(
            format!("{}\n{}\n{}\n{}\n{FIXTURE}",
                super::super::LIGHTING_SHADER,
                include_str!("../daylight/test_camera.wgsl"),
                "fn bg_bsl_reference_frame()->BgBslReferenceFrame {return BgBslReferenceFrame(vec3f(1.0),vec3f(1.0),vec3f(0.0,1.0,0.0),1.0,0.0,1.0,1.0);}",
                super::super::surface_shader_for(true)).into(),
        ),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let result = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &[0; 192],
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 192,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: result.as_entire_binding(),
        }],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(12, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&result, 0, &read, 0, 192);
    queue.submit([encoder.finish()]);
    read.slice(..)
        .map_async(wgpu::MapMode::Read, |result| result.unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let mapped = read.slice(..).get_mapped_range().unwrap();
    let rows: &[[f32; 4]] = bytemuck::cast_slice(&mapped);
    assert!(rows.iter().flatten().all(|value| value.is_finite()));
    // Filled below from independently evaluated GLSL defaults, including its
    // minimum/block-light curves, albedo balancing, AO² and desaturation.
    let expected: [[f64; 3]; 10] = [
        [0.2105354164, 0.5305036524, 0.1781818182],
        [0.0144951243, 0.0595588550, 0.0327272727],
        [0.0071036256, 0.0291880082, 0.0160386546],
        [0.0004655207, 0.0007403912, 0.0003280854],
        [0.2442409022, 0.3379119590, 0.0482170579],
        [0.0526338541, 0.1326259131, 0.0445454545],
        [0.5750811509, 1.4992992882, 0.2438984521],
        [0.0276743659, 0.0763269864, 0.0501707932],
        [0.0379316353, 0.0973070770, 0.0336370433],
        [0.0006083410, 0.0038246510, 0.0030644827],
    ];
    for (row, expected) in rows.iter().zip(expected) {
        for (actual, expected) in row[..3].iter().zip(expected) {
            assert!(
                (f64::from(*actual) - expected).abs() < 2e-6,
                "{row:?} vs {expected}"
            );
        }
    }
    // BSL comparison mode deliberately retains its source artistic solar units.
    // Enhanced 1/pi normalization must not alter shadow subtraction helpers.
    for (index, cosine) in [(10, 1.0f32), (11, 0.5)] {
        for (channel, solar) in [1.728f32, 1.608, 1.344].into_iter().enumerate() {
            assert!((rows[index][channel] - solar * cosine * 0.75).abs() < 0.00001);
        }
    }
}
