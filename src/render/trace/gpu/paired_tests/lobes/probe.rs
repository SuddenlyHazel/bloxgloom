//! Bounded test-only readback and original four-MRT primary probes.
use super::*;

fn output(f: &Fixture, size: wgpu::Extent3d, format: wgpu::TextureFormat) -> wgpu::TextureView {
    f.device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("bounded water oracle target"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
        .create_view(&Default::default())
}
fn copy_float32(f: &Fixture, view: &wgpu::TextureView) -> Vec<[f32; 4]> {
    let size = view.texture().size();
    let stride = (size.width * 16).div_ceil(256) * 256;
    let buffer = f.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("bounded water oracle readback"),
        size: u64::from(stride * size.height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = f.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        view.texture().as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(size.height),
            },
        },
        size,
    );
    let submitted = f.queue.submit([encoder.finish()]);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
    f.device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submitted),
            timeout: Some(deadline.saturating_duration_since(std::time::Instant::now())),
        })
        .expect("water oracle GPU readback within 180s");
    rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
        .expect("water oracle map callback within 180s")
        .expect("water oracle buffer mapped");
    let mapped = buffer.slice(..).get_mapped_range().unwrap();
    let result = mapped
        .chunks_exact(stride as usize)
        .flat_map(|line| {
            bytemuck::cast_slice::<u8, [f32; 4]>(&line[..size.width as usize * 16]).to_vec()
        })
        .collect();
    drop(mapped);
    buffer.unmap();
    result
}

pub(super) fn read(f: &Fixture, input: &wgpu::TextureView) -> Vec<[f32; 4]> {
    let source = r#"
@group(0) @binding(0) var input:texture_2d<f32>;
@vertex fn vs_main(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
 let p=vec2f(f32((i<<1u)&2u),f32(i&2u));return vec4f(p*2.0-1.0,0.0,1.0);
}
@fragment fn fs_main(@builtin(position) p:vec4f)->@location(0) vec4f {return textureLoad(input,vec2i(p.xy),0);}
"#;
    let layout = f
        .device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
    let group = f.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(input),
        }],
    });
    let mut size = input.texture().size();
    size.depth_or_array_layers = 1;
    draw(
        f,
        source,
        "fs_main",
        size,
        &[&group],
        &[Some(&layout)],
        &[wgpu::TextureFormat::Rgba32Float],
    )
}

pub(super) fn primary(
    f: &Fixture,
    gpu: &Gpu,
    views: [&wgpu::TextureView; 4],
    source: &str,
    size: wgpu::Extent3d,
) -> Vec<[f32; 4]> {
    let [depth, normal, response, indirect] = views;
    let bind = |binding, resource| wgpu::BindGroupEntry { binding, resource };
    let mut entries = vec![
        bind(0, gpu.uniform.as_entire_binding()),
        bind(1, gpu.geometry.nodes.as_entire_binding()),
        bind(2, gpu.geometry.triangles.as_entire_binding()),
        bind(3, wgpu::BindingResource::TextureView(depth)),
        bind(4, wgpu::BindingResource::TextureView(normal)),
        bind(5, wgpu::BindingResource::TextureView(response)),
        bind(6, wgpu::BindingResource::TextureView(indirect)),
        bind(7, wgpu::BindingResource::TextureView(&gpu.history[0])),
        bind(
            8,
            wgpu::BindingResource::TextureView(&gpu.history_geometry[0]),
        ),
        bind(9, wgpu::BindingResource::TextureView(&gpu.baseline)),
        bind(10, gpu.geometry.coverage.as_entire_binding()),
        bind(
            15,
            wgpu::BindingResource::TextureView(&gpu.primary_transmission[0]),
        ),
    ];
    entries.extend(gpu.lod.entries());
    if let Some(reconstruction) = &gpu.water_reconstruction {
        entries.push(bind(
            16,
            wgpu::BindingResource::TextureView(&reconstruction.raw),
        ));
    }
    let group = f.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &gpu.layout,
        entries: &entries,
    });
    // Two mat4s + nine vec4s precede counts; disable only counts.w at runtime.
    // Keep the original temporal branch and all four fragment outputs alive.
    const HISTORY_OFFSET: u64 = 2 * 64 + 9 * 16 + 3 * 4;
    assert_eq!(HISTORY_OFFSET, 284);
    f.queue
        .write_buffer(&gpu.uniform, HISTORY_OFFSET, bytemuck::bytes_of(&0u32));
    draw(
        f,
        source,
        "fs_transport",
        size,
        &[&group, &f.materials, &gpu.dynamic.group],
        &[
            Some(&gpu.layout),
            Some(&f.material_layout),
            Some(&gpu.dynamic.layout),
        ],
        &[
            wgpu::TextureFormat::Rgba32Float,
            wgpu::TextureFormat::Rgba16Float,
            wgpu::TextureFormat::R16Float,
            wgpu::TextureFormat::R16Float,
        ],
    )
}

#[allow(clippy::too_many_arguments)]
fn draw(
    f: &Fixture,
    source: &str,
    entry: &str,
    size: wgpu::Extent3d,
    groups: &[&wgpu::BindGroup],
    layouts: &[Option<&wgpu::BindGroupLayout>],
    formats: &[wgpu::TextureFormat],
) -> Vec<[f32; 4]> {
    eprintln!(
        "water oracle {entry}: create pipeline ({} MRTs)",
        formats.len()
    );
    let module = f.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("water oracle original entrypoint"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let layout = f
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: layouts,
            immediate_size: 0,
        });
    let targets = formats
        .iter()
        .map(|format| {
            Some(wgpu::ColorTargetState {
                format: *format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })
        })
        .collect::<Vec<_>>();
    let pipeline = f
        .device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("water oracle original entrypoint"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                targets: &targets,
            }),
            multiview_mask: None,
            cache: None,
        });
    let outputs = formats
        .iter()
        .map(|format| output(f, size, *format))
        .collect::<Vec<_>>();
    let attachments = outputs
        .iter()
        .map(|view| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })
        })
        .collect::<Vec<_>>();
    let mut encoder = f.device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &attachments,
            ..Default::default()
        });
        pass.set_pipeline(&pipeline);
        for (i, group) in groups.iter().enumerate() {
            pass.set_bind_group(i as u32, *group, &[]);
        }
        pass.draw(0..3, 0..1);
    }
    eprintln!("water oracle {entry}: submit and bounded readback");
    f.queue.submit([encoder.finish()]);
    copy_float32(f, &outputs[0])
}
