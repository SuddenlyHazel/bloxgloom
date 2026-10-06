//! The complete raster composite must obey measured path length, leave
//! interface reflections alone, and preserve unconfigured fallback draws.
use wgpu::util::DeviceExt;

#[test]
fn gpu_raster_water_transmission_matches_beer_and_unconfigured_fallback() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let source = super::super::shader(
        r#"
struct Camera { view_projection:mat4x4f,sun:vec4f,horizon:vec4f,eye:vec4f,fog_range:vec4f,
 parallax:vec4f,sun_radiance:vec4f,sky_zenith:vec4f,ambient_lower:vec4f,ambient_upper:vec4f,cloud:vec4f,ambient_sh:array<vec4f,6> };
@group(0) @binding(0) var<uniform> camera:Camera;
@group(2) @binding(0) var<storage,read_write> output:array<vec4f>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id:vec3u) {
 let x=(f32(id.x)+0.5)/12.0*2.0-1.0;
 let sky=select(1.0,0.0,id.x>=10u);
 let color=vec4f(0.02,0.13,0.27,0.62);let normal=vec3f(0.0,1.0,0.0);
 let world=vec3f(x,0.0,0.0);let relative=vec3f(0.0,-10.0,0.0);
 let footprint=vec4f(1000.0,0.0,0.0,1000.0);
 let result=bg_enhanced_water_surface(color,normal,sky,0.0,world,relative,true,0.0,0.0,footprint,vec2f(f32(id.x)+0.5,0.5));
 output[id.x]=result.color;
 output[id.x+12u]=bg_water_surface(color,normal,sky,0.0,world,relative,true,0.0,0.0,footprint).color;
 output[id.x+24u]=result.reflection_response;
}
"#,
    );
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let mut atmosphere = crate::render::daylight::Atmosphere::at(6000);
    atmosphere.sun = glam::Vec3::Y;
    let camera = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(
            &atmosphere.camera_data(glam::Mat4::IDENTITY, glam::Vec3::new(0.0, 10.0, 0.0)),
        ),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let camera_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: camera.as_entire_binding(),
        }],
    });
    let mut inputs = super::Inputs::fallback(&device);
    // Upload textures are separate from render-only production snapshots.
    let upload = |format| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 12,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
    };
    let depth = upload(wgpu::TextureFormat::R32Float);
    let color = upload(crate::render::post::HDR_FORMAT);
    let depths: Vec<f32> = [0.0f32, 1.0, 4.0, 16.0, 64.0, 16.0]
        .into_iter()
        .flat_map(|d| [d / 100.0; 2])
        .collect();
    let colors: Vec<u16> = (0..12)
        .flat_map(|i| {
            let value = if i % 2 == 1 && i < 10 { 0x3c00u16 } else { 0 };
            [value, value, value, 0x3c00]
        })
        .collect();
    let layout = |bytes_per_row| wgpu::TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(bytes_per_row),
        rows_per_image: Some(1),
    };
    queue.write_texture(
        depth.as_image_copy(),
        bytemuck::cast_slice(&depths),
        layout(48),
        depth.size(),
    );
    queue.write_texture(
        color.as_image_copy(),
        bytemuck::cast_slice(&colors),
        layout(96),
        color.size(),
    );
    inputs.depth = depth.create_view(&Default::default());
    inputs.color = color.create_view(&Default::default());
    let water_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(1),
        entries: &inputs.entries(1),
    });
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 36 * 16,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 36 * 16,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let output_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(2),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: output.as_entire_binding(),
        }],
    });
    for configured in [true, false] {
        let mut frame = [0.0f32; 24];
        frame[..16].copy_from_slice(&[
            1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, -100.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ]);
        frame[20..23].copy_from_slice(&[12.0, 1.0, f32::from(configured)]);
        queue.write_buffer(&inputs.frame, 0, bytemuck::cast_slice(&frame));
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &camera_group, &[]);
            pass.set_bind_group(1, &water_group, &[]);
            pass.set_bind_group(2, &output_group, &[]);
            pass.dispatch_workgroups(12, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, 36 * 16);
        queue.submit([encoder.finish()]);
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, |r| r.unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        {
            let bytes = readback.slice(..).get_mapped_range().unwrap();
            let rows: &[[f32; 4]] = bytemuck::cast_slice(&bytes);
            if configured {
                for (i, path) in [0.0f64, 1.0, 4.0, 16.0, 64.0].iter().enumerate() {
                    for (channel, sigma) in
                        [0.3406803136f64, 0.0579, 0.0125513234].iter().enumerate()
                    {
                        let actual = f64::from(rows[2 * i + 1][channel] - rows[2 * i][channel]);
                        let expected = 0.98 * (-sigma * path).exp();
                        assert!(
                            (actual - expected).abs() < 2e-6,
                            "path {path}, channel {channel}: {actual} expected {expected}"
                        );
                    }
                    assert_eq!(rows[2 * i][3], 1.0);
                    assert!(rows[24 + 2 * i][..3].iter().all(|v| v.is_finite()));
                    for (actual, original) in rows[24 + 2 * i][..3].iter().zip(&rows[24][..3]) {
                        assert!(
                            (actual - original).abs() < 1e-6,
                            "depth absorption changed interface reflection strength"
                        );
                    }
                }
                assert!(
                    rows[10][..3].iter().all(|v| *v < 0.01),
                    "sealed cave water invented exterior energy: {:?}",
                    rows[10]
                );
            } else {
                for i in 0..12 {
                    for (actual, original) in rows[i].iter().zip(&rows[i + 12]) {
                        assert!(
                            (actual - original).abs() < 1e-6,
                            "fallback altered standalone water"
                        );
                    }
                }
            }
        }
        readback.unmap();
    }
}
