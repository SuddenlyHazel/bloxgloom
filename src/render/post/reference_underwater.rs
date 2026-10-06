//! Source normal-water composite absorption and pre-tone scene/bloom distortion.
//! Front depth is supplied explicitly by reference water, never opaque fallback.
use super::HDR_FORMAT;
use wgpu::util::DeviceExt;
const FOG: &str = include_str!("reference_underwater/fog.wgsl");
const DISTORTION: &str = include_str!("reference_underwater/distortion.wgsl");
#[cfg(test)]
mod tests;
pub(super) struct Underwater {
    pub depth: Option<wgpu::TextureView>,
    scratch: wgpu::TextureView,
    uniform: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    composite_layout: wgpu::BindGroupLayout,
    fog: wgpu::RenderPipeline,
    copy: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    data: [f32; 28],
    water: bool,
}
impl Underwater {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
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
        let uniform = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let sampler = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let float = wgpu::TextureSampleType::Float { filterable: true };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("reference underwater front-depth inputs"),
            entries: &[
                texture(0, float),
                texture(1, wgpu::TextureSampleType::Depth),
                uniform(2),
                sampler(3),
            ],
        });
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("reference underwater scene/bloom distortion"),
            entries: &[
                texture(0, float),
                texture(1, float),
                sampler(2),
                uniform(3),
                uniform(4),
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("source underwater final-composite absorption"),
            source: wgpu::ShaderSource::Wgsl(FOG.into()),
        });
        let composite = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("source underwater pre-tone distortion"),
            source: wgpu::ShaderSource::Wgsl(composite_source().into()),
        });
        Self {
            depth: None,
            scratch: target(device, width, height),
            uniform: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("source underwater frame"),
                contents: bytemuck::cast_slice(&[0.0_f32; 28]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }),
            fog: pipeline(device, &shader, &layout, "vs", "fog"),
            copy: pipeline(device, &shader, &layout, "vs", "copy"),
            composite: pipeline(
                device,
                &composite,
                &composite_layout,
                "vs_main",
                "composite",
            ),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("source underwater linear clamp"),
                min_filter: wgpu::FilterMode::Linear,
                mag_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            layout,
            composite_layout,
            data: [0.0; 28],
            water: false,
        }
    }
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.scratch = target(device, width, height);
        self.depth = None;
    }
    pub fn configure(
        &mut self,
        matrix: glam::Mat4,
        eye: glam::Vec3,
        atmosphere: crate::render::daylight::Atmosphere,
        water: bool,
    ) {
        self.data[..16].copy_from_slice(&matrix.inverse().to_cols_array());
        self.data[16..19].copy_from_slice(&eye.to_array());
        self.data[20..23].copy_from_slice(&fog_color(atmosphere).to_array());
        self.data[24] = atmosphere.presentation_seconds;
        self.water = water;
    }
    pub fn resolve(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
        enabled: bool,
    ) {
        queue.write_buffer(&self.uniform, 0, bytemuck::cast_slice(&self.data));
        if !self.water || !enabled {
            return;
        }
        let Some(depth) = &self.depth else {
            return;
        };
        let group = |scene| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("source underwater absorption input"),
                layout: &self.layout,
                entries: &[
                    super::reference_display::texture_entry(0, scene),
                    super::reference_display::texture_entry(1, depth),
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            })
        };
        super::reference_display::draw(encoder, &self.fog, &group(scene), &[&self.scratch]);
        super::reference_display::draw(encoder, &self.copy, &group(&self.scratch), &[scene]);
    }
    #[allow(clippy::too_many_arguments)]
    pub fn composite(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
        bloom: &wgpu::TextureView,
        settings: &wgpu::Buffer,
        target: &wgpu::TextureView,
    ) -> bool {
        if !self.water {
            return false;
        }
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("source underwater distorted scene and bloom"),
            layout: &self.composite_layout,
            entries: &[
                super::reference_display::texture_entry(0, scene),
                super::reference_display::texture_entry(1, bloom),
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: settings.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.uniform.as_entire_binding(),
                },
            ],
        });
        super::reference_display::draw(encoder, &self.composite, &group, &[target]);
        true
    }
}
fn fog_color(atmosphere: crate::render::daylight::Atmosphere) -> glam::Vec3 {
    let light = crate::render::bsl_reference::palettes(atmosphere).0;
    // Source WATER_MODE0, WATER_I=.35, WATER_F=1.2; alpha is I².
    let palette = glam::Vec3::new(64.0, 160.0, 255.0) * (0.35 / 255.0);
    let tint =
        light * atmosphere.fog_exposure.clamp(0.0, 1.0) * atmosphere.reference_shadow_fade * 0.9
            + glam::Vec3::splat(0.1);
    palette * palette * (0.35 * 0.35 * 1.2 * 1.2) * (tint * tint.length()).sqrt()
}
fn composite_source() -> String {
    // Keep tone/vignette/bloom consumer shared. Only the two source sampled UVs
    // change; screen-space vignette and lens coordinates retain original UV.
    let source = include_str!("../post.wgsl");
    assert_eq!(
        source
            .matches("var hdr = textureSample(scene, linear_sampler, input.uv).rgb;")
            .count(),
        1
    );
    let source=source.replace("var hdr = textureSample(scene, linear_sampler, input.uv).rgb;","let sample_uv=bg_reference_underwater_uv(input.uv);\nvar hdr = textureSample(scene, linear_sampler, sample_uv).rgb;").replace("bg_reference_bloom(hdr, input.uv)","bg_reference_bloom(hdr, sample_uv)");
    format!(
        "{}\nconst BG_BSL_STYLE:bool={};\nconst BG_REFERENCE_BLOOM:bool=true;\nconst BG_REFERENCE_DISPLAY:bool=true;\n{}\n{DISTORTION}\n{source}\n{}",
        crate::render::sky::STYLE_SHADER,
        crate::render::sky::style_enabled(),
        super::reference_bloom::COMMON,
        super::reference_bloom::COMPOSITE
    )
}
fn target(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("reference underwater linear scratch"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default())
}
fn pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::BindGroupLayout,
    vertex: &str,
    fragment: &str,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(fragment),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(vertex),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: HDR_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}
