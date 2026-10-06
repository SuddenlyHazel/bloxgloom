use super::super::Atmosphere;
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

const FIXTURE: &str = r#"
struct Camera { view_projection:mat4x4f,sun:vec4f,horizon:vec4f,eye:vec4f,fog_range:vec4f,parallax:vec4f,sun_radiance:vec4f,sky_zenith:vec4f,ambient_lower:vec4f,ambient_upper:vec4f,cloud:vec4f };
@group(0) @binding(0) var<uniform> camera:Camera;
@group(0) @binding(1) var<storage,read_write> results:array<vec4f>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id:vec3u) {
 let world=vec3f(f32(id.x%16u)*128.0,20.0,0.0);
 let visibility=bg_primary_sun_transmittance(world);
 let n=vec3f(0.0,1.0,0.0);let sky=select(1.0,0.0,id.x>=32u);
 let ambient=bg_indirect_daylight(n,camera.sun,sky);
 let direct=bg_direct_light(n,camera.sun,sky);
 let light=bg_surface_light(n,camera.sun,sky,vec3f(0.0),vec3f(0.0),vec3f(0.0),1.0);
 let shaded=light-direct*(1.0-visibility);
 results[id.x]=vec4f(select(shaded,ambient,id.x>=16u&&id.x<32u),visibility);
}
"#;
fn source() -> String {
    format!(
        "{}\n{}\n{}\n{FIXTURE}",
        crate::render::sky::CLOUD_SHADER,
        include_str!("../../daylight.wgsl"),
        include_str!("../primary_cloud.wgsl")
    )
}

#[test]
fn primary_cloud_shader_validates() {
    let module = wgpu::naga::front::wgsl::parse_str(&source()).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}

#[test]
fn gpu_spatial_cloud_shadows_only_attenuate_direct_sun() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("primary cloud visibility regression"),
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
    let atmosphere = Atmosphere {
        sun: Vec3::Y,
        cloud: 1.0,
        scene_transport: true,
        ..Atmosphere::at(crate::daylight::INITIAL_MS)
    };
    let data = atmosphere.camera_data(Mat4::IDENTITY, Vec3::ZERO);
    let camera = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&data),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 48 * 16,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 48 * 16,
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
    let draw = |enabled: f32| {
        queue.write_buffer(&camera, 55 * 4, bytemuck::bytes_of(&enabled));
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(48, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, 48 * 16);
        queue.submit([encoder.finish()]);
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, |result| result.unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let pixels =
            bytemuck::cast_slice::<u8, [f32; 4]>(&readback.slice(..).get_mapped_range().unwrap())
                .to_vec();
        readback.unmap();
        pixels
    };
    let clear = draw(0.0);
    let cloudy = draw(1.0);
    let minimum = cloudy[..16]
        .iter()
        .map(|pixel| pixel[3])
        .fold(1.0f32, f32::min);
    let maximum = cloudy[..16]
        .iter()
        .map(|pixel| pixel[3])
        .fold(0.0f32, f32::max);
    assert!(
        minimum < 0.8 && maximum - minimum > 0.05,
        "cloud volume must cast spatially varying shadows: {minimum}..{maximum}"
    );
    for index in 0..48 {
        assert_eq!(clear[index][3], 1.0);
        for channel in 0..3 {
            if index >= 16 {
                assert!(
                    (clear[index][channel] - cloudy[index][channel]).abs() < 0.000001,
                    "ambient/sealed cave changed at {index}: {:?} vs {:?}",
                    clear[index],
                    cloudy[index]
                );
            } else {
                let direct =
                    atmosphere.sun_radiance().to_array()[channel] * std::f32::consts::FRAC_1_PI;
                assert!(
                    (clear[index][channel]
                        - cloudy[index][channel]
                        - direct * (1.0 - cloudy[index][3]))
                        .abs()
                        < 0.000001
                );
            }
        }
    }
}
