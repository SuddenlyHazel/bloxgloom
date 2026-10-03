use super::*;
use glam::{Mat4, Vec3};

#[test]
fn camera_uniform_carries_real_eye_and_shelter_controls_storm_density() {
    let mut atmosphere = crate::render::daylight::Atmosphere::at(crate::daylight::INITIAL_MS);
    atmosphere.fog = 1.0;
    let eye = Vec3::new(-1200.0, 40.0, 700.0);
    let data = atmosphere.camera_data(Mat4::IDENTITY, eye);
    assert_eq!(&data[24..27], &eye.to_array());
    assert_eq!(data[23], 0.065);
    assert_eq!(data[27], 1.0);
    assert!(density(0.1, 1.0) < density(0.333, 1.0));
    assert!(density(0.333, 1.0) < data[23]);
    atmosphere.fog_exposure = 0.0;
    assert_eq!(atmosphere.camera_data(Mat4::IDENTITY, eye)[27], 0.0);
    atmosphere.fog = 0.0;
    atmosphere.fog_exposure = 1.0;
    assert_eq!(atmosphere.camera_data(Mat4::IDENTITY, eye)[23], 0.0);
}

#[test]
#[ignore = "requires a GPU adapter"]
fn gpu_storm_fog_preserves_near_contrast_and_obscures_distant_shadows() {
    use wgpu::util::DeviceExt;
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let source = shader(COMPUTE_FIXTURE);
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("depth fog regression"),
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
    let mut atmosphere = crate::render::daylight::Atmosphere::at(crate::daylight::INITIAL_MS);
    atmosphere.fog = 1.0;
    let camera = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(
            &atmosphere.camera_data(Mat4::IDENTITY, Vec3::new(1000.0, 20.0, -500.0)),
        ),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 208,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 208,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: camera.as_entire_binding(),
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
        pass.dispatch_workgroups(8, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, 128);
    queue.submit([encoder.finish()]);
    let slice = readback.slice(..);
    slice.map_async(wgpu::MapMode::Read, |result| result.unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let bytes = slice.get_mapped_range().unwrap();
    let values: &[[f32; 4]] = bytemuck::cast_slice(&bytes);
    assert_eq!(values[0][3], 0.0);
    assert!((values[0][0] - 0.02).abs() < 0.001);
    assert!((values[1][0] - 0.62).abs() < 0.001);
    assert!(values[2][3] > 0.5 && values[2][3] < values[4][3]);
    assert!(values[6][3] > 0.999);
    for (shaded, lit) in values[6].iter().zip(&values[7]).take(3) {
        assert!(
            (shaded - lit).abs() < 0.002,
            "distant shadows retain contrast through dense fog"
        );
    }
}

const COMPUTE_FIXTURE: &str = r#"
struct Camera { view_projection: mat4x4f, sun: vec4f, horizon: vec4f, eye: vec4f, fog_range: vec4f };
@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<storage, read_write> result: array<vec4f>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3u) {
    let distances = array<f32, 4>(5.0, 20.0, 40.0, 80.0);
    let distance = distances[id.x / 2u];
    let sky = f32(id.x % 2u);
    let color = vec3f(0.02 + sky * 0.6);
    result[id.x] = vec4f(bg_apply_fog(color, camera.eye.xyz + vec3f(distance,0.0,0.0), sky),
        bg_weather_fog(distance, camera.horizon.w * camera.eye.w));
}
"#;

#[test]
fn weather_fog_compute_fixture_validates_without_a_gpu() {
    let module = wgpu::naga::front::wgsl::parse_str(&shader(COMPUTE_FIXTURE)).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}
