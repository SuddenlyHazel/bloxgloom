use super::*;
use wgpu::util::DeviceExt;
const WIDTH: u32 = 96;
const HEIGHT: u32 = 64;

#[test]
fn reflection_shaders_validate_without_a_gpu() {
    for source in [shader_source(), include_str!("downsample.wgsl").to_owned()] {
        let module = wgpu::naga::front::wgsl::parse_str(&source).unwrap();
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
    }
}
#[test]
fn gpu_reflections_hit_scene_color_preserve_misses_and_do_not_invent_cave_light() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut reflections = Reflections::new(&device, WIDTH, HEIGHT);
    reflections.enabled = true;
    if reflections.gpu.is_none() {
        return;
    }
    let eye = Vec3::new(0.0, 1.8, 5.0);
    let view = glam::camera::rh::view::look_at_mat4(eye, Vec3::new(0.0, 0.2, -1.0), Vec3::Y);
    let matrix = glam::camera::rh::proj::directx::perspective(
        60f32.to_radians(),
        WIDTH as f32 / HEIGHT as f32,
        0.1,
        40.0,
    ) * view;
    let mut atmosphere = super::super::daylight::Atmosphere::at(6000);
    atmosphere.horizon = Vec3::new(0.59, 0.72, 0.82);
    atmosphere.zenith = Vec3::new(0.20, 0.45, 0.75);
    atmosphere.lighting.environment_intensity = 1.0;
    reflections.configure(eye, atmosphere);
    let black = fixture(
        &device,
        &queue,
        &reflections,
        matrix,
        eye,
        0.0,
        0.2,
        0.0,
        0.0,
        0.0,
    );
    let mirror = fixture(
        &device,
        &queue,
        &reflections,
        matrix,
        eye,
        1.0,
        0.2,
        0.0,
        0.0,
        0.0,
    );
    let rough = fixture(
        &device,
        &queue,
        &reflections,
        matrix,
        eye,
        1.0,
        1.0,
        0.0,
        0.0,
        0.0,
    );
    let water = fixture(
        &device,
        &queue,
        &reflections,
        matrix,
        eye,
        1.0,
        0.12,
        0.3,
        0.0,
        0.0,
    );
    let fallback = fixture(
        &device,
        &queue,
        &reflections,
        matrix,
        eye,
        1.0,
        0.2,
        0.0,
        1.0,
        0.0,
    );
    let polluted = fixture(
        &device,
        &queue,
        &reflections,
        matrix,
        eye,
        0.0,
        0.2,
        0.0,
        0.0,
        100.0,
    );
    for (reference, sky) in rough.chunks_exact(4).zip(polluted.chunks_exact(4)) {
        if (reference[0] - 0.02).abs() < 0.001 {
            assert!(
                sky[..3].iter().all(|v| v.abs() < 0.00001),
                "invalid sky pixels must not leak through rough reflection mips"
            );
        }
    }
    let mut reflected = 0;
    let mut water_reflected = 0;
    for ((black, mirror), (rough, water)) in black
        .chunks_exact(4)
        .zip(mirror.chunks_exact(4))
        .zip(rough.chunks_exact(4).zip(water.chunks_exact(4)))
    {
        for pixel in [black, mirror, rough, water] {
            assert!(pixel.iter().all(|v| v.is_finite()));
        }
        // Receiver floor has tiny neutral local energy and zero sky visibility.
        // The black wall cannot manufacture illumination despite bright sky.
        assert!(
            black[..3].iter().all(|v| v.abs() < 0.00001),
            "zero scene radiance cannot acquire sky through SSR"
        );
        if (rough[0] - 0.02).abs() < 0.001 {
            assert!(
                (rough[0] - 0.02).abs() < 0.001,
                "rough nonreceivers retain fallback"
            );
            if mirror[0] - mirror[1] > 0.02 {
                reflected += 1;
            }
            if water[0] - water[1] > 0.02 {
                water_reflected += 1;
            }
        }
        assert!(
            (mirror[3] - 1.0).abs() < 0.001,
            "reflection resolve preserves HDR alpha"
        );
    }
    assert!(
        reflected > 40,
        "opaque floor should reflect real red geometry, pixels={reflected}"
    );
    assert!(
        water_reflected > 40,
        "transparent receiver must use its own linear distance, pixels={water_reflected}"
    );
    let replaced = mirror
        .chunks_exact(4)
        .zip(fallback.chunks_exact(4))
        .filter(|(base, with_sky)| {
            base[0] - base[1] > 0.02
                && (base[0] - with_sky[0]).abs() < 0.005
                && (base[1] - with_sky[1]).abs() < 0.005
        })
        .count();
    assert!(
        replaced > 40,
        "confident hits replace sky instead of adding a second reflection, pixels={replaced}"
    );
    // Real SSR replacement must subtract the artistic raster fallback, while
    // explicitly marked water retains its own enhanced fallback in that mode.
    reflections.artistic = true;
    reflections.atmosphere.moon_phase = 4;
    for receiver_height in [0.0, 0.3] {
        let base = fixture(
            &device,
            &queue,
            &reflections,
            matrix,
            eye,
            1.0,
            0.2,
            receiver_height,
            0.0,
            0.0,
        );
        let sky = fixture(
            &device,
            &queue,
            &reflections,
            matrix,
            eye,
            1.0,
            0.2,
            receiver_height,
            1.0,
            0.0,
        );
        let replaced = base
            .chunks_exact(4)
            .zip(sky.chunks_exact(4))
            .filter(|(base, sky)| {
                base[0] - base[1] > 0.02
                    && (base[0] - sky[0]).abs() < 0.005
                    && (base[1] - sky[1]).abs() < 0.005
            })
            .count();
        assert!(
            replaced > 40,
            "artistic/water fallback replaced once: height{receiver_height}, pixels{replaced}"
        );
    }
    reflections.resize(&device, 1, 1);
    assert_eq!(reflections.normal.texture().width(), 1);
    assert_eq!(reflections.targets.levels.len(), 1);
}
#[allow(clippy::too_many_arguments)]
fn fixture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    reflections: &Reflections,
    matrix: Mat4,
    eye: Vec3,
    wall: f32,
    roughness: f32,
    receiver_height: f32,
    sky_visibility: f32,
    invalid_background: f32,
) -> Vec<f32> {
    let scene = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("reflection regression HDR"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: super::super::post::HDR_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let scene = scene.create_view(&Default::default());
    let indirect = super::super::scene_ao::create_indirect(device, WIDTH, HEIGHT);
    let depth = device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("reflection regression opaque depth"),
            size: wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: super::super::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default());
    let source = format!(
        "{}\n{}\n{}\n{}\n{}",
        super::super::sky::STYLE_SHADER,
        super::super::bsl_reference::REFLECTION_SHADER,
        include_str!("../material/pbr.wgsl"),
        include_str!("normal.wgsl"),
        r#"
struct Parameters {inverse:mat4x4f,projection:mat4x4f,eye:vec4f,options:vec4f,sun:vec4f,climate:vec4f};
@group(0) @binding(0) var<uniform> params:Parameters;
@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
 let p=array<vec2f,3>(vec2f(-1.0,-1.0),vec2f(3.0,-1.0),vec2f(-1.0,3.0));return vec4f(p[i],0.0,1.0);
}
fn unproject(uv:vec2f,z:f32)->vec3f {let h=params.inverse*vec4f(uv*vec2f(2.0,-2.0)+vec2f(-1.0,1.0),z,1.0);return h.xyz/h.w;}
struct Out {@location(0) color:vec4f,@location(1) indirect:vec4f,@location(2) normal:vec4f,@location(3) response:vec4f,@builtin(frag_depth) depth:f32};
@fragment fn fs(@builtin(position) p:vec4f)->Out {
 let uv=p.xy/vec2f(96.0,64.0);let origin=params.eye.xyz;let ray=normalize(unproject(uv,1.0)-origin);
 var distance=1000.0;var floor=false;
 let floor_t=-origin.y/ray.y;let floor_p=origin+ray*floor_t;
 if floor_t>0.0 && abs(floor_p.x)<6.0 && floor_p.z>-6.0 && floor_p.z<6.0 {distance=floor_t;floor=true;}
 let wall_t=(-3.0-origin.z)/ray.z;let wall_p=origin+ray*wall_t;
 if wall_t>0.0 && wall_t<distance && abs(wall_p.x)<3.5 && wall_p.y>0.0 && wall_p.y<3.0 {distance=wall_t;floor=false;}
 if distance>999.0 {return Out(vec4f(vec3f(params.eye.w),1.0),vec4f(0.0),vec4f(0.0),vec4f(0.0),1.0);}
 let point=origin+ray*distance;let clip=params.projection*vec4f(point,1.0);
 if floor {
  let receiver_t=(params.options.z-origin.y)/ray.y;
  let reflected=reflect(ray,vec3f(0.0,1.0,0.0));
  var sky=bg_pbr_prefiltered_sky(reflected,params.options.y,vec3f(0.59,0.72,0.82),vec4f(0.20,0.45,0.75,params.climate.w));
  if params.climate.z>0.5 && params.options.z<=0.0 {
   sky=bg_bsl_artistic_environment(reflected,params.sun,params.climate.xy);
  }
  return Out(vec4f(vec3f(0.02)*select(0.0,1.0,params.options.x>0.0)+sky*0.2*params.options.w,1.0),vec4f(0.0,0.0,0.0,select(0.0,-2.0,params.options.z>0.0)),vec4f(bg_reflection_oct_encode(vec3f(0.0,1.0,0.0)),params.options.y,receiver_t),vec4f(0.2,0.2,0.2,params.options.w),clip.z/clip.w);
 }
 return Out(vec4f(vec3f(2.0,0.05,0.02)*params.options.x,1.0),vec4f(0.0),vec4f(0.0),vec4f(0.0),clip.z/clip.w);
}
"#
    );
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("reflection synthetic geometry fixture"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let mut parameters = [0.0f32; 48];
    parameters[..16].copy_from_slice(&matrix.inverse().to_cols_array());
    parameters[16..32].copy_from_slice(&matrix.to_cols_array());
    parameters[32..35].copy_from_slice(&eye.to_array());
    parameters[35] = invalid_background;
    parameters[36..40].copy_from_slice(&[wall, roughness, receiver_height, sky_visibility]);
    parameters[40..43].copy_from_slice(&reflections.atmosphere.sun.to_array());
    parameters[43] = reflections.atmosphere.time_brightness();
    parameters[44] = reflections.atmosphere.rain_strength;
    parameters[45] = reflections.atmosphere.moon_multiplier();
    parameters[46] = f32::from(reflections.artistic);
    parameters[47] = reflections.atmosphere.camera_data(Mat4::IDENTITY, eye)[43];
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&parameters),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("reflection regression scene"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &super::super::scene_ao::color_targets(super::super::post::HDR_FORMAT, None),
        }),
        primitive: Default::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: super::super::DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform.as_entire_binding(),
        }],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("reflection fixture geometry"),
            color_attachments: &super::super::scene_ao::attachments(
                &scene,
                &indirect,
                &reflections.normal,
                &reflections.response,
                wgpu::Color::BLACK,
            ),
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }
    reflections.capture_opaque(device, &mut encoder, &scene, &depth);
    reflections.resolve(
        device,
        queue,
        &mut encoder,
        &scene,
        &indirect,
        &depth,
        matrix,
    );
    let row = (WIDTH * 8).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(row * HEIGHT),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: scene.texture(),
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(HEIGHT),
            },
        },
        wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, |result| result.unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let bytes = buffer.slice(..).get_mapped_range().unwrap();
    let mut output = Vec::new();
    for row in bytes.chunks_exact(row as usize) {
        for value in bytemuck::cast_slice::<u8, u16>(&row[..WIDTH as usize * 8]) {
            output.push(half(*value));
        }
    }
    output
}
fn half(bits: u16) -> f32 {
    let sign = if bits & 0x8000 == 0 { 1.0 } else { -1.0 };
    let exponent = (bits >> 10) & 31;
    let mantissa = f32::from(bits & 1023) / 1024.0;
    if exponent == 0 {
        sign * mantissa * 2f32.powi(-14)
    } else {
        sign * (1.0 + mantissa) * 2f32.powi(i32::from(exponent) - 15)
    }
}

#[test]
fn gpu_ggx_sky_prefilter_and_integrated_brdf_preserve_energy_and_azimuth() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let source = format!(
        "{}\n{}",
        include_str!("../material/pbr.wgsl"),
        r#"
@group(0) @binding(0) var<storage,read_write> output:array<vec4f>;
@compute @workgroup_size(1) fn main() {
 let white=vec3f(1.0);let zenith=vec4f(white,1.0);
 output[0]=vec4f(bg_pbr_prefiltered_sky(vec3f(0.0,1.0,0.0),0.15,white,zenith),1.0);
 output[1]=vec4f(bg_pbr_prefiltered_sky(vec3f(0.0,-1.0,0.0),0.85,white,zenith),1.0);
 output[2]=vec4f(bg_pbr_environment_weight(1.0,0.15,vec3f(0.04)),1.0);
 output[3]=vec4f(bg_pbr_environment_weight(1.0,1.0,vec3f(0.04)),1.0);
 output[4]=vec4f(bg_pbr_environment_weight(0.5,0.4,vec3f(0.9,0.65,0.25)),1.0);
 output[5]=vec4f(bg_pbr_prefiltered_sky(vec3f(1.0,0.0,0.0),0.55,white,zenith),1.0);
 let horizon=vec3f(0.6,0.7,0.8);let sky=vec4f(0.2,0.4,0.75,1.0);
 output[6]=vec4f(bg_pbr_prefiltered_sky(vec3f(0.0,1.0,0.0),0.8,horizon,vec4f(sky.xyz,0.0)),1.0);
 let ray=normalize(vec3f(0.8,0.3,0.52));
 output[7]=vec4f(bg_pbr_prefiltered_sky(ray,0.55,horizon,sky),1.0);
 output[8]=vec4f(bg_pbr_prefiltered_sky(vec3f(ray.z,ray.y,-ray.x),0.55,horizon,sky),1.0);
}
"#
    );
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("GGX sky energy and isotropy regression"),
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
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 9 * 16,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 9 * 16,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
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
        pass.dispatch_workgroups(1, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, 9 * 16);
    queue.submit([encoder.finish()]);
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, |result| result.unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let bytes = readback.slice(..).get_mapped_range().unwrap();
    let rows: &[[f32; 4]] = bytemuck::cast_slice(&bytes);
    assert!(
        rows.iter()
            .flatten()
            .all(|v| v.is_finite() && *v >= 0.0 && *v <= 1.00001)
    );
    assert!(rows[0][..3].iter().all(|v| (*v - 1.0).abs() < 0.00001));
    assert!(
        rows[1][..3].iter().all(|v| v.abs() < 0.00001),
        "no invented ground radiance"
    );
    assert!(rows[2][0] > 0.03 && rows[2][0] < 0.05);
    assert!(
        rows[3][0] < rows[2][0],
        "rough dielectric DFG integrates masking loss"
    );
    assert!(
        rows[4][0] > rows[4][1] && rows[4][1] > rows[4][2],
        "metal conductor color is retained"
    );
    assert!(
        rows[5][0] > 0.1 && rows[5][0] < 0.9,
        "horizontal lobes integrate both sky hemispheres"
    );
    assert!(rows[6][..3].iter().all(|v| v.abs() < 0.00001));
    for (a, b) in rows[7][..3].iter().zip(&rows[8][..3]) {
        assert!(
            (a - b).abs() < 0.00001,
            "sky convolution must not depend on azimuth"
        );
    }
}
