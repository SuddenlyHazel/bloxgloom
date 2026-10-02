use super::custom;
use super::material;
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
        .expect("builtin material resources fit the requested device limits")
}

pub(crate) fn create_voxel_pipeline_with_catalog(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
    catalog: &Catalog,
) -> Result<VoxelPipelines, String> {
    let usage = material::resources::validate(
        catalog.textures().len(),
        device.limits().max_texture_array_layers,
    )?;
    tracing::debug!(
        layers = usage.layers,
        mip_bytes = usage.mip_bytes,
        maximum_bytes = material::resources::MAX_ARRAY_BYTES,
        "material texture array admitted"
    );
    let source = format!(
        "{}\n{RELIEF_SHADER}\n{PARALLAX_SHADER}\n{DETAIL_SHADER}\nfn bg_vertex(input: BgVertex, layer: u32) -> BgVertex {{ return input; }}\nfn bg_surface(input: BgSurface, layer: u32) -> BgSurface {{ return input; }}\n{SHADER}",
        custom::TYPES
    );
    Ok(create_voxel_pipeline_source(
        device, queue, format, catalog, &source, None,
    ))
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
    let usage = material::resources::validate(
        catalog.textures().len(),
        device.limits().max_texture_array_layers,
    )?;
    tracing::debug!(
        layers = usage.layers,
        mip_bytes = usage.mip_bytes,
        maximum_bytes = material::resources::MAX_ARRAY_BYTES,
        "material texture array admitted"
    );
    let source = format!(
        "{}\n{}\n{RELIEF_SHADER}\n{PARALLAX_SHADER}\n{DETAIL_SHADER}\n{SHADER}",
        custom::TYPES,
        custom::compose(prepared)
    );
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
                    &source,
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
        source: wgpu::ShaderSource::Wgsl(super::daylight::shader(source).into()),
    });
    let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("camera matrix"),
        size: 128,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let camera_layout = super::sun_shadow::camera_layout(device);
    let camera_group = super::sun_shadow::fallback_camera_group(device, &camera_buffer);
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
    let (mips, companions) = std::thread::scope(|scope| {
        scope
            .spawn(|| {
                (
                    material::material_mips_for(catalog),
                    material::companions::prepare(catalog),
                )
            })
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
    let normal_view = material::companions::upload(
        device,
        queue,
        &companions.normal,
        material::texture_layers_for(catalog),
        "voxel normal maps",
    );
    let specular_view = material::companions::upload(
        device,
        queue,
        &companions.specular,
        material::texture_layers_for(catalog),
        "voxel specular maps",
    );
    let map_flags = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("frozen material companion flags"),
        contents: bytemuck::cast_slice(&companions.flags),
        usage: wgpu::BufferUsages::STORAGE,
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
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 5,
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
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&normal_view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(&specular_view),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: map_flags.as_entire_binding(),
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
    let attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32, 4 => Float32x2, 5 => Float32, 6 => Float32];
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

/// Compile depth-only entry points from the same bounded material hooks and
/// vertex layout as the color pipeline. Custom vertex displacement and alpha
/// cutoff therefore participate in the shared map without a second mesh path.
pub(crate) fn create_sun_shadow_pipelines(
    device: &wgpu::Device,
    color: &wgpu::RenderPipeline,
    prepared: Option<&custom::Prepared>,
) -> (wgpu::RenderPipeline, wgpu::RenderPipeline) {
    let hooks = prepared.map_or_else(|| String::from("fn bg_vertex(input: BgVertex, layer: u32) -> BgVertex { return input; }\nfn bg_surface(input: BgSurface, layer: u32) -> BgSurface { return input; }"), custom::compose);
    let source = super::daylight::shader(&format!(
        "{}\n{RELIEF_SHADER}\n{PARALLAX_SHADER}\n{DETAIL_SHADER}\n{hooks}\n{SHADER}",
        custom::TYPES
    ));
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("shared voxel sun casters"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let camera = color.get_bind_group_layout(0);
    let texture = color.get_bind_group_layout(1);
    let visual = prepared.map(|_| color.get_bind_group_layout(2));
    let mut layouts = vec![Some(&camera), Some(&texture)];
    if let Some(layout) = &visual {
        layouts.push(Some(layout));
    }
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("sun voxel caster layout"),
        bind_group_layouts: &layouts,
        immediate_size: 0,
    });
    let attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32, 4 => Float32x2, 5 => Float32, 6 => Float32];
    let pipeline = |label, cull_mode, cutout| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_shadow"),
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
            depth_stencil: Some(super::sun_shadow::depth_state()),
            multisample: Default::default(),
            fragment: if cutout {
                Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_shadow"),
                    compilation_options: Default::default(),
                    targets: &[],
                })
            } else {
                None
            },
            multiview_mask: None,
            cache: None,
        })
    };
    (
        pipeline("opaque sun caster", Some(wgpu::Face::Back), false),
        pipeline("cutout sun caster", None, true),
    )
}

const RELIEF_SHADER: &str = include_str!("material/relief.wgsl");
const PARALLAX_SHADER: &str = include_str!("material/parallax.wgsl");
const DETAIL_SHADER: &str = include_str!("material/companions.wgsl");
const SHADER: &str = include_str!("pipeline.wgsl");
