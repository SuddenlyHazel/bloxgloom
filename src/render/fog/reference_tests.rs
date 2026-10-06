//! Independent double-precision goldens evaluated from checked-in fog.glsl
//! with its default settings. These validate explicit inputs, not Minecraft's
//! eyeBrightnessSmooth equivalence to the engine shelter input.
use super::*;
use wgpu::util::DeviceExt;

const FIXTURE: &str = r#"
struct Camera { view_projection: mat4x4f, sun: vec4f, horizon: vec4f, eye: vec4f, fog_range: vec4f, parallax:vec4f, sun_radiance:vec4f, sky_zenith:vec4f, ambient_lower:vec4f, ambient_upper:vec4f, cloud:vec4f };
var<private> camera:Camera;
@group(0) @binding(0) var<storage,read_write> rows:array<vec4f>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id:vec3u) {
    let i=id.x;
    if i<9u {
        let inputs=array<vec4f,9>(vec4f(0.0,62.0,1.0,1.0),vec4f(64.0,62.0,1.0,1.0),
            vec4f(135.0,62.0,1.0,1.0),vec4f(1024.0,62.0,1.0,1.0),vec4f(64.0,190.0,1.0,1.0),
            vec4f(64.0,62.0,1.0,0.0),vec4f(64.0,62.0,1.0,1.0),vec4f(64.0,62.0,0.0,1.0),vec4f(64.0,62.0,0.5,1.0));
        let rain=array<f32,9>(0.0,0.0,0.0,0.0,0.0,0.0,1.0,0.0,0.3);
        let p=inputs[i];rows[i]=vec4f(0.0,0.0,0.0,bg_bsl_air_fog_amount(p.x,p.y,p.z,p.w,rain[i]));return;
    }
    if i<14u {
        let exterior=array<f32,5>(0.0,0.5,1.0,1.0,1.0);
        let altitude=array<f32,5>(62.0,62.0,62.0,-66.0,-70.0);
        rows[i]=vec4f(bg_bsl_air_fog_exterior(vec3f(0.1,0.2,0.4),exterior[i-9u],altitude[i-9u],-64.0),1.0);return;
    }
    camera.sun=vec4f(0.0,1.0,0.0,1.0);camera.eye=vec4f(0.0,48.0,0.0,1.0);
    camera.sky_zenith.w=1.0;camera.ambient_lower.w=1.0;camera.fog_range=vec4f(48.0,-64.0,0.0,0.0);
    var world=vec3f(0.0,48.0,135.0);
    if i==15u {camera.eye.y=0.0;world.y=0.0;}
    if i==16u {camera.fog_range.w=1.0;}
    if i==17u {camera.fog_range.w=-1.0;}
    rows[i]=vec4f(bg_apply_fog(vec3f(0.2),world,0.0),bg_fog_transmittance(world,0.0));
}
"#;

fn source() -> String {
    format!(
        "{}\nconst BG_FOG_BSL_STYLE:bool=true;\nconst BG_FOG_REFERENCE:bool=true;\nconst BG_FOG_NOON_HEIGHT:f32=1.0;\n{REFERENCE_SHADER}\n{SHADER}\n{FIXTURE}",
        crate::render::sky::STYLE_SHADER
    )
}

#[test]
fn reference_fog_fixture_validates_without_gpu() {
    let module = wgpu::naga::front::wgsl::parse_str(&source()).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}

#[test]
fn gpu_reference_air_fog_matches_source_defaults_and_relative_lod() {
    let instance = wgpu::Instance::default();
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
        return;
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("source default air fog"),
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
        contents: &[0; 288],
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 288,
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
    encoder.copy_buffer_to_buffer(&output, 0, &read, 0, 288);
    queue.submit([encoder.finish()]);
    read.slice(..)
        .map_async(wgpu::MapMode::Read, |result| result.unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let mapped = read.slice(..).get_mapped_range().unwrap();
    let rows: &[[f32; 4]] = bytemuck::cast_slice(&mapped);
    let amounts: [f64; 9] = [
        0.0,
        0.023404828296,
        0.075200125693,
        0.610479994343,
        0.007782061740,
        0.297811498673,
        0.443955369643,
        0.060586937187,
        0.036145842294,
    ];
    for (row, expected) in rows.iter().zip(amounts) {
        assert!(
            (f64::from(row[3]) - expected).abs() < 0.000002,
            "{row:?} vs {expected}"
        );
    }
    let colors: [[f64; 3]; 5] = [
        [0.001259823145; 3],
        [0.025629911572, 0.050629911572, 0.100629911572],
        [0.1, 0.2, 0.4],
        [0.05, 0.1, 0.2],
        [0.0; 3],
    ];
    for (row, expected) in rows[9..14].iter().zip(colors) {
        for (actual, expected) in row.iter().zip(expected) {
            assert!((f64::from(*actual) - expected).abs() < 0.000002);
        }
    }
    assert!((f64::from(rows[14][3]) - (1.0 - amounts[2])).abs() < 0.000002);
    for (a, b) in rows[14].iter().zip(rows[15]) {
        assert!(
            (*a - b).abs() < 0.000001,
            "relative LOD changed fog altitude"
        );
    }
    assert_eq!(
        rows[17],
        [0.2, 0.2, 0.2, 1.0],
        "actual reference water eye bypasses normal-air fog before composite absorption"
    );
    assert_eq!(
        rows[16],
        [0.2, 0.2, 0.2, 1.0],
        "scene transport must bypass raster fog"
    );
}
