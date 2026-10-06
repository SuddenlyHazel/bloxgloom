//! Read back the production height trace against analytic synthetic surfaces.
use wgpu::util::DeviceExt;

const CASES: usize = 21;
const FIXTURE: &str = r#"
@group(0) @binding(0) var<storage, read_write> result: array<vec4f>;
@group(0) @binding(1) var material_normal: texture_2d_array<f32>;
@group(0) @binding(2) var material_sampler: sampler;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3u) {
    let uv = vec2f(0.6, 0.4);
    if id.x < 4u {
        result[id.x] = vec4f(bg_parallax_trace(uv, i32(id.x), vec2f(0.0), vec2f(0.0), vec2f(0.08, 0.0), 32u), 0.0, 1.0);
    } else if id.x >= 15u {
        var layer = 4;
        var light = normalize(vec3f(1.0,0.0,0.5));
        if id.x == 15u { layer = 0; }
        if id.x == 16u { layer = 1; }
        if id.x == 18u { light.x = -light.x; }
        if id.x == 19u { light = vec3f(0.0,0.0,1.0); }
        if id.x == 20u { light.z = -light.z; }
        result[id.x] = vec4f(bg_parallax_shadow(vec2f(0.55,0.4),layer,vec2f(0.0),vec2f(0.0),light,0.08),0.0,0.0,1.0);
    } else {
        var view = normalize(vec3f(0.6, 0.0, 0.8));
        var distance = 2.0;
        var mip = 0.0;
        var settings = vec4f(0.035, 32.0, 32.0, 0.0);
        if id.x == 5u { view.x = -view.x; }
        if id.x == 6u { distance = 32.0; }
        if id.x == 7u { mip = 4.0; }
        if id.x == 8u { view = vec3f(1.0, 0.0, 0.0); }
        if id.x == 9u { view = vec3f(0.0, 0.0, 1.0); }
        if id.x == 10u { view = vec3f(0.6, 0.0, -0.8); }
        if id.x == 11u { settings.x = 0.0; }
        if id.x == 12u { settings.x = 0.07; }
        if id.x == 13u { distance = 24.0; settings.y = 16.0; }
        if id.x == 14u { distance = 24.0; settings.y = 64.0; }
        let ray = bg_parallax_ray(view, distance, mip, settings);
        result[id.x] = vec4f(bg_parallax_trace(uv, 1, vec2f(0.0), vec2f(0.0), ray, 16u), ray);
    }
}
"#;

fn source() -> String {
    // The texture trace is shared verbatim; only the fragment derivative wrapper
    // is omitted from this compute-stage readback.
    let trace = include_str!("../../parallax.wgsl")
        .split("fn bg_material_coordinates")
        .next()
        .unwrap();
    format!("{}\n{trace}\n{FIXTURE}", include_str!("../../relief.wgsl"))
}

#[test]
fn parallax_trace_shader_validates_without_a_gpu() {
    let module = wgpu::naga::front::wgsl::parse_str(&source()).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}

#[test]
fn gpu_height_trace_intersects_surfaces_and_fades_without_grazing_instability() {
    let instance = wgpu::Instance::default();
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
        eprintln!("parallax GPU test skipped: no adapter");
        return;
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("parallax height regression"),
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
    let height = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("flat and ramp height fixtures"),
        size: wgpu::Extent3d {
            width: 128,
            height: 1,
            depth_or_array_layers: 5,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let pixels: Vec<u8> = (0..5)
        .flat_map(|layer| {
            (0..128).flat_map(move |x| {
                let alpha = match layer {
                    0 => 255,
                    1 => 128,
                    2 => 0,
                    3 => x * 2,
                    _ => {
                        if x >= 77 {
                            255
                        } else {
                            128
                        }
                    }
                };
                [128, 128, 255, alpha]
            })
        })
        .collect();
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &height,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(512),
            rows_per_image: Some(1),
        },
        height.size(),
    );
    let view = height.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let output = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &[0; CASES * 16],
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (CASES * 16) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: output.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(CASES as u32, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, (CASES * 16) as u64);
    queue.submit([encoder.finish()]);
    let slice = readback.slice(..);
    slice.map_async(wgpu::MapMode::Read, |result| result.unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let bytes = slice.get_mapped_range().unwrap();
    let rows: &[[f32; 4]] = bytemuck::cast_slice(&bytes);
    assert!(rows.iter().flatten().all(|v| v.is_finite()));
    let depth = 127.0 / 255.0;
    for (i, expected) in [0.6, 0.6 - 0.08 * depth, 0.52].into_iter().enumerate() {
        assert!((rows[i][0] - expected).abs() < 1e-5, "height case {i}");
        assert!((rows[i][1] - 0.4).abs() < 1e-5);
    }
    // For this linear ramp, alpha(u)=(256*u-1)/255 and depth=(1-alpha).
    let ramp_expected = (0.6 - 0.08 * 256.0 / 255.0) / (1.0 - 0.08 * 256.0 / 255.0);
    assert!((rows[3][0] - ramp_expected).abs() < 1e-5);
    assert!(rows[4][0] < 0.6 && rows[5][0] > 0.6);
    assert!((rows[4][0] + rows[5][0] - 1.2).abs() < 1e-5);
    for row in rows[6..12].iter().chain([&rows[13]]) {
        assert!((row[0] - 0.6).abs() < 1e-5);
        assert_eq!(&row[2..], &[0.0, 0.0]);
    }
    assert!((0.6 - rows[12][0] - 2.0 * (0.6 - rows[4][0])).abs() < 1e-5);
    assert!((rows[14][0] - rows[4][0]).abs() < 1e-5);
    assert_eq!(rows[15][0], 1.0, "flat face has no relief shadow");
    assert_eq!(rows[16][0], 1.0, "uniform recess cannot shadow itself");
    assert!(rows[17][0] < 0.1, "ridge occludes oblique sun");
    assert_eq!(
        rows[18][0], 1.0,
        "opposite light direction clears the ridge"
    );
    assert_eq!(rows[19][0], 1.0, "overhead light clears the ridge");
    assert_eq!(
        rows[20][0], 0.0,
        "recess cannot see light through its backing plane"
    );
}
