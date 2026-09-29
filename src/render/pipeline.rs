use super::custom;
use super::material;
use super::shader::with_world_sun;
use super::{DEPTH_FORMAT, VERTEX_FLOATS};
use crate::content::Catalog;
use wgpu::util::DeviceExt;

pub(crate) type VoxelPipelines = (
    wgpu::RenderPipeline,
    wgpu::RenderPipeline,
    wgpu::Buffer,
    wgpu::BindGroup,
    wgpu::BindGroup,
);

const VERTEX_STRIDE: u64 = VERTEX_FLOATS as u64 * 4;

pub(crate) fn create_voxel_pipeline(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
) -> (
    wgpu::RenderPipeline,
    wgpu::RenderPipeline,
    wgpu::Buffer,
    wgpu::BindGroup,
    wgpu::BindGroup,
) {
    create_voxel_pipeline_with_catalog(device, queue, format, crate::content::catalog())
}

pub(crate) fn create_voxel_pipeline_with_catalog(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
    catalog: &Catalog,
) -> (
    wgpu::RenderPipeline,
    wgpu::RenderPipeline,
    wgpu::Buffer,
    wgpu::BindGroup,
    wgpu::BindGroup,
) {
    let source = format!(
        "{}\nfn bg_vertex(input: BgVertex, layer: u32) -> BgVertex {{ return input; }}\nfn bg_surface(input: BgSurface, layer: u32) -> BgSurface {{ return input; }}\n{SHADER}",
        custom::TYPES
    );
    create_voxel_pipeline_source(
        device,
        queue,
        format,
        catalog,
        &with_world_sun(&source),
        None,
    )
}

/// Prepared on a worker; the renderer still owns vertex geometry, projection,
/// tile sampling, light, fog, alpha testing and depth state.
pub(crate) fn create_custom_voxel_pipeline(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
    catalog: &Catalog,
    prepared: &custom::Prepared,
) -> Result<(VoxelPipelines, custom::Gpu), String> {
    let source = format!("{}\n{}\n{SHADER}", custom::TYPES, custom::compose(prepared));
    let owners = prepared
        .materials
        .iter()
        .map(|m| m.owner.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
                let gpu = custom::Gpu::new(device, prepared);
                let pipelines = create_voxel_pipeline_source(
                    device,
                    queue,
                    format,
                    catalog,
                    &with_world_sun(&source),
                    Some(&gpu.layout),
                );
                if let Some(error) = pollster::block_on(error_scope.pop()) {
                    Err(format!("{owners}: material GPU preparation: {error}"))
                } else {
                    Ok((pipelines, gpu))
                }
            })
            .join()
            .map_err(|_| format!("{owners}: material GPU preparation worker panicked"))?
    })
}

fn create_voxel_pipeline_source(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
    catalog: &Catalog,
    source: &str,
    visual_layout: Option<&wgpu::BindGroupLayout>,
) -> (
    wgpu::RenderPipeline,
    wgpu::RenderPipeline,
    wgpu::Buffer,
    wgpu::BindGroup,
    wgpu::BindGroup,
) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("opaque voxel shader"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("camera matrix"),
        size: 64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("camera layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let camera_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("camera bind group"),
        layout: &camera_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: camera_buffer.as_entire_binding(),
        }],
    });
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("voxel material tiles"),
        size: wgpu::Extent3d {
            width: material::TEXTURE_SIZE,
            height: material::TEXTURE_SIZE,
            depth_or_array_layers: material::texture_layers_for(catalog),
        },
        mip_level_count: material::TEXTURE_MIPS,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    // Decode and build mips off the window thread. The verified catalog owns
    // bounded PNG bytes; no package image work belongs in a frame or event.
    let mips = std::thread::scope(|scope| {
        scope
            .spawn(|| material::material_mips_for(catalog))
            .join()
            .expect("verified material tiles decode")
    });
    for (level, pixels) in mips.iter().enumerate() {
        let size = material::TEXTURE_SIZE >> level;
        let layer_bytes = (size * size * 4) as usize;
        for layer in 0..material::texture_layers_for(catalog) {
            let start = layer as usize * layer_bytes;
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &pixels[start..start + layer_bytes],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(size * 4),
                    rows_per_image: Some(size),
                },
                wgpu::Extent3d {
                    width: size,
                    height: size,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
    let texture_view = texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("nearest repeating voxel tiles"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        ..Default::default()
    });
    let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("voxel material layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            },
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
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let emission = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("frozen voxel material emission"),
        contents: bytemuck::cast_slice(&material::emission_strengths(catalog)),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let texture_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("voxel material bind group"),
        layout: &texture_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: emission.as_entire_binding(),
            },
        ],
    });
    let mut layouts = vec![Some(&camera_layout), Some(&texture_layout)];
    if let Some(visual) = visual_layout {
        layouts.push(Some(visual));
    }
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("voxel pipeline layout"),
        bind_group_layouts: &layouts,
        immediate_size: 0,
    });
    let attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32, 4 => Float32x2, 5 => Float32];
    let make_pipeline = |label, cull_mode, fragment_entry| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: VERTEX_STRIDE,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attributes,
                })],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(fragment_entry),
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
    };
    let pipeline = make_pipeline("opaque voxel pipeline", Some(wgpu::Face::Back), "fs_main");
    let cutout_pipeline = make_pipeline("cutout foliage pipeline", None, "fs_cutout");
    (
        pipeline,
        cutout_pipeline,
        camera_buffer,
        camera_group,
        texture_group,
    )
}

const SHADER: &str = include_str!("pipeline.wgsl");
