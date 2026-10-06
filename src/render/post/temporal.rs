//! Camera/object-reprojected temporal AA, before bloom/display mapping.
//! Enabled by default; BLOXGLOOM_TAA=0 retains single-frame rendering.
//! History stores linear depth separately from HDR color (half-float device depth
//! loses too much precision). A resolve copies back to scene so authored effects
//! see the same stable HDR input and never accumulate their own output.
use glam::{Mat4, Vec2, Vec3};

#[cfg(test)]
mod tests;

/// Naga's GLSL backend cannot load depth textures (and non-comparison depth
/// sampling is also unsupported). Do not turn an optional effect into a crash.
pub(crate) fn supported(device: &wgpu::Device) -> bool {
    device.adapter_info().backend != wgpu::Backend::Gl
}

pub(crate) struct Temporal {
    colors: [wgpu::TextureView; 2],
    depths: [wgpu::TextureView; 2],
    pub(super) motion: wgpu::TextureView,
    pub(super) motion_frame: Option<crate::render::avatars::motion::Frame>,
    resolved: bool,
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    settings: wgpu::Buffer,
    sampler: wgpu::Sampler,
    previous: Option<(Mat4, crate::render::Camera)>,
    pending: Option<(Mat4, crate::render::Camera)>,
    frame: u32,
    index: usize,
    valid: bool,
}

pub(crate) fn jitter(frame: u32) -> Vec2 {
    fn radical(mut index: u32, base: u32) -> f32 {
        let mut value = 0.0;
        let mut factor = 1.0;
        while index > 0 {
            factor /= base as f32;
            value += factor * (index % base) as f32;
            index /= base;
        }
        value
    }
    let index = frame % 8 + 1;
    Vec2::new(radical(index, 2), radical(index, 3)) - Vec2::splat(0.5)
}

pub(crate) fn jitter_matrix(matrix: Mat4, offset: Vec2, width: u32, height: u32) -> Mat4 {
    Mat4::from_translation(Vec3::new(
        2.0 * offset.x / width.max(1) as f32,
        -2.0 * offset.y / height.max(1) as f32,
        0.0,
    )) * matrix
}

fn continuous(previous: crate::render::Camera, current: crate::render::Camera) -> bool {
    previous.position.distance(current.position) < 2.0
        && previous.direction().dot(current.direction()) > 0.9
        && (previous.fov_y_radians - current.fov_y_radians).abs() < 0.0001
}

impl Temporal {
    pub(crate) fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let texture = |label, format| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
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
        };
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty,
            count: None,
        };
        let tex = |sample_type| wgpu::BindingType::Texture {
            sample_type,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("temporal inputs"),
            entries: &[
                entry(0, tex(wgpu::TextureSampleType::Float { filterable: true })),
                entry(1, tex(wgpu::TextureSampleType::Depth)),
                entry(2, tex(wgpu::TextureSampleType::Float { filterable: true })),
                entry(3, tex(wgpu::TextureSampleType::Float { filterable: false })),
                entry(6, tex(wgpu::TextureSampleType::Float { filterable: true })),
                entry(7, tex(wgpu::TextureSampleType::Float { filterable: true })),
                entry(
                    4,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
                entry(
                    5,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("temporal resolve"),
            source: wgpu::ShaderSource::Wgsl(include_str!("temporal.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("temporal layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let target = |format| {
            Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("temporal resolve"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("resolve"),
                compilation_options: Default::default(),
                targets: &[
                    target(super::HDR_FORMAT),
                    target(wgpu::TextureFormat::R32Float),
                ],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            colors: [
                texture("temporal color A", super::HDR_FORMAT),
                texture("temporal color B", super::HDR_FORMAT),
            ],
            depths: [
                texture("temporal depth A", wgpu::TextureFormat::R32Float),
                texture("temporal depth B", wgpu::TextureFormat::R32Float),
            ],
            motion: texture(
                "temporal object motion",
                crate::render::avatars::motion::FORMAT,
            ),
            motion_frame: None,
            resolved: false,
            layout,
            pipeline,
            settings: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("temporal camera"),
                size: 160,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("temporal clamp"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            previous: None,
            pending: None,
            frame: 0,
            index: 0,
            valid: false,
        }
    }

    pub(crate) fn prepare(
        &mut self,
        queue: &wgpu::Queue,
        camera: crate::render::Camera,
        width: u32,
        height: u32,
    ) -> (Mat4, Vec2) {
        let offset = jitter(self.frame);
        let stable = crate::render::view_projection(camera, width, height);
        let matrix = jitter_matrix(stable, offset, width, height);
        let (previous, valid) = self.previous.map_or((stable, false), |(matrix, old)| {
            (matrix, self.valid && continuous(old, camera))
        });
        let mut data = Vec::with_capacity(40);
        // Reconstruct the actual raster sample. The shader adds current jitter
        // back after projecting to previous stable coordinates, so a stationary
        // camera has zero output motion and can accumulate different samples.
        data.extend(matrix.inverse().to_cols_array());
        data.extend(previous.to_cols_array());
        data.extend([0.9, if valid { 1.0 } else { 0.0 }, 0.01, 0.0]);
        data.extend([
            crate::render::visibility::CAMERA_NEAR,
            crate::render::visibility::CAMERA_FAR,
            offset.x / width.max(1) as f32,
            offset.y / height.max(1) as f32,
        ]);
        queue.write_buffer(&self.settings, 0, bytemuck::cast_slice(&data));
        self.motion_frame = Some(crate::render::avatars::motion::Frame {
            previous: previous.to_cols_array(),
            viewport: [width as f32, height as f32, offset.x, offset.y],
            depth: [
                crate::render::visibility::CAMERA_NEAR,
                crate::render::visibility::CAMERA_FAR,
                if valid { 1.0 } else { 0.0 },
                0.0,
            ],
        });
        self.pending = Some((stable, camera));
        self.resolved = false;
        (matrix, offset)
    }

    pub(crate) fn resolve(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        reactive: Option<&wgpu::TextureView>,
    ) {
        // Only a submitted resolve advances history. Surface acquisition failures
        // can call prepare again without inventing a nonexistent previous frame.
        if self.pending.is_none() || self.resolved {
            return;
        }
        let source = self.index;
        let target = 1 - source;
        let bind = |binding, view| wgpu::BindGroupEntry {
            binding,
            resource: wgpu::BindingResource::TextureView(view),
        };
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("temporal frame"),
            layout: &self.layout,
            entries: &[
                bind(0, scene),
                bind(1, depth),
                bind(2, &self.colors[source]),
                bind(3, &self.depths[source]),
                bind(6, &self.motion),
                bind(7, reactive.unwrap_or(&self.motion)),
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: self.settings.as_entire_binding(),
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
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("temporal AA"),
                color_attachments: &[
                    attachment(&self.colors[target]),
                    attachment(&self.depths[target]),
                ],
                ..Default::default()
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..3, 0..1);
        }
        encoder.copy_texture_to_texture(
            self.colors[target].texture().as_image_copy(),
            scene.texture().as_image_copy(),
            scene.texture().size(),
        );
        self.resolved = true;
    }

    pub(super) fn submitted(&mut self) {
        if !self.resolved {
            return;
        }
        self.resolved = false;
        self.index = 1 - self.index;
        self.previous = self.pending.take();
        self.valid = true;
        self.frame = self.frame.wrapping_add(1);
    }
}
