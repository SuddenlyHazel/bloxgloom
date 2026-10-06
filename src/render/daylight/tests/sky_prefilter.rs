//! Execute actual companion, water, SSR and path replacement callers together.
use super::super::*;
use wgpu::util::DeviceExt;

fn function(source: &str, name: &str) -> String {
    let start = source.find(&format!("fn {name}(")).unwrap();
    let body = start + source[start..].find('{').unwrap();
    let mut depth = 0;
    for (offset, byte) in source[body..].bytes().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return source[start..=body + offset].to_owned();
                }
            }
            _ => {}
        }
    }
    unreachable!()
}

fn source(reference: bool) -> String {
    let camera = include_str!("../../pipeline.wgsl")
        .lines()
        .find(|line| line.starts_with("struct Camera"))
        .unwrap();
    format!(
        "{}\n{}\n{}\nconst BG_FOG_REFERENCE:bool={reference};\nconst BG_FOG_BSL_STYLE:bool=true;\nconst BG_FOG_NOON_HEIGHT:f32={};\n{camera}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        crate::render::sky::STYLE_SHADER,
        include_str!("../../material/pbr.wgsl"),
        include_str!("../../material/sky_prefilter.wgsl"),
        crate::render::SUN_DIRECTION.normalize().y,
        include_str!("../../daylight/prefilter.wgsl"),
        crate::render::bsl_reference::REFLECTION_SHADER,
        include_str!("sky_prefilter_fixture.wgsl"),
        function(
            include_str!("../../material/companions.wgsl"),
            "bg_material_highlight"
        ),
        include_str!("../../scene_ao_output.wgsl"),
        include_str!("../../water_surface.wgsl"),
        function(
            include_str!("../../reflections.wgsl"),
            "bg_reflection_fallback"
        ),
        function(
            include_str!("../../trace/composite.wgsl"),
            "ray_specular_fallback"
        ),
    )
}

#[test]
fn actual_sky_prefilter_callers_validate_in_enhanced_and_reference_modes() {
    for reference in [false, true] {
        let module = wgpu::naga::front::wgsl::parse_str(&source(reference)).unwrap();
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
    }
}

#[test]
fn gpu_actual_material_water_ssr_and_path_sky_fallbacks_match_source_and_cancel() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("actual sky fallback regression"),
        size: 12 * 16,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: output.size(),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let noon = Atmosphere::at(crate::daylight::INITIAL_MS);
    let night = Atmosphere::at(crate::daylight::CYCLE_MS * 3 / 4);
    let cases = [
        noon,
        Atmosphere::at(crate::daylight::CYCLE_MS / 2 - crate::daylight::CYCLE_MS / 48),
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
            rain_strength: 0.35,
            ..noon
        },
        Atmosphere {
            rain_strength: 1.0,
            ..night
        },
        Atmosphere { cloud: 1.0, ..noon },
        Atmosphere {
            lighting: crate::config::lighting::Lighting {
                environment_intensity: 0.0,
                ..noon.lighting
            },
            ..noon
        },
        Atmosphere {
            lighting: crate::config::lighting::Lighting {
                environment_intensity: 2.3,
                ..noon.lighting
            },
            ..noon
        },
        Atmosphere::at(crate::daylight::CYCLE_MS / 48),
        Atmosphere {
            moon_phase: 4,
            rain_strength: 0.6,
            ..night
        },
    ];
    for reference in [false, true] {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("actual material sky callers"),
            source: wgpu::ShaderSource::Wgsl(source(reference).into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        for (case, atmosphere) in cases.into_iter().enumerate() {
            let mut camera = atmosphere.camera_data(Mat4::IDENTITY, Vec3::new(0.0, 1.0, 1.0));
            // Direct/local transport are separately tested; isolate reflections.
            camera[36..39].fill(0.0);
            if reference {
                // Actual reference aliases: 43=moon, 47=sun visibility, 51=fade.
                camera[43] = atmosphere.moon_multiplier();
                camera[47] = 0.5;
                camera[51] = 0.25;
            }
            let mut reflection = [0.0f32; 56];
            reflection[36..40].copy_from_slice(&camera[20..24]);
            reflection[40..44].copy_from_slice(&camera[40..44]);
            reflection[48..51].copy_from_slice(&atmosphere.sun.to_array());
            reflection[51] = atmosphere.time_brightness();
            reflection[52] = atmosphere.rain_strength;
            reflection[53] = atmosphere.moon_multiplier();
            reflection[55] = f32::from(!reference);
            let mut ray = [0.0f32; 72];
            ray[36..40].copy_from_slice(&reflection[48..52]);
            ray[44..48].copy_from_slice(&reflection[36..40]);
            ray[48..52].copy_from_slice(&reflection[40..44]);
            ray[55] = f32::from(!reference);
            ray[56..58].copy_from_slice(&reflection[52..54]);
            reflection[54] = f32::from(reference);
            let buffers = [camera.as_slice(), reflection.as_slice(), ray.as_slice()].map(|data| {
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: bytemuck::cast_slice(data),
                    usage: wgpu::BufferUsages::UNIFORM,
                })
            });
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: buffers[0].as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: output.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: buffers[1].as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: buffers[2].as_entire_binding(),
                    },
                ],
            });
            let mut encoder = device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_compute_pass(&Default::default());
                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, &group, &[]);
                pass.dispatch_workgroups(12, 1, 1);
            }
            encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, output.size());
            queue.submit([encoder.finish()]);
            let slice = readback.slice(..);
            slice.map_async(wgpu::MapMode::Read, |r| r.unwrap());
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            {
                let bytes = slice.get_mapped_range().unwrap();
                let values: &[[f32; 4]] = bytemuck::cast_slice(&bytes);
                let rgb = |i: usize| Vec3::from_slice(&values[i][..3]);
                let tolerance = rgb(1).length() * 0.0002 + 0.000002;
                assert!(
                    rgb(0).distance(rgb(1)) < tolerance,
                    "actual companion case={case} reference={reference}: {:?}",
                    &values[..2]
                );
                assert!(
                    rgb(2).distance(rgb(3)) < tolerance,
                    "actual water case={case} reference={reference}: {:?}",
                    &values[2..4]
                );
                for (i, value) in values.iter().enumerate().take(10).skip(4) {
                    assert!(
                        Vec3::from_slice(&value[..3]).abs().max_element() < 0.000003,
                        "replacement/legacy/default row={i} case={case} reference={reference}: {value:?}"
                    );
                }
                if !reference && case == 0 {
                    eprintln!(
                        "actual rough sky source={:?}, legacy={:?}",
                        rgb(10),
                        rgb(11)
                    );
                    assert!(
                        rgb(11).distance(rgb(10)) > 0.1,
                        "test must distinguish legacy and source sky: source={:?}, legacy={:?}",
                        rgb(10),
                        rgb(11)
                    );
                }
            }
            readback.unmap();
        }
    }
}
