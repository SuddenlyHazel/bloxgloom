//! Actual near fragment consumer: replace unrelated lighting/material hooks with
//! identity fixtures, keep production fs_main/fs_cutout sampling and alpha gate.
use wgpu::util::DeviceExt;
const BYTES: [u8; 9] = [0, 1, 8, 10, 11, 32, 64, 128, 255];
fn source(default: bool, advanced: bool) -> String {
    let pipeline = include_str!("../../pipeline.wgsl");
    let varying = pipeline
        .split("struct VertexOutput")
        .nth(1)
        .unwrap()
        .split("@group(1)")
        .next()
        .unwrap();
    let fragment = pipeline
        .split("@fragment fn fs_main")
        .nth(1)
        .unwrap()
        .split("fn bg_discard_local_emitter")
        .next()
        .unwrap()
        .replace("-> BgSceneOutput {", "-> @location(0) BgSceneOutput {");
    format!(
        "const BG_BSL_REFERENCE:bool={default};\nconst BG_BSL_ADVANCED_REFERENCE:bool={advanced};\n{}\nstruct VertexOutput{varying}\n{}\n@fragment fn fs_main{fragment}",
        super::super::ALBEDO_SHADER,
        r#"
@group(0) @binding(0) var material:texture_2d_array<f32>;
@group(0) @binding(1) var material_sampler:sampler;
struct Camera{eye:vec4f};const camera=Camera(vec4f(0.0));
struct MaterialCoordinates{uv:vec2f,dx:vec2f,dy:vec2f};
struct BgSurface{albedo:vec4f};struct MaterialSurface{shaded:BgSurface};
alias BgSceneOutput=vec4f;alias BgPbr=f32;
fn bg_shadow_receiver(world:vec3f)->f32{return 1.0;}
fn bg_material_coordinates(input:VertexOutput,height:bool)->MaterialCoordinates{return MaterialCoordinates(input.uv,dpdx(input.uv),dpdy(input.uv));}
fn bg_material_layer(layer:i32)->i32{return layer;}
fn bg_material_specular(input:VertexOutput,coordinates:MaterialCoordinates,albedo:vec3f)->BgPbr{return 0.0;}
fn surface(input:VertexOutput,albedo:vec4f,coordinates:MaterialCoordinates,specular:BgPbr)->MaterialSurface{return MaterialSurface(BgSurface(albedo));}
fn shade(input:VertexOutput,surface:MaterialSurface,specular:BgPbr,receiver:f32)->BgSceneOutput{return surface.shaded.albedo;}
@fragment fn fs_baseline(input:VertexOutput)->@location(0)vec4f {
    return textureSampleGrad(material,material_sampler,input.uv,0,dpdx(input.uv),dpdy(input.uv));
}
@fragment fn fs_math(input:VertexOutput)->@location(0)vec4f {
    let ideal=array<f32,9>(0.0,0.00030352698,0.0024282159,0.0030352698,0.0033465358,0.014443844,0.051269457,0.2158605,1.0);
    return vec4f(bg_bsl_texture_albedo(vec3f(ideal[u32(input.position.x)])),1.0);
}
@vertex fn vs_fixture(@builtin(vertex_index) id:u32)->VertexOutput{
    let uv=vec2f(f32((id<<1u)&2u),f32(id&2u));
    var out:VertexOutput;out.position=vec4f(uv*2.0-1.0,0.0,1.0);out.uv=vec2f(uv.x,1.0-uv.y);
    out.layer=0;return out;
}
"#
    )
}
#[test]
fn reference_albedo_actual_near_consumer_validates_all_three_material_modes() {
    for (default, advanced) in [(true, false), (false, true), (false, false)] {
        let module = wgpu::naga::front::wgsl::parse_str(&source(default, advanced)).unwrap();
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
    }
}
fn srgb(byte: u8) -> f64 {
    let c = f64::from(byte) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
#[test]
fn independent_source_albedo_pixel_goldens_include_srgb_toe() {
    // Source terrain252/entities254/water407 use2.2, not piecewise sRGB.
    let expected = [
        0.0,
        0.000005077051900661759,
        0.0004925037871914326,
        0.0008046584995130583,
        0.0009923743040743253,
        0.010397802292555288,
        0.04777575355617064,
        0.2195197180748679,
        1.0,
    ];
    for (byte, expected) in BYTES.into_iter().zip(expected) {
        assert!(((f64::from(byte) / 255.0).powf(2.2) - expected).abs() < 1e-14);
    }
    assert!(srgb(8) > 3.0 * (8.0_f64 / 255.0).powf(2.2));
}
#[test]
fn gpu_actual_near_albedo_consumer_matches_source_default_advanced_and_enhanced() {
    pollster::block_on(async {
        let adapter = wgpu::Instance::default()
            .request_adapter(&Default::default())
            .await
            .unwrap();
        let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
        let input = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("unaltered sRGB source pixels"),
            size: wgpu::Extent3d {
                width: 9,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let data: Vec<_> = BYTES
            .into_iter()
            .enumerate()
            .flat_map(|(i, c)| [c, c, c, if i == 1 { 64 } else { 192 }])
            .collect();
        queue.write_texture(
            input.as_image_copy(),
            &data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(36),
                rows_per_image: Some(1),
            },
            input.size(),
        );
        let view = input.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let output = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: input.size(),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let target = output.create_view(&Default::default());
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let mut hardware = [[0.0_f32; 4]; 9];
        for (default, advanced) in [(false, false), (true, false), (false, true)] {
            let entries: &[&str] = if !default && !advanced {
                &["fs_baseline", "fs_main", "fs_cutout"]
            } else {
                &["fs_math", "fs_main", "fs_cutout"]
            };
            for &entry in entries {
                let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("production near albedo consumer"),
                    source: wgpu::ShaderSource::Wgsl(source(default, advanced).into()),
                });
                let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: None,
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_fixture"),
                        compilation_options: Default::default(),
                        buffers: &[],
                    },
                    primitive: Default::default(),
                    depth_stencil: None,
                    multisample: Default::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some(entry),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: wgpu::TextureFormat::Rgba32Float,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    multiview_mask: None,
                    cache: None,
                });
                let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &pipeline.get_bind_group_layout(0),
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                    ],
                });
                let read = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: &[0; 256],
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                });
                let mut encoder = device.create_command_encoder(&Default::default());
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &target,
                            resolve_target: None,
                            depth_slice: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        ..Default::default()
                    });
                    pass.set_pipeline(&pipeline);
                    pass.set_bind_group(0, &group, &[]);
                    pass.draw(0..3, 0..1);
                }
                encoder.copy_texture_to_buffer(
                    output.as_image_copy(),
                    wgpu::TexelCopyBufferInfo {
                        buffer: &read,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(256),
                            rows_per_image: Some(1),
                        },
                    },
                    output.size(),
                );
                queue.submit(Some(encoder.finish()));
                let (tx, rx) = std::sync::mpsc::channel();
                read.slice(..)
                    .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
                device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                rx.recv().unwrap().unwrap();
                let bytes = read.slice(..).get_mapped_range().unwrap();
                let rows: &[[f32; 4]] = bytemuck::cast_slice(&bytes[..144]);
                for (i, (&byte, row)) in BYTES.iter().zip(rows).enumerate() {
                    if entry == "fs_baseline" {
                        hardware[i] = *row;
                        assert!(
                            row[..3]
                                .iter()
                                .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
                        );
                        if byte == 0 || byte == 255 {
                            assert_eq!(row[0], f32::from(byte) / 255.0);
                        }
                        continue;
                    }
                    if entry == "fs_cutout" && i == 1 {
                        assert_eq!(
                            *row, [0.0; 4],
                            "reference conversion cannot alter cutout alpha rejection"
                        );
                        continue;
                    }
                    // sRGB texture decoding is implementation-approximated on Metal,
                    // especially below byte11. Preserve that independent observed
                    // baseline; separately validate the helper's ideal input in fs_math.
                    let expected = if entry == "fs_math" {
                        (f64::from(byte) / 255.0).powf(2.2)
                    } else if default || advanced {
                        let linear = f64::from(hardware[i][0]);
                        let encoded = if linear <= 0.0031308 {
                            linear * 12.92
                        } else {
                            1.055 * linear.powf(1.0 / 2.4) - 0.055
                        };
                        encoded.powf(2.2)
                    } else {
                        f64::from(hardware[i][0])
                    };
                    for actual in &row[..3] {
                        assert!(
                            (f64::from(*actual) - expected).abs() < 1e-7 + expected * 0.001,
                            "{entry} default{default}/advanced{advanced} byte{byte}: {row:?} vs{expected}"
                        );
                    }
                    let alpha = if entry == "fs_math" {
                        1.0
                    } else {
                        f32::from(data[i * 4 + 3]) / 255.0
                    };
                    assert!(
                        (row[3] - alpha).abs() < 1e-6,
                        "albedo conversion must leave alpha exact"
                    );
                }
            }
        }
    });
}
