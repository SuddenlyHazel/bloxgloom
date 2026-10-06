//! Bindings and three focused source-shaft pipelines.
use wgpu::util::DeviceExt;
pub(super) struct Gpu {
    pub layout: wgpu::BindGroupLayout,
    pub uniform: wgpu::Buffer,
    pub noise: crate::render::sky::ReferenceNoise,
    pub comparison: wgpu::Sampler,
    pub sampler: wgpu::Sampler,
    pub integrate: wgpu::RenderPipeline,
    pub reconstruct: wgpu::RenderPipeline,
    pub copy: wgpu::RenderPipeline,
}
impl Gpu {
    pub fn new(device: &wgpu::Device) -> Self {
        let texture = |binding, sample_type| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let sampler = |binding, kind| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(kind),
            count: None,
        };
        let float = wgpu::TextureSampleType::Float { filterable: true };
        let depth = wgpu::TextureSampleType::Depth;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("source light shafts"),
            entries: &[
                texture(0, float),
                texture(1, depth),
                texture(2, depth),
                texture(3, float),
                texture(4, float),
                sampler(5, wgpu::SamplerBindingType::Filtering),
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                texture(7, depth),
                sampler(8, wgpu::SamplerBindingType::Comparison),
                texture(9, float),
                sampler(10, wgpu::SamplerBindingType::Filtering),
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("source7-logarithmic shafts and4tap encoded reconstruction"),
            source: wgpu::ShaderSource::Wgsl(super::shader().into()),
        });
        Self {
            uniform: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("source shaft frame"),
                contents: bytemuck::cast_slice(&[0.0_f32; 56]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }),
            noise: crate::render::sky::ReferenceNoise::new(device),
            comparison: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("source shaft bilinear shadow comparison"),
                compare: Some(wgpu::CompareFunction::LessEqual),
                min_filter: wgpu::FilterMode::Linear,
                mag_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("source shaft encoded bilinear reconstruction"),
                min_filter: wgpu::FilterMode::Linear,
                mag_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            integrate: pipeline(
                device,
                &shader,
                &layout,
                "integrate",
                wgpu::TextureFormat::Rgba8Unorm,
            ),
            reconstruct: pipeline(
                device,
                &shader,
                &layout,
                "reconstruct",
                crate::render::post::HDR_FORMAT,
            ),
            copy: pipeline(
                device,
                &shader,
                &layout,
                "copy",
                crate::render::post::HDR_FORMAT,
            ),
            layout,
        }
    }
}
fn pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::BindGroupLayout,
    entry: &str,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(entry),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(entry),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(entry),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}
pub(super) fn target(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("source light shaft target"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
        .create_view(&Default::default())
}
