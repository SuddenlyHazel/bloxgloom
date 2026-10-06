//! Uses synthetic distributable noise, real linear texture sampling and actual
//! source-comparison volume helper. Original BSL noise is never test-bundled.
use wgpu::util::DeviceExt;
#[test]
fn gpu_reference_cloud_defaults_match_density_dither_and_volume_goldens() {
    let instance = wgpu::Instance::default();
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
        eprintln!("reference clouds GPU skipped: no adapter");
        return;
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let fixture = r#"
@group(0) @binding(3) var<storage,read_write> rows:array<vec4f>;
@compute @workgroup_size(1) fn check(@builtin(global_invocation_id) id:vec3u) {
let i=id.x;
if i<3u {
 let rain=select(0.0,1.0,i==1u);let reveal=select(0.0,1.0,i==2u);
 rows[i]=vec4f(bg_reference_cloud_density(0.6,0.4,0.0,reveal,rain),bg_reference_cloud_density(0.6,0.4,0.125,reveal,rain),bg_reference_cloud_density(0.6,0.4,0.5,reveal,rain),bg_reference_cloud_density(0.6,0.4,1.0,reveal,rain));
} else if i==3u {rows[i]=vec4f(bg_reference_cloud_dither(vec2f(1.5,2.5),0.0),bg_reference_cloud_dither(vec2f(1.5,2.5),17.0),bg_reference_cloud_dither(vec2f(6.5,7.5),17.0),bg_reference_cloud_dither(vec2f(6.5,7.5),18.0));}
else if i==4u {rows[i]=vec4f(bg_reference_cloud_sample(vec3f(128.0,207.0,256.0),vec2f(0.07,-0.02),0.0,0.0),bg_reference_cloud_sample(vec3f(-640.0,229.8,384.0),vec2f(0.07,-0.02),0.0,0.0),bg_reference_cloud_sample(vec3f(333.0,248.4,-921.0),vec2f(0.07,-0.02),0.0,0.0),1.0);}
else if i<9u {
 let origins=array<vec3f,4>(vec3f(0.0,100.0,0.0),vec3f(0.0,222.0,0.0),vec3f(0.0,100.0,0.0),vec3f(0.0,100.0,0.0));
 let rays=array<vec3f,4>(vec3f(0.8,0.6,0.0),vec3f(0.8,0.6,0.0),vec3f(0.8,-0.6,0.0),vec3f(0.0,1.0,0.0));
 rows[i]=bg_reference_cloud_integrate(origins[i-5u],rays[i-5u],vec2f(1.5,2.5),sky_camera);
} else if i<11u {
 var camera=sky_camera;
 if i==9u {
 camera.climate.x=1.0;
 let raw=bg_bsl_daylight_palette(1.0);let weather=vec3f(176.0,224.0,255.0)*(1.2/255.0);let tinted=weather*bg_bsl_luminance(raw);camera.sun_radiance=vec4f(tinted*tinted,1.0);
 } else {camera.sun.y=-1.0;camera.reference.w=-1.0;camera.sun_radiance=vec4f(pow(vec3f(96.0,192.0,255.0)*(0.3/255.0),vec3f(2.0)),0.0);}
 rows[i]=bg_reference_cloud_integrate(vec3f(0.0,100.0,0.0),vec3f(0.8,0.6,0.0),vec2f(1.5,2.5),camera);
} else {
 let origins=array<vec3f,3>(vec3f(0.0,100.0,0.0),vec3f(0.0,222.0,0.0),vec3f(0.0,100.0,0.0));
 let rays=array<vec3f,3>(vec3f(0.9949874371,0.1,0.0),vec3f(0.8,0.6,0.0),vec3f(0.8,0.6,0.0));
 let heights=array<f32,3>(100.0,100.0,-70.0);
 rows[i]=bg_reference_cloud_integrate_case(origins[i-11u],rays[i-11u],vec2f(1.5,2.5),sky_camera,heights[i-11u],true);
}}
"#;
    let source = format!(
        "{}\n{}\n{}\n{fixture}",
        crate::render::sky::STYLE_SHADER,
        include_str!("../camera.wgsl"),
        super::super::SHADER
    );
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("reference clouds known inputs"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: Some("check"),
        compilation_options: Default::default(),
        cache: None,
    });
    let mut camera = [0f32; 40];
    camera[13] = 1.;
    camera[24] = (196. / 255. * 1.4f32).powi(2);
    camera[25] = (220. / 255. * 1.4f32).powi(2);
    camera[26] = 1.96;
    camera[27] = 1.;
    camera[29] = 100.;
    camera[33] = 1.;
    camera[36] = 1.;
    camera[37] = 1.;
    camera[38] = 17.;
    camera[39] = 1.;
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&camera),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("synthetic constant linear RGBA cloud noise"),
        size: wgpu::Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &[153, 0, 102, 255].repeat(4),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(8),
            rows_per_image: Some(2),
        },
        wgpu::Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        },
    );
    let view = texture.create_view(&Default::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 224,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 224,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: output.as_entire_binding(),
            },
        ],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(14, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &read, 0, 224);
    queue.submit([encoder.finish()]);
    read.slice(..)
        .map_async(wgpu::MapMode::Read, |r| r.unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let mapped = read.slice(..).get_mapped_range().unwrap();
    let rows: &[[f32; 4]] = bytemuck::cast_slice(&mapped);
    let close = |actual: f32, expected: f64| {
        assert!(
            (f64::from(actual) - expected).abs() < 3e-5,
            "{actual} vs {expected}"
        )
    };
    for (i, row) in rows[..3].iter().enumerate() {
        for (j, g) in [0., 0.125, 0.5, 1.].into_iter().enumerate() {
            close(
                row[j],
                super::oracle::density(
                    0.6,
                    0.4,
                    g,
                    if i == 2 { 1. } else { 0. },
                    if i == 1 { 1. } else { 0. },
                ),
            );
        }
    }
    for (j, (pixel, frame)) in [
        ([1.5, 2.5], 0.),
        ([1.5, 2.5], 17.),
        ([6.5, 7.5], 17.),
        ([6.5, 7.5], 18.),
    ]
    .into_iter()
    .enumerate()
    {
        close(rows[3][j], super::oracle::dither(pixel, frame));
    }
    for (actual, g) in rows[4][..3].iter().zip([0.25, 0.63, 0.94]) {
        close(*actual, super::oracle::density(0.6, 0.4, g, 0., 0.));
    }
    for (i, (origin, ray)) in [
        ([0., 100., 0.], [0.8, 0.6, 0.]),
        ([0., 222., 0.], [0.8, 0.6, 0.]),
        ([0., 100., 0.], [0.8, -0.6, 0.]),
        ([0., 100., 0.], [0., 1., 0.]),
    ]
    .into_iter()
    .enumerate()
    {
        let expected = super::oracle::integrate(origin, ray, [1.5, 2.5], 0., false);
        for (a, e) in rows[i + 5].iter().zip(expected) {
            close(*a, e);
        }
    }
    for (i, rain, night) in [(9, 1., false), (10, 0., true)] {
        let expected =
            super::oracle::integrate([0., 100., 0.], [0.8, 0.6, 0.], [1.5, 2.5], rain, night);
        for (a, e) in rows[i].iter().zip(expected) {
            close(*a, e);
        }
    }
    for (i, origin, ray, eye_height) in [
        (11, [0., 100., 0.], [0.9949874371, 0.1, 0.], 100.),
        (12, [0., 222., 0.], [0.8, 0.6, 0.], 100.),
        (13, [0., 100., 0.], [0.8, 0.6, 0.], -70.),
    ] {
        let expected =
            super::oracle::integrate_case(origin, ray, [1.5, 2.5], 0., false, eye_height, true);
        for (a, e) in rows[i].iter().zip(expected) {
            close(*a, e);
        }
    }
    assert!(
        rows[11][3] > 0.99,
        "source fast-fade reflection must clear beyond its cloud range"
    );
    assert!(
        super::oracle::integrate(
            [0., 100., 0.],
            [0.9949874371, 0.1, 0.],
            [1.5, 2.5],
            0.,
            false
        )[3] < 0.99,
        "control must distinguish ordinary sky from reflected cloud fade"
    );
    assert!(
        rows[13][..3].iter().all(|v| *v == 0.),
        "reflection must use camera altitude for underground attenuation"
    );
    drop(mapped);
    read.unmap();
    // Distinct R/B values exercise linear repeat, wind, UV scale and vertical
    // slice interpolation rather than proving those against constant noise.
    let varying = [
        166, 0, 25, 255, 230, 0, 200, 255, 204, 0, 90, 255, 255, 0, 240, 255,
    ];
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &varying,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(8),
            rows_per_image: Some(2),
        },
        wgpu::Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        },
    );
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(5, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &read, 0, 224);
    queue.submit([encoder.finish()]);
    read.slice(..)
        .map_async(wgpu::MapMode::Read, |r| r.unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let mapped = read.slice(..).get_mapped_range().unwrap();
    let rows: &[[f32; 4]] = bytemuck::cast_slice(&mapped);
    for (actual, position) in rows[4][..3].iter().zip([
        [128., 207., 256.],
        [-640., 229.8, 384.],
        [333., 248.4, -921.],
    ]) {
        let expected = super::oracle::sample(&varying, position, [0.07, -0.02]);
        // Filtering weights for RGBA8 textures have finite hardware precision;
        // density math and constant-noise volume retain the stricter 3e-5 bound.
        assert!(
            (f64::from(*actual) - expected).abs() < 0.001,
            "linear RGBA8 repeat sample {position:?}: {actual} vs {expected}"
        );
    }
}
