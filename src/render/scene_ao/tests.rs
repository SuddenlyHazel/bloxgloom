use super::*;
use wgpu::util::DeviceExt;
const SIDE: u32 = 64;

#[test]
fn gpu_scene_ao_uses_geometry_and_only_removes_additional_indirect_energy() {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let adapter = instance.request_adapter(&Default::default()).await.unwrap();
        let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
        let mut ao = AmbientOcclusion::new(&device, SIDE, SIDE);
        ao.settings = Settings::default();
        let supported = super::super::post::temporal::supported(&device);
        // This exercises GL without compiling an unsupported depth-load shader.
        assert_eq!(ao.gpu.is_some(), supported);
        let flat = render(&device, &queue, &ao, false, 1.0, true);
        let (cavity, cavity_indirect) =
            render_with_particle(&device, &queue, &ao, (true, 1.0, true), None);
        let protected = render(&device, &queue, &ao, true, 1.0, false);
        let baked = render(&device, &queue, &ao, true, 0.15, true);
        let (transparent, transparent_indirect) =
            render_with_particle(&device, &queue, &ao, (true, 1.0, true), Some((0.0, 0.0)));
        assert_eq!(
            transparent_indirect, cavity_indirect,
            "zero-alpha particles must preserve the full indirect/reactivity record"
        );
        let tip = render_with_particle(&device, &queue, &ao, (true, 1.0, true), Some((1.0, 1.0)));
        assert_eq!(
            tip,
            (cavity.clone(), cavity_indirect),
            "fully faded tips must preserve HDR and temporal reactivity"
        );
        let emissive =
            render_with_particle(&device, &queue, &ao, (true, 1.0, true), Some((1.0, 0.0)));
        assert_eq!(
            transparent, cavity,
            "zero-alpha particle triangles cannot mask background AO"
        );
        ao.settings.strength = 0.0;
        let emissive_off =
            render_with_particle(&device, &queue, &ao, (true, 1.0, true), Some((1.0, 0.0)));
        assert_eq!(
            emissive, emissive_off,
            "opaque particle emission must be independent of AO"
        );
        let off = render(&device, &queue, &ao, true, 1.0, true);
        let baked_off = render(&device, &queue, &ao, true, 0.15, true);
        let protected_off = render(&device, &queue, &ao, true, 1.0, false);
        assert_eq!(
            protected, protected_off,
            "direct/emission and cave-floor energy must be bit-identical"
        );
        assert_eq!(
            baked, baked_off,
            "strong existing AO must not be multiplied again"
        );
        if !supported {
            assert_eq!(off, cavity);
            return;
        }
        assert_eq!(flat, off, "a flat surface must not self-occlude");
        let mut changed = 0;
        for (after, before) in cavity.chunks_exact(4).zip(off.chunks_exact(4)) {
            for channel in 0..3 {
                let after = half(after[channel]);
                let before = half(before[channel]);
                assert!(after <= before + 0.002, "AO cannot add energy");
                assert!(
                    after >= [2.0, 1.0, 0.5][channel] - 0.002,
                    "direct/emissive base cannot be removed"
                );
            }
            assert_eq!(after[3], before[3], "HDR alpha is unchanged");
            if half(before[0]) - half(after[0]) > 0.01 {
                changed += 1;
            }
        }
        assert!(
            changed > 80,
            "depth blocker should occlude neighboring geometry, changed={changed}"
        );
        // A blocker is finite-range; a far corner remains untouched.
        assert_eq!(&cavity[..4], &off[..4]);
        ao.resize(&device, 1, 1);
        assert_eq!(ao.indirect.texture().width(), 1);
    });
}

fn render(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    ao: &AmbientOcclusion,
    blocker: bool,
    local: f32,
    indirect: bool,
) -> Vec<u16> {
    render_with_particle(device, queue, ao, (blocker, local, indirect), None).0
}

fn render_with_particle(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    ao: &AmbientOcclusion,
    case: (bool, f32, bool),
    particle: Option<(f32, f32)>,
) -> (Vec<u16>, Vec<u16>) {
    let (blocker, local, indirect) = case;
    let scene = texture(
        device,
        SIDE,
        SIDE,
        super::super::post::HDR_FORMAT,
        "AO regression scene",
    );
    let depth = device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("AO regression depth"),
            size: wgpu::Extent3d {
                width: SIDE,
                height: SIDE,
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
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("AO physical split fixture"),
        source: wgpu::ShaderSource::Wgsl(
            r#"
struct Params { blocker:f32,local:f32,indirect:f32,pad:f32 };
@group(0) @binding(0) var<uniform> params:Params;
@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
 let p=array<vec2f,3>(vec2f(-1.0,-1.0),vec2f(3.0,-1.0),vec2f(-1.0,3.0));return vec4f(p[i],0.0,1.0);
}
struct Out { @location(0) color:vec4f,@location(1) indirect:vec4f,@builtin(frag_depth) depth:f32 };
@fragment fn fs(@builtin(position) p:vec4f)->Out {
 let energy=vec3f(0.6,0.3,0.1)*params.indirect;
 let base=vec3f(2.0,1.0,0.5);
 let block=params.blocker>0.0 && p.x>27.0 && p.x<43.0 && p.y>20.0 && p.y<44.0;
 return Out(vec4f(base+energy*params.local,1.0),vec4f(energy,params.local),select(0.75,0.4,block));
}
"#
            .into(),
        ),
    });
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&[
            if blocker { 1.0f32 } else { 0.0 },
            local,
            if indirect { 1.0 } else { 0.0 },
            0.0,
        ]),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("AO test scene"),
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
            targets: &color_targets(super::super::post::HDR_FORMAT, None),
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
            label: Some("AO matched fixture"),
            color_attachments: &attachments(&scene, &ao.indirect, wgpu::Color::BLACK),
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
    ao.resolve(device, queue, &mut encoder, &scene, &depth, Mat4::IDENTITY);
    if let Some((alpha, uv_y)) = particle {
        let camera = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&Mat4::IDENTITY.to_cols_array()),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let mut fire = super::super::fire::FireRenderer::new(device, &camera);
        let mut vertices = Vec::new();
        for position in [[-1.0f32, -1.0, 0.1], [3.0, -1.0, 0.1], [-1.0, 3.0, 0.1]] {
            vertices.extend_from_slice(&position);
            vertices.extend_from_slice(&[0.5, uv_y, 4.0, 2.0, 1.0, alpha]);
        }
        fire.set_mesh(queue, &vertices);
        let mut targets = attachments(&scene, &ao.indirect, wgpu::Color::BLACK);
        for target in targets.iter_mut().flatten() {
            target.ops.load = wgpu::LoadOp::Load;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("production particles after AO"),
            color_attachments: &targets,
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        fire.draw(&mut pass);
    }
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(SIDE * SIDE * 8 * 2),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    for (index, view) in [&scene, &ao.indirect].into_iter().enumerate() {
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: view.texture(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: index as u64 * u64::from(SIDE * SIDE * 8),
                    bytes_per_row: Some(SIDE * 8),
                    rows_per_image: Some(SIDE),
                },
            },
            wgpu::Extent3d {
                width: SIDE,
                height: SIDE,
                depth_or_array_layers: 1,
            },
        );
    }
    queue.submit(Some(encoder.finish()));
    let (tx, rx) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let bytes = readback.slice(..).get_mapped_range().unwrap();
    let pixels: &[u16] = bytemuck::cast_slice(&bytes);
    let (scene, indirect) = pixels.split_at((SIDE * SIDE * 4) as usize);
    (scene.to_vec(), indirect.to_vec())
}
fn half(bits: u16) -> f32 {
    let sign = if bits & 0x8000 == 0 { 1.0 } else { -1.0 };
    let exponent = (bits >> 10) & 31;
    let mantissa = f32::from(bits & 1023) / 1024.0;
    if exponent == 0 {
        sign * mantissa * 2.0f32.powi(-14)
    } else {
        sign * (1.0 + mantissa) * 2.0f32.powi(i32::from(exponent) - 15)
    }
}

#[test]
fn gpu_production_indirect_record_handles_overlapping_voxel_contacts_and_bounce() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let source = format!(
        "{}\n{}\n{}",
        include_str!("../daylight/test_camera.wgsl"),
        include_str!("../daylight.wgsl"),
        r#"
@group(0) @binding(0) var<storage,read_write> out:array<vec4f>;
@compute @workgroup_size(1) fn main() {
 let sky=bg_local_indirect_record(vec3f(0.5),vec3f(0.0),0.5,0.74);
 let bounced=bg_local_indirect_record(vec3f(0.5),vec3f(0.2),0.5,0.74);
 let black=bg_local_indirect_record(vec3f(0.0),vec3f(0.0),1.0,1.0);
 out[0]=sky;out[1]=bounced;out[2]=black;
 out[3]=vec4f(sky.rgb*sky.a-sky.rgb*max(0.0,sky.a-0.3),1.0);
 out[4]=vec4f(bounced.rgb*bounced.a-bounced.rgb*max(0.0,bounced.a-0.3),1.0);
}
"#
    );
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("production indirect normalization overlap"),
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
        size: 80,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 80,
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
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, 80);
    queue.submit(Some(encoder.finish()));
    let (tx, rx) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let bytes = readback.slice(..).get_mapped_range().unwrap();
    let values: &[[f32; 4]] = bytemuck::cast_slice(&bytes);
    assert!((values[0][3] - 0.37).abs() < 1e-6);
    assert!(
        (values[0][0] * values[0][3] - 0.37).abs() < 1e-6,
        "retained sky is exact"
    );
    assert!(
        (values[1][0] * values[1][3] - 0.57).abs() < 1e-6,
        "retained sky+bounce is exact"
    );
    assert_eq!(values[2], [0.0, 0.0, 0.0, 1.0]);
    assert!(
        (values[3][0] - 0.3).abs() < 1e-6,
        "overlap must union, not double multiply"
    );
    assert!(
        (values[4][0] - (0.3 + 0.2 * 0.3 / 0.37)).abs() < 1e-6,
        "common visibility conservatively protects bounce"
    );
}

#[test]
fn gpu_gl_fallback_never_compiles_depth_load_ao() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::GL,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
        eprintln!("GL adapter unavailable; optional GL validation skipped");
        return;
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    assert_eq!(device.adapter_info().backend, wgpu::Backend::Gl);
    let mut ao = AmbientOcclusion::new(&device, SIDE, SIDE);
    assert!(ao.gpu.is_none());
    let fallback = render(&device, &queue, &ao, true, 1.0, true);
    ao.settings.strength = 0.0;
    assert_eq!(fallback, render(&device, &queue, &ao, true, 1.0, true));
    ao.resize(&device, 5, 3);
    assert_eq!(ao.indirect.texture().size().height, 3);
}
