//! Reuse the real additive compositor, including exact baseline cancellation.
use super::{Gpu, draw};

const BLIT: &str = r#"
@group(0) @binding(0) var input:texture_2d<f32>;
@vertex fn vs_main(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
 let p=vec2f(f32((i<<1u)&2u),f32(i&2u));return vec4f(p*2.0-1.0,0.0,1.0);
}
@fragment fn fs_blit(@builtin(position) p:vec4f)->@location(0) vec4f {return textureLoad(input,vec2i(p.xy),0);}
"#;
fn hdr(device: &wgpu::Device, size: wgpu::Extent3d) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("cached water production comparison"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: crate::render::post::HDR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        })
        .create_view(&Default::default())
}
pub(super) fn encode(
    gpu: &Gpu,
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    views: [&wgpu::TextureView; 4],
    filtered: &[wgpu::TextureView],
) -> [wgpu::TextureView; 2] {
    let current = (gpu.frame as usize - 1) % 2;
    let [depth, normal, response, indirect] = views;
    let layout = draw::layout(device, &[0], None, None);
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(BLIT.into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("cached filter half-format quantization"),
        layout: Some(&pipeline_layout),
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
            entry_point: Some("fs_blit"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: crate::render::post::HDR_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    std::array::from_fn(|mode| {
        let radiance = hdr(device, filtered[mode * 3].texture().size());
        let blit = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&filtered[mode * 3]),
            }],
        });
        draw::encode(encoder, &pipeline, &blit, &radiance);
        let binding = |binding, resource| wgpu::BindGroupEntry { binding, resource };
        let mut entries = vec![
            binding(0, wgpu::BindingResource::TextureView(&radiance)),
            binding(1, wgpu::BindingResource::TextureView(normal)),
            binding(2, gpu.uniform.as_entire_binding()),
            binding(
                3,
                wgpu::BindingResource::TextureView(&gpu.history_geometry[current]),
            ),
            binding(4, wgpu::BindingResource::TextureView(indirect)),
            binding(5, wgpu::BindingResource::TextureView(depth)),
            binding(6, wgpu::BindingResource::TextureView(response)),
            binding(
                7,
                wgpu::BindingResource::TextureView(&gpu.primary_transmission[current]),
            ),
            binding(8, wgpu::BindingResource::TextureView(&gpu.baseline)),
            binding(
                9,
                wgpu::BindingResource::TextureView(&gpu.current_correction),
            ),
        ];
        if let Some(reconstruction) = &gpu.water_reconstruction
            && reconstruction.filtering
        {
            entries.extend([
                binding(
                    10,
                    wgpu::BindingResource::TextureView(&reconstruction.moments[current]),
                ),
                binding(
                    11,
                    wgpu::BindingResource::TextureView(&reconstruction.guide[current]),
                ),
            ]);
        }
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("unchanged cached production compositor"),
            layout: &gpu.composite_layout,
            entries: &entries,
        });
        let output = hdr(device, gpu.baseline.texture().size());
        let baseline = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("immutable baseline HDR blit"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&gpu.baseline),
            }],
        });
        // Baseline intentionally has no COPY_SRC in the ordinary renderer.
        // Matching HDR textureLoad/store preserves its pixels without changing
        // any default attachment usage or allocating ordinary frame resources.
        draw::encode(encoder, &pipeline, &baseline, &output);
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cached production GI composite"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &output,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&gpu.composite);
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..3, 0..1);
        }
        output
    })
}
