use super::super::{Pass, shader};
use crate::render::parameters;
use wgpu::util::DeviceExt;

pub(super) struct GpuPass {
    pub(super) pipeline: wgpu::RenderPipeline,
    pub(super) layout: wgpu::BindGroupLayout,
    pub(super) sampler: wgpu::Sampler,
    pub(super) time: wgpu::Buffer,
    pub(super) data: Data,
    pub prepared: Pass,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct Data {
    pub(super) frame: [f32; 4],
    pub(super) parameters: [[f32; 4]; parameters::MAX_PARAMETERS],
}

impl GpuPass {
    pub(super) fn compile(device: &wgpu::Device, prepared: &Pass) -> Result<Self, String> {
        let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut entries = vec![
            texture_entry(0),
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(if prepared.descriptor.version == 1 {
                        16
                    } else {
                        std::mem::size_of::<Data>() as u64
                    }),
                },
                count: None,
            },
        ];
        if prepared.descriptor.version == 2 {
            entries.push(texture_entry(3));
        }
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(&prepared.owner),
            entries: &entries,
        });
        let source = if prepared.descriptor.version == 1 {
            prepared.source.clone()
        } else {
            shader::compose(&prepared.source)
        };
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(&prepared.owner),
            source: wgpu::ShaderSource::Wgsl(source.as_str().into()),
        });
        let vertex = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("renderer-owned effect triangle"), source: wgpu::ShaderSource::Wgsl(
                "@vertex fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f { let p = array<vec2f,3>(vec2f(-1.,-1.),vec2f(3.,-1.),vec2f(-1.,3.)); return vec4f(p[i],0.,1.); }".into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(&prepared.owner),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&prepared.owner),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vertex,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
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
        if let Some(error) = pollster::block_on(error_scope.pop()) {
            return Err(format!("{}: shader pipeline: {error}", prepared.owner));
        }
        Ok(Self {
            pipeline,
            layout,
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            time: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("effect time/size"),
                contents: bytemuck::bytes_of(&Data {
                    frame: [0.0; 4],
                    parameters: parameters::defaults(prepared.parameters())
                        .expect("verified effect parameters"),
                }),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }),
            data: Data {
                frame: [0.0; 4],
                parameters: parameters::defaults(prepared.parameters())
                    .expect("verified effect parameters"),
            },
            prepared: prepared.clone(),
        })
    }
}

fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}
