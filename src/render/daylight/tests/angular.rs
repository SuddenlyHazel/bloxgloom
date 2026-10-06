//! Independent source hemisphere quadrature versus the actual compact SH9
//! consumer, including radiometry, azimuth, shelter and primary/GI metadata.
use super::super::*;
use wgpu::util::DeviceExt;
const NORMALS: u32 = 54;
fn source() -> String {
    let camera = include_str!("../../pipeline.wgsl")
        .lines()
        .find(|l| l.starts_with("struct Camera"))
        .unwrap();
    surface_shader(&format!(
        r#"{camera}
 @group(0) @binding(0) var<uniform> camera:Camera;
 @group(0) @binding(1) var<storage,read_write> result:array<vec4f>;
 fn radical(i:u32)->f32 {{var b=i;b=((b>>1u)&0x55555555u)|((b&0x55555555u)<<1u);b=((b>>2u)&0x33333333u)|((b&0x33333333u)<<2u);b=((b>>4u)&0x0f0f0f0fu)|((b&0x0f0f0f0fu)<<4u);b=((b>>8u)&0x00ff00ffu)|((b&0x00ff00ffu)<<8u);return f32((b>>16u)|(b<<16u))*2.3283064365386963e-10;}}
 fn receiver(i:u32)->vec3f {{let axes=array<vec3f,6>(vec3f(0.0,1.0,0.0),vec3f(0.0,-1.0,0.0),vec3f(-1.0,0.0,0.0),vec3f(1.0,0.0,0.0),vec3f(0.0,0.0,1.0),vec3f(0.0,0.0,-1.0));if i<6u {{return axes[i];}} let k=f32(i-6u);let y=1.0-2.0*(k+0.5)/48.0;let p=k*2.39996322973;let r=sqrt(1.0-y*y);return vec3f(r*cos(p),y,r*sin(p));}}
 @compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id:vec3u) {{
 let n=receiver(id.x);let ref_axis=select(vec3f(0.0,0.0,1.0),vec3f(0.0,1.0,0.0),abs(n.z)>0.9);
 let t=normalize(cross(ref_axis,n));let b=cross(n,t);var sum=vec3f(0.0);
 for(var i=0u;i<8192u;i++) {{let u=(f32(i)+0.5)/8192.0;let phi=radical(i)*6.28318530718;
 let ray=t*(sqrt(u)*cos(phi))+b*(sqrt(u)*sin(phi))+n*sqrt(1.0-u);
 sum+=bg_bsl_sky_default(ray,camera.sun.xyz,clamp(camera.sun.y/BG_FOG_NOON_HEIGHT,0.0,1.0),camera.sun_radiance.w,camera.ambient_upper.w)*smoothstep(-0.08,0.0,ray.y);}}
 let ambient=bg_indirect_daylight(n,camera.sun,1.0);result[id.x*5u]=vec4f(sum/8192.0,1.0);result[id.x*5u+1u]=vec4f(ambient,1.0);
 result[id.x*5u+2u]=vec4f(bg_indirect_daylight(n,camera.sun,0.0),1.0);
 let bounce=vec3f(0.01,0.02,0.03);let energy=bg_local_indirect_record(ambient,bounce,0.5,0.6);
 let record=bg_occluded_indirect_record(energy.rgb,energy.a,0.8,1.0);
 result[id.x*5u+3u]=vec4f(record.rgb*record.a,1.0);
 result[id.x*5u+4u]=vec4f(bg_occlude_indirect(ambient*0.6+bounce+vec3f(0.125),energy.rgb,energy.a,0.8),1.0);
 }}"#
    ))
}
#[test]
fn angular_sky_source_shader_validates_and_camera_keeps_legacy_offsets() {
    let m = wgpu::naga::front::wgsl::parse_str(&source()).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&m)
    .unwrap();
    let a = Atmosphere::at(crate::daylight::INITIAL_MS);
    let data = a.camera_data(Mat4::IDENTITY, Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(data.len(), 80);
    assert_eq!(std::mem::size_of_val(&data), 320);
    assert_eq!(&data[24..27], &[1.0, 2.0, 3.0]);
    assert_eq!(
        data[59],
        f32::from(crate::render::sky::style_enabled() && !crate::render::bsl_reference::enabled())
    );
}
#[test]
fn gpu_angular_sky_convolution_matches_source_and_retained_primary_energy() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("actual source sky and compact SH9 receiver"),
        source: wgpu::ShaderSource::Wgsl(source().into()),
    });
    let pipe = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let size = u64::from(NORMALS * 5 * 16);
    let out = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let baseline = Atmosphere::at(crate::daylight::INITIAL_MS);
    for (case, (elevation, rain, moon_phase, azimuth)) in [
        (baseline.sun.y, 0.0, 0, 0.0),
        (0.12, 0.0, 0, 0.0),
        (0.0, 0.0, 0, 0.0),
        (-0.3, 0.0, 0, 0.0),
        (0.12, 1.0, 0, 0.0),
        (0.12, 0.5, 4, 0.0),
        (0.12, 0.0, 0, std::f32::consts::FRAC_PI_2),
    ]
    .into_iter()
    .enumerate()
    {
        let horizontal = f32::sqrt(1.0 - elevation * elevation);
        let sun = Vec3::new(
            -horizontal * f32::cos(azimuth),
            elevation,
            -horizontal * f32::sin(azimuth),
        );
        let a = Atmosphere {
            sun,
            rain_strength: rain,
            moon_phase,
            ..baseline
        };
        let data = a.camera_data(Mat4::IDENTITY, Vec3::ZERO);
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&data),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipe.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: out.as_entire_binding(),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipe);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(NORMALS, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&out, 0, &read, 0, size);
        queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        read.slice(..)
            .map_async(wgpu::MapMode::Read, move |v| tx.send(v).unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        rx.recv().unwrap().unwrap();
        let values =
            bytemuck::cast_slice::<u8, [f32; 4]>(&read.slice(..).get_mapped_range().unwrap())
                .to_vec();
        read.unmap();
        let rgb = |i: usize| Vec3::from_array(values[i][..3].try_into().unwrap());
        let weights = Vec3::new(0.2126, 0.7152, 0.0722);
        let peak = (0..NORMALS as usize)
            .map(|i| rgb(i * 5).dot(weights))
            .fold(0.0_f32, f32::max)
            .max(1e-8);
        let mut squared = 0.0_f32;
        let mut maximum = 0.0_f32;
        for i in 0..NORMALS as usize {
            let truth = rgb(i * 5);
            let ambient = rgb(i * 5 + 1);
            let error = (ambient - truth).dot(weights).abs() / peak;
            maximum = maximum.max(error);
            squared += error * error;
            assert_eq!(rgb(i * 5 + 2), Vec3::ZERO, "sealed receiver");
            let expected = (ambient * 0.6 + Vec3::new(0.01, 0.02, 0.03)) * 0.8;
            assert!(
                rgb(i * 5 + 3).distance(expected) < 1e-6,
                "actual GI record must carry retained SH energy"
            );
            assert!(
                rgb(i * 5 + 4).distance(expected + Vec3::splat(0.125)) < 1e-6,
                "AO preserves direct/emission complement"
            );
        }
        let rms = (squared / NORMALS as f32).sqrt();
        eprintln!(
            "SH9 climate{case}: RMS{:0.3}% max{:0.3}% peak",
            rms * 100.0,
            maximum * 100.0
        );
        assert!(
            rms < 0.025 && maximum < 0.085,
            "climate{case}: {rms} {maximum}"
        );
        if case == 1 {
            assert!(
                rgb(2 * 5 + 1).x > rgb(3 * 5 + 1).x * 2.0,
                "warm sunward horizon radiance must retain azimuth"
            );
        }
    }
}
