use super::super::super::*;
use wgpu::util::DeviceExt;
const WIDTH: u32 = 24;
const HEIGHT: u32 = 16;
struct Fixture {
    device: wgpu::Device,
    queue: wgpu::Queue,
    taa: Temporal,
    scene: wgpu::Texture,
    scene_view: wgpu::TextureView,
    depth: wgpu::TextureView,
    reactive: wgpu::TextureView,
    camera: wgpu::Buffer,
    group: wgpu::BindGroup,
    history_pipeline: wgpu::RenderPipeline,
    current_pipeline: wgpu::RenderPipeline,
}
impl Fixture {
    fn new() -> Option<Self> {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).ok()?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        if !supported(&device) {
            return None;
        }
        let taa = Temporal::new(&device, WIDTH, HEIGHT);
        let texture = |format, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("reference sky history fixture"),
                size: wgpu::Extent3d {
                    width: WIDTH,
                    height: HEIGHT,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let scene = texture(
            super::super::super::super::HDR_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
        );
        let scene_view = scene.create_view(&Default::default());
        let depth = texture(
            wgpu::TextureFormat::Depth32Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        )
        .create_view(&Default::default());
        let reactive = texture(
            super::super::super::super::HDR_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        )
        .create_view(&Default::default());
        let source = format!(
            "{}\n{}\n{}\n{}",
            crate::render::sky::STYLE_SHADER,
            include_str!("../../../sky/camera.wgsl"),
            include_str!("../../../sky/reference_clouds.wgsl"),
            r#"
fn test_color(input:SkyVertex)->vec3f {
 let ray=bg_sky_camera_ray(input.uv);let gray=0.45+ray.y*0.12+ray.z*0.08;
 let noise=(bg_reference_cloud_dither(vec2f(input.position.x,16.0-input.position.y),sky_camera.reference.z)-0.5)*0.20;
 return vec3f(gray+noise,gray*0.8+noise*0.35,gray*1.2-noise*0.2);
}
struct Previous {@location(0) color:vec4f,@location(1) depth:f32};
@fragment fn previous_frame(input:SkyVertex)->Previous {
 let flags=u32(sky_camera.reference.w);let foreground=(flags&32u)!=0u||((flags&1u)!=0u&&(input.position.x>=11.0||input.position.x<10.0));
 return Previous(vec4f(test_color(input),1.0),select(-1.0,0.1,foreground));
}
struct Current {@location(0) color:vec4f,@location(1) motion:vec4f,@location(2) reactive:vec4f,@builtin(frag_depth) depth:f32};
@fragment fn current_frame(input:SkyVertex)->Current {
 let flags=u32(sky_camera.reference.w);let color=select(test_color(input),vec3f(0.0),(flags&64u)!=0u);
 return Current(vec4f(color,1.0),vec4f(0.0,0.0,0.0,select(0.0,-1.0,(flags&8u)!=0u)),vec4f(0.0,0.0,0.0,select(0.0,-1.0,(flags&16u)!=0u)),select(1.0,0.5,(flags&2u)!=0u));
}
"#
        );
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("reference sky dither camera draw"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let binding = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sky fixture camera layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&binding)],
            immediate_size: 0,
        });
        let target = |format| {
            Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })
        };
        let make = |entry, targets: &[Option<wgpu::ColorTargetState>], depth| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("reference sky history source"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: depth,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets,
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let history_pipeline = make(
            "previous_frame",
            &[
                target(super::super::super::super::HDR_FORMAT),
                target(wgpu::TextureFormat::R32Float),
            ],
            None,
        );
        let current_pipeline = make(
            "current_frame",
            &[
                target(super::super::super::super::HDR_FORMAT),
                target(crate::render::avatars::motion::FORMAT),
                target(super::super::super::super::HDR_FORMAT),
            ],
            Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
        );
        let camera = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: &[0; 160],
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &binding,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera.as_entire_binding(),
            }],
        });
        Some(Self {
            device,
            queue,
            taa,
            scene,
            scene_view,
            depth,
            reactive,
            camera,
            group,
            history_pipeline,
            current_pipeline,
        })
    }
    fn write_camera(&self, camera: crate::render::Camera, frame: u32, flags: u32) {
        let mut data = crate::render::sky_camera_data_at_sample(
            camera,
            WIDTH,
            HEIGHT,
            crate::render::daylight::Atmosphere::at(crate::daylight::INITIAL_MS),
            frame,
        );
        data[39] = flags as f32;
        self.queue
            .write_buffer(&self.camera, 0, bytemuck::cast_slice(&data));
    }
    fn seed(&mut self, camera: crate::render::Camera, flags: u32) {
        self.write_camera(camera, 16, flags);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[
                    attachment(&self.taa.colors[self.taa.index]),
                    attachment(&self.taa.depths[self.taa.index]),
                ],
                ..Default::default()
            });
            pass.set_pipeline(&self.history_pipeline);
            pass.set_bind_group(0, &self.group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([encoder.finish()]);
        self.taa.previous = Some((
            crate::render::view_projection(camera, WIDTH, HEIGHT),
            camera,
        ));
        self.taa.valid = true;
    }
    fn frame(
        &mut self,
        camera: crate::render::Camera,
        sample: u32,
        flags: u32,
        reference: bool,
    ) -> [f32; 3] {
        self.taa.prepare(&self.queue, camera, WIDTH, HEIGHT);
        self.queue.write_buffer(
            &self.taa.settings,
            140,
            bytemuck::bytes_of(&f32::from(reference)),
        );
        self.write_camera(camera, sample, flags);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[
                    attachment(&self.scene_view),
                    attachment(&self.taa.motion),
                    attachment(&self.reactive),
                ],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&self.current_pipeline);
            pass.set_bind_group(0, &self.group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.taa.resolve(
            &self.device,
            &mut encoder,
            &self.scene_view,
            &self.depth,
            Some(&self.reactive),
        );
        let read = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 256,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.scene,
                mip_level: 0,
                origin: wgpu::Origin3d { x: 10, y: 8, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &read,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(1),
                },
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        self.taa.submitted();
        read.slice(..)
            .map_async(wgpu::MapMode::Read, |r| r.unwrap());
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        let mapped = read.slice(..).get_mapped_range().unwrap();
        std::array::from_fn(|i| {
            let half = u16::from_le_bytes([mapped[i * 2], mapped[i * 2 + 1]]);
            if half == 0 {
                0.
            } else {
                f32::from_bits(
                    ((u32::from(half) >> 10) + 112) << 23 | ((u32::from(half) & 1023) << 13),
                )
            }
        })
    }
}
fn attachment(view: &wgpu::TextureView) -> Option<wgpu::RenderPassColorAttachment<'_>> {
    Some(wgpu::RenderPassColorAttachment {
        view,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
            store: wgpu::StoreOp::Store,
        },
    })
}
#[test]
fn gpu_reference_sky_reprojects_camera_dither_and_rejects_foreground() {
    let Some(mut fixture) = Fixture::new() else {
        eprintln!("reference sky temporal GPU skipped");
        return;
    };
    let old = crate::render::Camera {
        position: Vec3::ZERO,
        yaw: 0.,
        pitch: 0.2,
        fov_y_radians: 1.,
    };
    let pixel = Vec2::new(10.5, 8.5);
    for (yaw, translation, flags, reference, blend) in [
        (0., Vec3::ZERO, 0, true, true),
        (0.025, Vec3::ZERO, 0, true, true),
        (-0.03, Vec3::new(0.6, 0.4, -0.8), 0, true, true),
        (0., Vec3::ZERO, 1, true, false),
        (0., Vec3::ZERO, 32, true, false),
        (0., Vec3::ZERO, 2, true, false),
        (0., Vec3::ZERO, 8, true, false),
        (0., Vec3::ZERO, 16, true, false),
        (0., Vec3::ZERO, 0, false, false),
        (0., Vec3::new(10., 0., 0.), 0, true, false),
    ] {
        fixture.seed(old, flags);
        let current = crate::render::Camera {
            position: translation,
            yaw,
            ..old
        };
        let result = fixture.frame(current, 17, flags, reference);
        let expected = if blend {
            super::oracle::expected(current, old, 17, pixel)
        } else {
            super::oracle::color(current, pixel, 17)
        };
        for (actual, expected) in result.into_iter().zip(expected) {
            assert!(
                (f64::from(actual) - expected).abs() < 0.0015,
                "camera sky history yaw{yaw} flags{flags} reference{reference}: {actual} vs {expected}"
            );
        }
    }
    // Actual prior foreground/UI/reactive samples write a positive depth tag;
    // the next uncovered sky must reject them, not only the current reactive frame.
    for flags in [2, 8, 16] {
        fixture.seed(old, 0);
        fixture.frame(old, 17, flags, true);
        let result = fixture.frame(old, 18, 0, true);
        let expected = super::oracle::color(old, pixel, 18);
        for (a, e) in result.into_iter().zip(expected) {
            assert!(
                (f64::from(a) - e).abs() < 0.001,
                "uncovered sky inherited prior class {flags}: {a} vs {e}"
            );
        }
    }
    // Newly uniform black sky cannot inherit old bright cloud/celestial colors.
    fixture.seed(old, 0);
    assert_eq!(fixture.frame(old, 17, 64, true), [0.; 3]);
    // Every sample uses actual submitted jitter, while source cloud dither advances.
    fixture.taa.previous = None;
    fixture.taa.valid = false;
    fixture.taa.frame = 0;
    let mut filtered = Vec::new();
    let mut raw = Vec::new();
    for sample in 0..24 {
        let result = fixture.frame(old, sample, 0, true);
        if sample >= 4 {
            filtered.push(f64::from(result[0]));
            raw.push(super::oracle::color(old, pixel, sample)[0]);
        }
    }
    let variance = |values: &[f64]| {
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64
    };
    assert!(
        variance(&filtered) < variance(&raw) * 0.4,
        "source cloud dither not accumulated: filtered {} raw {}",
        variance(&filtered),
        variance(&raw)
    );
}
