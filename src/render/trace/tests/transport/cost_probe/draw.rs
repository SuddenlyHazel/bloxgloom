//! Two float MRTs keep HDR and attribution observable without storage atomics.
use super::*;

pub(super) fn run(
    f: &Fixture,
    scene: &wgpu::BindGroup,
    source: &str,
    entry: &str,
) -> Vec<[[f32; 4]; 2]> {
    let shader = f.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("private-counter production transport"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    // Existing fixture supplies all production storage/material/dynamic layouts.
    let layout = f
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[
                Some(&f.layout),
                Some(&f.material_layout),
                Some(&f.dynamic.layout),
            ],
            immediate_size: 0,
        });
    let targets = std::array::from_fn::<_, 2, _>(|_| {
        Some(wgpu::ColorTargetState {
            format: wgpu::TextureFormat::Rgba32Float,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })
    });
    let pipeline = f
        .device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("coast secondary count probe"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                targets: &targets,
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
    let width = coast::RAYS as u32;
    let height = coast::SEEDS as u32;
    let textures: [wgpu::Texture; 2] = std::array::from_fn(|_| {
        f.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("query count + HDR"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    });
    let views = textures
        .each_ref()
        .map(|texture| texture.create_view(&Default::default()));
    let row = (width * 16).div_ceil(256) * 256;
    let buffers: [wgpu::Buffer; 2] = std::array::from_fn(|_| {
        f.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(row * height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        })
    });
    let mut encoder = f.device.create_command_encoder(&Default::default());
    {
        let attachments = views.each_ref().map(|view| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("256 real finite coast paths, no frame timing"),
            color_attachments: &attachments,
            ..Default::default()
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, scene, &[]);
        pass.set_bind_group(1, &f.materials, &[]);
        pass.set_bind_group(2, &f.dynamic.group, &[]);
        pass.draw(0..3, 0..1);
    }
    for (texture, buffer) in textures.iter().zip(&buffers) {
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(height),
                },
            },
            texture.size(),
        );
    }
    let submission = f.queue.submit([encoder.finish()]);
    for buffer in &buffers {
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, |result| result.unwrap());
    }
    f.device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .expect("small coast attribution GPU completion");
    let rows: [Vec<[f32; 4]>; 2] = buffers.each_ref().map(|buffer| {
        let data = buffer.slice(..).get_mapped_range().unwrap();
        data.chunks_exact(row as usize)
            .flat_map(|line| {
                bytemuck::cast_slice::<u8, [f32; 4]>(&line[..width as usize * 16]).to_vec()
            })
            .collect()
    });
    rows[0]
        .iter()
        .zip(&rows[1])
        .map(|(first, second)| [*first, *second])
        .collect()
}
