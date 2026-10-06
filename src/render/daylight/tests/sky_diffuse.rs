//! Source-derived diffuse LUT acceptance against actual GPU sky equations.
use super::super::*;
use wgpu::util::DeviceExt;

fn source() -> String {
    let camera = include_str!("../../pipeline.wgsl")
        .lines()
        .find(|line| line.starts_with("struct Camera"))
        .unwrap();
    surface_shader(&format!(
        "{camera}\n@group(0) @binding(0) var<uniform> camera:Camera;\n{}",
        r#"
@group(0) @binding(1) var<storage,read_write> results:array<vec4f>;
fn radical(index:u32)->f32 {
 var bits=index;
 bits=((bits>>1u)&0x55555555u)|((bits&0x55555555u)<<1u);
 bits=((bits>>2u)&0x33333333u)|((bits&0x33333333u)<<2u);
 bits=((bits>>4u)&0x0f0f0f0fu)|((bits&0x0f0f0f0fu)<<4u);
 bits=((bits>>8u)&0x00ff00ffu)|((bits&0x00ff00ffu)<<8u);
 bits=(bits>>16u)|(bits<<16u);
 return f32(bits)*2.3283064365386963e-10;
}
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id:vec3u) {
 let brightness=clamp(camera.sun.y/BG_FOG_NOON_HEIGHT,0.0,1.0);
 if id.x<2u {
  var sum=vec3f(0.0);
  for(var index=0u;index<32768u;index++) {
   let u=(f32(index)+0.5)/32768.0;let phi=radical(index)*6.28318530718;
   let r=sqrt(u);var ray=vec3f(r*cos(phi),sqrt(1.0-u),r*sin(phi));
   if id.x==1u {ray.y=-ray.y;}
   sum+=bg_bsl_sky_default(ray,camera.sun.xyz,brightness,
       camera.sun_radiance.w,camera.ambient_upper.w)*smoothstep(-0.08,0.0,ray.y);
  }
  results[id.x]=vec4f(sum/32768.0,1.0);return;
 }
 var ray=vec3f(0.0,1.0,0.0);
 if id.x==3u {ray=normalize(vec3f(1.0,0.1,0.0));}
 if id.x==4u {ray=normalize(vec3f(1.0,-0.04,0.0));}
 if id.x==5u {ray=vec3f(0.0,-1.0,0.0);}
 if id.x<6u {results[id.x]=vec4f(bg_environment_radiance(ray),1.0);return;}
 if id.x<10u {
  if id.x==7u {ray=-ray;}
  if id.x==8u {ray=vec3f(1.0,0.0,0.0);}
  let sky=select(1.0,0.0,id.x==9u);
  results[id.x]=vec4f(bg_indirect_daylight(ray,camera.sun,sky),1.0);return;
 }
 results[id.x]=vec4f(bg_bsl_sky_default(ray,camera.sun.xyz,brightness,
     camera.sun_radiance.w,camera.ambient_upper.w)*camera.sky_zenith.w,1.0);
}
"#,
    ))
}

#[test]
fn source_sky_diffuse_and_raster_environment_fixture_validates() {
    let module = wgpu::naga::front::wgsl::parse_str(&source()).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}

#[test]
fn lunar_energy_and_rain_follow_the_source_without_changing_local_controls() {
    let night = Atmosphere::at(crate::daylight::CYCLE_MS * 3 / 4);
    let half = Atmosphere {
        moon_phase: 4,
        ..night
    };
    assert!(half.ambient().1.distance(night.ambient().1 * 0.25) < 1e-7);
    let noon = Atmosphere::at(crate::daylight::INITIAL_MS);
    let rain = Atmosphere {
        rain_strength: 1.0,
        ..noon
    };
    assert!(
        rain.ambient()
            .1
            .distance(Vec3::new(0.027155, 0.043987, 0.057004))
            < 0.00002
    );
    let cloud = Atmosphere { cloud: 1.0, ..noon };
    assert_eq!(
        cloud.ambient(),
        noon.ambient(),
        "base sky excludes cloud volume"
    );
    let tuned = Atmosphere {
        lighting: crate::config::lighting::Lighting {
            ambient_intensity: 2.0,
            ..noon.lighting
        },
        ..noon
    };
    assert_eq!(tuned.ambient().1, noon.ambient().1 * 2.0);
    assert_eq!(tuned.sun_radiance(), noon.sun_radiance());
}

#[test]
fn gpu_source_sky_convolution_and_raster_environment_match_real_camera_units() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("actual BSL sky convolution and raster environment"),
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
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 11 * 16,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 11 * 16,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let noon = Atmosphere::at(crate::daylight::INITIAL_MS);
    let night = Atmosphere::at(crate::daylight::CYCLE_MS * 3 / 4);
    let sunset = Atmosphere::at(crate::daylight::CYCLE_MS / 48);
    let cases = [
        noon,
        sunset,
        night,
        Atmosphere {
            moon_phase: 4,
            ..night
        },
        Atmosphere {
            rain_strength: 1.0,
            ..noon
        },
        Atmosphere {
            rain_strength: 0.37,
            ..sunset
        },
        Atmosphere {
            lighting: crate::config::lighting::Lighting {
                environment_intensity: 0.4,
                ..noon.lighting
            },
            ..noon
        },
        Atmosphere {
            lighting: crate::config::lighting::Lighting {
                environment_intensity: 0.0,
                ..night.lighting
            },
            ..night
        },
    ];
    for (case, atmosphere) in cases.into_iter().enumerate() {
        let data = atmosphere.camera_data(Mat4::IDENTITY, Vec3::ZERO);
        let camera = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&data),
            usage: wgpu::BufferUsages::UNIFORM,
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
            pass.dispatch_workgroups(11, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, 11 * 16);
        queue.submit([encoder.finish()]);
        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |result| result.unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        {
            let bytes = slice.get_mapped_range().unwrap();
            let rows: &[[f32; 4]] = bytemuck::cast_slice(&bytes);
            let rgb = |i: usize| Vec3::new(rows[i][0], rows[i][1], rows[i][2]);
            let (lower, upper) = atmosphere.ambient();
            // Runtime interpolation is approximate only between elevation/rain knots.
            let tolerance = upper.length() * 0.012 + 0.00002;
            assert!(
                rgb(0).distance(upper) < tolerance,
                "case {case}: GPU {:?}, LUT {upper:?}",
                rgb(0)
            );
            assert!(
                rgb(1).distance(lower) < 0.00003,
                "case {case}: GPU {:?}, LUT {lower:?}",
                rgb(1)
            );
            assert!(
                rgb(2).distance(rgb(10)) < 0.000001,
                "linear source reflection case {case}"
            );
            assert_eq!(rgb(5), Vec3::ZERO, "no synthetic ground case {case}");
            assert_eq!(rgb(9), Vec3::ZERO, "sealed receiver case {case}");
            assert!(rgb(6).distance(upper) < 1e-7);
            assert!(rgb(7).distance(lower) < 1e-7);
            assert!(rgb(8).distance((upper + lower) * 0.5) < 1e-7);
            if atmosphere.lighting.environment_intensity == 0.0 {
                for i in 2..6 {
                    assert_eq!(rgb(i), Vec3::ZERO);
                }
            } else {
                assert!(rgb(3).min_element() > 0.0);
                assert!(rgb(4).min_element() > 0.0);
            }
        }
        readback.unmap();
    }
}
