//! Actual production SSR helper against an independent source march, including
//! encoded RGB10_A2 current-frame sampling and translated LOD coordinates.
use glam::{Mat4, Vec3, Vec4};
use wgpu::util::DeviceExt;

fn project(matrix: Mat4, p: Vec3) -> Vec3 {
    let h = matrix * p.extend(1.0);
    let n = h.truncate() / h.w;
    Vec3::new(n.x * 0.5 + 0.5, 0.5 - n.y * 0.5, n.z)
}
fn unproject(matrix: Mat4, p: Vec3) -> Vec3 {
    let h = matrix * Vec4::new(p.x * 2.0 - 1.0, 1.0 - p.y * 2.0, p.z, 1.0);
    h.truncate() / h.w
}
fn source_mask(matrix: Mat4, eye: Vec3, world: Vec3, normal: Vec3, depth: f32) -> f32 {
    let relative = world - eye;
    let start = world + normal * (relative.length() * 0.001 + 0.025);
    let incident = relative.normalize();
    let mut vector = incident - 2.0 * incident.dot(normal) * normal;
    let mut position = world + vector;
    let mut traveled = vector;
    let mut refinements = 0;
    let mut p = Vec3::new(0.0, 0.0, 1.0);
    for _ in 0..30 {
        p = project(matrix, position);
        if p.x < -0.05 || p.y < -0.05 || p.x > 1.05 || p.y > 1.05 {
            break;
        }
        let surface = unproject(matrix.inverse(), Vec3::new(p.x, p.y, depth));
        if position.distance(surface) < vector.length() * traveled.length().powf(0.1) * 1.3 {
            refinements += 1;
            if refinements >= 4 {
                break;
            }
            traveled -= vector;
            vector *= 0.1;
        }
        vector *= 2.0;
        traveled += vector;
        position = start + traveled;
    }
    if p.z >= 1.0 || depth >= 1.0 {
        return 0.0;
    }
    let border = (p.x - 0.5).abs().max((p.y - 0.5).abs()) * 1.85;
    (13.333 * (1.0 - border)).clamp(0.0, 1.0)
}

#[test]
fn gpu_source_water_ssr_current_opaque_geometry_border_and_world_translation() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let camera = include_str!("../../sky/camera.wgsl")
        .split("@group")
        .next()
        .unwrap();
    let helpers = include_str!("reflections.wgsl");
    let source = format!(
        r#"{camera}
struct WaterReferenceFrame{{sky:SkyCamera,properties:vec4f,view_projection:mat4x4f,inverse_view_projection:mat4x4f,eye:vec4f}};
@group(0) @binding(0) var<uniform> water_reference:WaterReferenceFrame;
@group(0) @binding(1) var bg_water_opaque_depth:texture_depth_2d;
@group(0) @binding(2) var bg_water_reflection_image:texture_2d<f32>;
@group(0) @binding(3) var bg_water_reflection_sampler:sampler;
{helpers}
@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4f{{let p=array<vec2f,3>(vec2f(-1.0,-1.0),vec2f(3.0,-1.0),vec2f(-1.0,3.0));return vec4f(p[i],0.0,1.0);}}
@fragment fn fs(@builtin(position) pixel:vec4f)->@location(0) vec4f{{let x=array<f32,6>(0.0,-3.5,3.5,-12.0,12.0,0.4);let world=water_reference.eye.xyz+vec3f(x[u32(pixel.x)],-1.0,-4.0);return bg_water_source_reflection(world,vec3f(0.0,1.0,0.0));}}
"#
    );
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("actual source-water SSR acceptance"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: None,
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba32Float,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        ..Default::default()
    });
    let mut composition = super::composition::Composition::new(&device);
    for eye in [Vec3::ZERO, Vec3::new(512.0, 64.0, -384.0)] {
        let matrix = glam::camera::rh::proj::directx::perspective(
            std::f32::consts::FRAC_PI_2,
            1.0,
            0.1,
            100.0,
        ) * glam::camera::rh::view::look_to_mat4(eye, -Vec3::Z, Vec3::Y);
        for sky in [false, true] {
            let opaque = super::tests::hdr(&device, 64, 64);
            let depth = super::tests::depth(&device, 64, 64);
            let depth_value = if sky {
                1.0
            } else {
                project(matrix, eye - Vec3::Z * 10.0).z
            };
            let mut encoder = device.create_command_encoder(&Default::default());
            {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &opaque,
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
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &depth,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(depth_value),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    ..Default::default()
                });
            }
            composition.begin(&device, &mut encoder, &opaque, &depth);
            let mut data = [0.0f32; 80];
            data[44..60].copy_from_slice(&matrix.to_cols_array());
            data[60..76].copy_from_slice(&matrix.inverse().to_cols_array());
            data[76..79].copy_from_slice(&eye.to_array());
            let frame = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
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
                        resource: frame.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&depth),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(
                            composition.reflection.as_ref().unwrap(),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            });
            let output = device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: 6,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = output.create_view(&Default::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
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
            let read = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 256,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
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
            queue.submit([encoder.finish()]);
            let slice = read.slice(..);
            slice.map_async(wgpu::MapMode::Read, |r| r.unwrap());
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            {
                let bytes = slice.get_mapped_range().unwrap();
                let values: &[f32] = bytemuck::cast_slice(&bytes);
                for (i, x) in [0.0, -3.5, 3.5, -12.0, 12.0, 0.4].into_iter().enumerate() {
                    let expected = source_mask(
                        matrix,
                        eye,
                        eye + Vec3::new(x, -1.0, -4.0),
                        Vec3::Y,
                        depth_value,
                    );
                    assert!(
                        (values[i * 4 + 3] - expected).abs() < 0.012,
                        "eye={eye} sky={sky} x={x}: actualmask={} source={expected}",
                        values[i * 4 + 3]
                    );
                    if expected > 0.05 {
                        for (c, color) in [0.04f32, 0.16, 0.64].into_iter().enumerate() {
                            let encoded = (color.powf(0.125) * 0.5 * 1023.0).round() / 1023.0;
                            let expected = (encoded * 2.0).powi(8);
                            assert!(
                                (values[i * 4 + c] - expected).abs() < 0.004,
                                "encoded-source channel {c}: {} !={expected}",
                                values[i * 4 + c]
                            );
                        }
                    }
                }
            }
            read.unmap();
        }
    }
}
