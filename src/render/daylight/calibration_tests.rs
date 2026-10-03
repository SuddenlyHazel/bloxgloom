//! Exercise the production WGSL, rather than a second CPU copy of its constants.
use super::*;
use wgpu::util::DeviceExt;

const FIXTURE: &str = r#"
@group(0) @binding(0) var<storage, read_write> result: array<vec4f>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3u) {
    if id.x >= 12u {
        let radiance = vec3f(0.16,0.04,0.25);
        var direction = vec3f(1.0,0.0,0.0);
        var normal = direction;
        if id.x == 13u { normal = -normal; }
        if id.x == 14u { direction = vec3f(0.0); }
        if id.x == 15u { direction *= 0.5; normal = -normal; }
        let local = bg_local_light(normal,radiance,direction);
        let visibility = select(1.0,0.0,id.x == 17u);
        result[id.x] = vec4f(bg_surface_light(normal,vec4f(0.0,1.0,0.0,0.0),0.0,
            local,vec3f(0.0),vec3f(0.0),visibility),1.0);
        return;
    }
    let sun = normalize(vec3f(-0.55, 0.65, -0.52));
    let away = normalize(vec3f(-sun.x, 0.0, -sun.z));
    var n = away;
    var strength = 1.0;
    var sky = 1.0;
    var glow = 0.0;
    var visibility = 1.0;
    if id.x == 1u { n = vec3f(0.0, 1.0, 0.0); }
    if id.x == 2u { n = vec3f(0.0, -1.0, 0.0); }
    if id.x == 3u || id.x == 5u || id.x == 7u { strength = 0.035; }
    if id.x >= 4u && id.x <= 7u { sky = 0.0; }
    if id.x == 6u || id.x == 7u { glow = 1.0; }
    if id.x == 8u || id.x == 10u { visibility = 0.75; }
    if id.x == 9u || id.x == 10u { n = -away; }
    if id.x == 11u { sky = 0.0; visibility = 0.75; }
    camera.sun_radiance *= strength;
    camera.ambient_lower *= strength;
    camera.ambient_upper *= strength;
    result[id.x] = vec4f(bg_surface_light(n, vec4f(sun, strength), sky, glow*glow*vec3f(1.0,0.57,0.23),
        vec3f(0.0), vec3f(0.0), visibility), 1.0);
}
"#;

fn source() -> String {
    // Standalone fixture supplies the same camera lighting fields as production.
    format!(
        "{}\n{}\n{FIXTURE}",
        include_str!("test_camera.wgsl"),
        include_str!("../daylight.wgsl")
    )
}

#[test]
fn calibrated_lighting_fixture_validates_without_gpu() {
    let module = wgpu::naga::front::wgsl::parse_str(&source()).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}

#[test]
#[ignore = "requires a GPU adapter"]
fn gpu_daylight_preserves_palette_caves_and_unoccluded_direct_light() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("shared daylight calibration regression"),
        source: wgpu::ShaderSource::Wgsl(source().into()),
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
        contents: &[0; 18 * 16],
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 18 * 16,
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
        pass.dispatch_workgroups(18, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, 18 * 16);
    queue.submit([encoder.finish()]);
    let slice = readback.slice(..);
    slice.map_async(wgpu::MapMode::Read, |result| result.unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let bytes = slice.get_mapped_range().unwrap();
    let rows: &[[f32; 4]] = bytemuck::cast_slice(&bytes);
    let light: Vec<_> = rows.iter().map(|v| Vec3::new(v[0], v[1], v[2])).collect();

    // A shaded neutral surface is only mildly cool. The old blue fill almost
    // cancelled the warm skin palette (red/blue ratio was approximately 1.08).
    assert!(light[0].z / light[0].x < 1.12);
    let skin = Vec3::new(0.91, 0.68, 0.49) * light[0];
    assert!(skin.x / skin.z > 1.65, "warm skin lost its hue: {skin:?}");
    let luminance = light[0].dot(Vec3::new(0.2126, 0.7152, 0.0722));
    assert!(
        (0.38..0.43).contains(&luminance),
        "exposure drift: {luminance}"
    );
    assert!(light[1].min_element() > light[0].max_element());
    assert!(light[2].max_element() < light[0].min_element());
    assert!(
        light[1].max_element() < 1.1,
        "daylight highlights are overdriven"
    );

    let floor = Vec3::new(0.012, 0.015, 0.022);
    assert!(light[4].distance(floor) < 1e-6);
    let local = Vec3::new(0.16, 0.04, 0.25);
    assert!(
        (light[12] - floor).distance(local) < 1e-6,
        "source RGB must survive"
    );
    assert!(
        (light[13] - floor).distance(local * 0.35) < 1e-6,
        "back face retains only unresolved voxel scattering"
    );
    assert_eq!(light[12], light[14], "zero confidence is isotropic");
    assert!(
        (light[15] - floor).distance(local * 0.675) < 1e-6,
        "direction confidence must remain continuous"
    );
    assert_eq!(light[16], light[17], "AO must not dim direct local light");
    assert_eq!(light[4], light[5], "sealed caves depend on time of day");
    assert_eq!(light[4], light[11], "local AO changed the cave floor");
    assert_eq!(light[6], light[7], "torch lighting depends on daylight");
    assert!((light[6] - floor).distance(Vec3::new(1.0, 0.57, 0.23)) < 1e-6);
    assert!((light[3] - floor).distance((light[0] - floor) * 0.035) < 1e-6);
    assert!((light[8] - floor).distance((light[0] - floor) * 0.75) < 1e-6);
    assert!(
        (light[9] - light[10]).distance(light[0] - light[8]) < 1e-6,
        "baked local occlusion must not dim the directional sun"
    );
}
