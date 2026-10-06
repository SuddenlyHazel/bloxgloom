//! Deterministic raster water absorption, shared by near and distant faces.
use super::Frame;
use wgpu::util::DeviceExt;

#[derive(Clone)]
pub(crate) struct Inputs {
    frame: wgpu::Buffer,
    depth: wgpu::TextureView,
    color: wgpu::TextureView,
}
pub(super) struct Optics {
    pub(super) inputs: Inputs,
    snapshot: Option<wgpu::RenderPipeline>,
}
impl Inputs {
    pub(crate) fn fallback(device: &wgpu::Device) -> Self {
        Self {
            frame: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("raster water optical frame"),
                contents: bytemuck::cast_slice(&[0.0f32; 24]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }),
            depth: target(device, 1, 1, wgpu::TextureFormat::R32Float, false),
            color: target(device, 1, 1, crate::render::post::HDR_FORMAT, false),
        }
    }
    pub(crate) fn layout_entries(start: u32) -> [wgpu::BindGroupLayoutEntry; 3] {
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty,
            count: None,
        };
        [
            entry(
                start,
                wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
            ),
            entry(
                start + 1,
                wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
            ),
            entry(
                start + 2,
                wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
            ),
        ]
    }
    pub(crate) fn entries(&self, start: u32) -> [wgpu::BindGroupEntry<'_>; 3] {
        [
            wgpu::BindGroupEntry {
                binding: start,
                resource: self.frame.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: start + 1,
                resource: wgpu::BindingResource::TextureView(&self.depth),
            },
            wgpu::BindGroupEntry {
                binding: start + 2,
                resource: wgpu::BindingResource::TextureView(&self.color),
            },
        ]
    }
}
impl Optics {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        let snapshot = crate::render::post::temporal::supported(device).then(|| {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("opaque water background snapshot"),
                source: wgpu::ShaderSource::Wgsl(include_str!("optics/snapshot.wgsl").into()),
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("opaque water background snapshot"),
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
                    targets: &[
                        Some(wgpu::ColorTargetState {
                            format: crate::render::post::HDR_FORMAT,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        }),
                        Some(wgpu::ColorTargetState {
                            format: wgpu::TextureFormat::R32Float,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        }),
                    ],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        });
        Self {
            inputs: Inputs::fallback(device),
            snapshot,
        }
    }
    pub(super) fn prepare(&self, queue: &wgpu::Queue, frame: Frame) {
        if self.snapshot.is_none() {
            return;
        }
        let mut data = [0.0f32; 24];
        data[..16].copy_from_slice(&frame.view_projection.inverse().to_cols_array());
        data[16..19].copy_from_slice(&frame.camera.position.to_array());
        data[19] = f32::from(frame.eye_in_water);
        data[20..22].copy_from_slice(&[frame.size[0] as f32, frame.size[1] as f32]);
        data[22] = 1.0;
        queue.write_buffer(&self.inputs.frame, 0, bytemuck::cast_slice(&data));
    }
    pub(super) fn begin(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
        depth: &wgpu::TextureView,
    ) {
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        let size = scene.texture().size();
        if self.inputs.color.texture().size() != size {
            self.inputs.color = target(
                device,
                size.width,
                size.height,
                crate::render::post::HDR_FORMAT,
                true,
            );
            self.inputs.depth = target(
                device,
                size.width,
                size.height,
                wgpu::TextureFormat::R32Float,
                true,
            );
        }
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("opaque water background snapshot"),
            layout: &snapshot.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(scene),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
            ],
        });
        let attachment = |view| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("opaque water background snapshot"),
            color_attachments: &[
                attachment(&self.inputs.color),
                attachment(&self.inputs.depth),
            ],
            ..Default::default()
        });
        pass.set_pipeline(snapshot);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }
}
fn target(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    render: bool,
) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("raster water optical snapshot"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | if render {
                    wgpu::TextureUsages::RENDER_ATTACHMENT
                } else {
                    wgpu::TextureUsages::empty()
                },
            view_formats: &[],
        })
        .create_view(&Default::default())
}
pub(super) fn source(source: &str, lod: bool) -> String {
    let (group, start) = if lod { (0, 3) } else { (1, 1) };
    let body = source
        .replace(
            "return bg_water_surface(v.color,",
            "return bg_enhanced_water_surface(v.color,",
        )
        .replace(
            "bg_sun_visibility(receiver),footprint);",
            "bg_sun_visibility(receiver),footprint,v.position.xy);",
        );
    format!(
        "struct BgWaterOptics {{inverse:mat4x4f,eye:vec4f,properties:vec4f}};\n@group({group}) @binding({start}) var<uniform> bg_water_optics:BgWaterOptics;\n@group({group}) @binding({}) var bg_water_bed_depth:texture_2d<f32>;\n@group({group}) @binding({}) var bg_water_bed_color:texture_2d<f32>;\n{}\n{body}",
        start + 1,
        start + 2,
        include_str!("optics/composition.wgsl")
    )
}
#[cfg(test)]
#[path = "optics/tests.rs"]
mod tests;
