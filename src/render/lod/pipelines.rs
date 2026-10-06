//! Shared geometry/uniform layouts with separate opaque and fluid depth states.
use super::super::{DEPTH_FORMAT, scene_ao, water};
pub(super) struct Pipelines {
    pub opaque: wgpu::RenderPipeline,
    pub water: wgpu::RenderPipeline,
    pub camera: wgpu::Buffer,
    pub options: wgpu::Buffer,
    pub group: wgpu::BindGroup,
    pub tile_layout: wgpu::BindGroupLayout,
}
pub(super) fn new(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    coverage: &wgpu::Buffer,
    materials: &wgpu::RenderPipeline,
) -> Pipelines {
    let camera = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("LOD camera"),
        size: 224,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let options = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("LOD appearance options"),
        size: 16,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("LOD camera coverage"),
        entries: &[
            entry(0, wgpu::BufferBindingType::Uniform),
            entry(1, wgpu::BufferBindingType::Storage { read_only: true }),
            entry(2, wgpu::BufferBindingType::Uniform),
        ],
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("LOD scene"),
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: camera.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: coverage.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: options.as_entire_binding(),
            },
        ],
    });
    let tile_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("LOD tile origin"),
        entries: &[entry(0, wgpu::BufferBindingType::Uniform)],
    });
    let texture_layout = materials.get_bind_group_layout(1);
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("LOD shader"),
        source: wgpu::ShaderSource::Wgsl(water::lod_shader(include_str!("shader.wgsl")).into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("LOD pipeline"),
        bind_group_layouts: &[Some(&layout), Some(&tile_layout), Some(&texture_layout)],
        immediate_size: 0,
    });
    let water_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("LOD water shadows"),
        bind_group_layouts: &[
            Some(&layout),
            Some(&tile_layout),
            Some(&texture_layout),
            Some(&super::super::sun_shadow::camera_layout(device)),
        ],
        immediate_size: 0,
    });
    let attrs = wgpu::vertex_attr_array![0=>Float32x3,1=>Unorm8x4,2=>Uint32];
    let create = |fluid| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(if fluid {
                "distant water"
            } else {
                "distant terrain"
            }),
            layout: Some(if fluid {
                &water_layout
            } else {
                &pipeline_layout
            }),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<super::vertex::Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attrs,
                })],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: if fluid { None } else { Some(wgpu::Face::Back) },
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(!fluid),
                depth_compare: Some(if fluid {
                    wgpu::CompareFunction::LessEqual
                } else {
                    wgpu::CompareFunction::Less
                }),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(if fluid { "fs_water" } else { "fs_main" }),
                compilation_options: Default::default(),
                targets: &scene_ao::color_targets(
                    format,
                    fluid.then_some(wgpu::BlendState::ALPHA_BLENDING),
                ),
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    Pipelines {
        opaque: create(false),
        water: create(true),
        camera,
        options,
        group,
        tile_layout,
    }
}
fn entry(binding: u32, ty: wgpu::BufferBindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}
