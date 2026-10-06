//! Calibrated leaf absorption uses the exact runtime WGSL in both render paths.
use wgpu::util::DeviceExt;

const CASES: u32 = 28;
const FIXTURE: &str = r#"
@group(0) @binding(0) var<storage,read_write> results:array<vec4f>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id:vec3u) {
 let colors=array<vec3f,4>(vec3f(0.60,0.36,0.34),vec3f(0.0),vec3f(0.02),vec3f(1.0));
 let strengths=array<f32,3>(0.0,0.653,1.0);
 if id.x<24u {
  let index=id.x%12u;
  let optics=bg_foliage_optics(colors[index%4u],strengths[index/4u],4u<<27u);
  results[id.x]=vec4f(select(optics.reflected+optics.transmitted,optics.transmitted,id.x>=12u),optics.share);
  return;
 }
 let optics=bg_foliage_optics(colors[0],strengths[1],4u<<27u);
 let sun=vec4f(0.0,select(1.0,-1.0,id.x==25u),0.0,1.0);
 let sky=select(1.0,0.0,id.x==26u);
 let solar=select(vec3f(1.0),vec3f(0.0),id.x==27u);
 results[id.x]=vec4f(bg_foliage_optical_direct(vec3f(0.0,1.0,0.0),vec3f(0.0,1.0,0.0),
  vec3f(0.0,1.0,0.0),sun,sky,0.0,optics,solar),1.0);
}
"#;
fn source() -> String {
    format!("{}\n{FIXTURE}", include_str!("../../foliage_optics.wgsl"))
}

#[test]
fn shared_foliage_optical_fixture_validates() {
    let module = wgpu::naga::front::wgsl::parse_str(&source()).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}

#[test]
fn gpu_foliage_optics_conserve_energy_and_separate_absorption_from_albedo() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("shared foliage absorption and energy regression"),
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
        contents: &[0u8; CASES as usize * 16],
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
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, |result| result.unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let data = readback.slice(..).get_mapped_range().unwrap();
    let values = bytemuck::cast_slice::<u8, [f32; 4]>(&data);
    for value in &values[..12] {
        assert!(
            value[..3]
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.000001).contains(v)),
            "leaf R+T energy exceeded one: {value:?}"
        );
        assert!((0.15..=0.800001).contains(&value[3]));
    }
    let tint = values[16];
    assert!((tint[3] - (0.15 + 0.65 * 0.653)).abs() < 0.00001);
    for (channel, color) in [0.60f32, 0.36, 0.34].into_iter().enumerate() {
        assert!(
            (tint[channel] - color.powf(0.2) * tint[3]).abs() < 0.00001,
            "thin absorption must use species path thickness: {tint:?}"
        );
        assert!(
            tint[channel] * tint[channel] > (color * 0.653).powi(2),
            "two sheets must not repeatedly use full opaque diffuse reflectance"
        );
        assert!(
            (values[24][channel]
                - (values[4][channel] - tint[channel]) * std::f32::consts::FRAC_1_PI)
                .abs()
                < 0.00001
        );
        assert!(
            (values[25][channel] - tint[channel] * std::f32::consts::FRAC_1_PI).abs() < 0.00001
        );
        assert_eq!(
            values[26][channel], 0.0,
            "sealed cave cannot gain exterior light"
        );
        assert_eq!(
            values[27][channel], 0.0,
            "optics must never emit its own light"
        );
    }
}
