use super::*;

fn validate(source: &str) {
    let module = wgpu::naga::front::wgsl::parse_str(source).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}

#[test]
fn source_near_and_lod_water_entrypoints_validate_with_actual_bindings() {
    for (lod, geometry) in [
        (false, include_str!("../../water.wgsl")),
        (true, include_str!("../../lod/shader.wgsl")),
    ] {
        let source = crate::render::water::reference_source_for(geometry, lod, true);
        let shadows = if lod {
            crate::render::sun_shadow::SHADER.replace("@group(0)", "@group(3)")
        } else {
            crate::render::sun_shadow::SHADER.to_owned()
        };
        let source = crate::render::daylight::surface_shader(&format!(
            "{}\n{shadows}\n{}\n{}\n{source}",
            include_str!("../../material/pbr.wgsl"),
            crate::render::sun_shadow::reference_shader(),
            concat!(
                include_str!("../waves.wgsl"),
                "\n",
                include_str!("../../water_surface.wgsl")
            )
        ));
        validate(&source);
    }
    validate(include_str!("composition.wgsl"));
}

#[test]
fn enhanced_water_source_remains_untouched_by_reference_hook() {
    let near = include_str!("../../water.wgsl");
    let lod = include_str!("../../lod/shader.wgsl");
    assert_eq!(
        crate::render::water::reference_source_for(near, false, false),
        near
    );
    assert_eq!(
        crate::render::water::reference_source_for(lod, true, false),
        lod
    );
}

pub(super) fn half(bits: u16) -> f32 {
    let sign = if bits & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exponent = (bits >> 10) & 31;
    let mantissa = f32::from(bits & 1023);
    match exponent {
        0 => sign * mantissa * 2.0f32.powi(-24),
        31 => sign * f32::INFINITY,
        _ => sign * (1.0 + mantissa / 1024.0) * 2.0f32.powi(i32::from(exponent) - 15),
    }
}

pub(super) fn hdr(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("source water regression HDR"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: crate::render::post::HDR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

#[test]
fn gpu_private_sqrt_water_blend_matches_source_for_multiple_layers_and_resize() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut composition = composition::Composition::new(&device);
    let shader=device.create_shader_module(wgpu::ShaderModuleDescriptor {label:Some("source water synthetic layers"),source:wgpu::ShaderSource::Wgsl(r#"
@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {let p=array<vec2f,3>(vec2f(-1.0,-1.0),vec2f(3.0,-1.0),vec2f(-1.0,3.0));return vec4f(p[i],0.0,1.0);}
@fragment fn first()->@location(0) vec4f {return vec4f(sqrt(vec3f(0.81,0.09,0.25)),0.7);}
@fragment fn second()->@location(0) vec4f {return vec4f(sqrt(vec3f(0.01,1.44,0.16)),0.35);}
"#.into())});
    let pipeline = |entry| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: crate::render::post::HDR_FORMAT,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        })
    };
    let first = pipeline("first");
    let second = pipeline("second");
    for (width, height, layers) in [(3, 2, 0), (3, 2, 1), (3, 2, 2), (7, 5, 2), (1, 1, 1)] {
        let scene = hdr(&device, width, height);
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &scene,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.04,
                            g: 0.16,
                            b: 0.64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        }
        let depth = depth(&device, width, height);
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.8),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
        }
        let target = composition.begin(&device, &mut encoder, &scene, &depth);
        for layer in 0..layers {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(if layer == 0 { &first } else { &second });
            pass.draw(0..3, 0..1);
        }
        composition.finish(&device, &mut encoder, &scene);
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 256 * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            scene.texture().as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(height),
                },
            },
            scene.texture().size(),
        );
        queue.submit([encoder.finish()]);
        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |r| r.unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let mut encoded = [0.2f32, 0.4, 0.8];
        if layers > 0 {
            for (component, water) in encoded.iter_mut().zip([0.9, 0.3, 0.5]) {
                *component = *component * 0.3 + water * 0.7;
            }
        }
        if layers > 1 {
            for (component, water) in encoded.iter_mut().zip([0.1, 1.2, 0.4]) {
                *component = *component * 0.65 + water * 0.35;
            }
        }
        {
            let bytes = slice.get_mapped_range().unwrap();
            for y in 0..height {
                for x in 0..width {
                    for (channel, expected) in encoded.iter().map(|value| value * value).enumerate()
                    {
                        let at = y as usize * 256 + x as usize * 8 + channel * 2;
                        let actual = half(u16::from_le_bytes([bytes[at], bytes[at + 1]]));
                        assert!(
                            (actual - expected).abs() < 0.003,
                            "layers={layers} size={width}x{height} channel={channel}: actual={actual}, source={expected}"
                        );
                    }
                }
            }
        }
        readback.unmap();
    }
}

pub(super) fn depth(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("source opaque depth fixture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: crate::render::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
        .create_view(&Default::default())
}
