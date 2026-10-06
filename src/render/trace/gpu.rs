//! Half-resolution path transport and depth/normal-aware radiance reconstruction.
use super::scene::Scene;
use crate::render::{daylight::Atmosphere, post::HDR_FORMAT};
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;
struct Geometry {
    nodes: wgpu::Buffer,
    source_triangles: wgpu::Buffer,
    triangles: wgpu::Buffer,
    coverage: wgpu::Buffer,
    count: u32,
}
pub(crate) struct Gpu {
    geometry: Geometry,
    deformation: super::deformation::Deformation,
    uniform: wgpu::Buffer,
    trace: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    filter: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    composite_layout: wgpu::BindGroupLayout,
    history: [wgpu::TextureView; 2],
    history_geometry: [wgpu::TextureView; 2],
    primary_transmission: [wgpu::TextureView; 2],
    filtered: wgpu::TextureView,
    frame: u32,
    previous: Mat4,
    previous_eye: Vec3,
    previous_atmosphere: Option<Atmosphere>,
}
fn buffer(device: &wgpu::Device, label: &str, data: &[u8]) -> wgpu::Buffer {
    let empty = [0u8; 96];
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: if data.is_empty() { &empty } else { data },
        usage: wgpu::BufferUsages::STORAGE,
    })
}
fn resolution_scale() -> u32 {
    match std::env::var("BLOXGLOOM_GI_SCALE").as_deref() {
        Ok("8") => 8,
        Ok("4") => 4,
        _ => 2,
    }
}
impl Geometry {
    fn new(device: &wgpu::Device, scene: &Scene) -> Self {
        Self {
            nodes: buffer(device, "ray BVH", bytemuck::cast_slice(&scene.nodes)),
            source_triangles: buffer(
                device,
                "undeformed ray triangles",
                bytemuck::cast_slice(&scene.triangles),
            ),
            triangles: buffer(
                device,
                "ray triangles",
                bytemuck::cast_slice(&scene.triangles),
            ),
            coverage: buffer(
                device,
                "ray loaded coverage",
                bytemuck::cast_slice(&scene.coverage),
            ),
            count: scene.nodes.len() as u32,
        }
    }
}
impl Gpu {
    #[cfg(test)]
    pub(super) fn target_size(&self) -> wgpu::Extent3d {
        self.history[0].texture().size()
    }
    pub fn new(
        device: &wgpu::Device,
        scene: &Scene,
        size: wgpu::Extent3d,
        material_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        tracing::debug!(
            bytes = scene.byte_len(),
            triangles = scene.triangles.len(),
            "ray scene admitted"
        );
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty,
            count: None,
        };
        let texture = |depth| wgpu::BindingType::Texture {
            sample_type: if depth {
                wgpu::TextureSampleType::Depth
            } else {
                wgpu::TextureSampleType::Float { filterable: false }
            },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        };
        let storage = wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("path transport inputs"),
            entries: &[
                entry(
                    0,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                entry(1, storage),
                entry(2, storage),
                entry(3, texture(true)),
                entry(4, texture(false)),
                entry(5, texture(false)),
                entry(6, texture(false)),
                entry(7, texture(false)),
                entry(8, texture(false)),
                entry(9, texture(false)),
                entry(10, storage),
            ],
        });
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ray radiance reconstruction"),
            entries: &[
                entry(0, texture(false)),
                entry(1, texture(false)),
                entry(
                    2,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                entry(3, texture(false)),
                entry(4, texture(false)),
                entry(5, texture(true)),
                entry(6, texture(false)),
                entry(7, texture(false)),
            ],
        });
        let source = format!(
            "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            crate::render::sky::environment_shader(),
            include_str!("../material/pbr.wgsl"),
            include_str!("../material/foliage.wgsl"),
            include_str!("../material/foliage_optics.wgsl"),
            include_str!("intersection.wgsl"),
            include_str!("coverage.wgsl"),
            include_str!("denoise.wgsl"),
            include_str!("transport.wgsl"),
            include_str!("medium.wgsl")
        );
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("path transport"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let composition = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ray radiance reconstruction"),
            source: wgpu::ShaderSource::Wgsl(
                format!(
                    "{}\n{}\n{}\n{}",
                    include_str!("../material/pbr.wgsl"),
                    include_str!("denoise.wgsl"),
                    include_str!("filter.wgsl"),
                    include_str!("composite.wgsl")
                )
                .into(),
            ),
        });
        let pipeline = |shader: &wgpu::ShaderModule,
                        layouts: &[Option<&wgpu::BindGroupLayout>],
                        fragment,
                        blend| {
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: layouts,
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(fragment),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some("vs_main"),
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
                    targets: &if fragment == "fs_transport" {
                        [HDR_FORMAT, HDR_FORMAT, wgpu::TextureFormat::R16Float]
                            .into_iter()
                            .map(|format| {
                                Some(wgpu::ColorTargetState {
                                    format,
                                    blend: None,
                                    write_mask: wgpu::ColorWrites::ALL,
                                })
                            })
                            .collect()
                    } else {
                        vec![Some(wgpu::ColorTargetState {
                            format: HDR_FORMAT,
                            blend,
                            write_mask: wgpu::ColorWrites::ALL,
                        })]
                    },
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let trace = pipeline(
            &shader,
            &[Some(&layout), Some(material_layout)],
            "fs_transport",
            None,
        );
        let additive = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let composite = pipeline(
            &composition,
            &[Some(&composite_layout)],
            "fs_main",
            Some(additive),
        );
        let filter = pipeline(&composition, &[Some(&composite_layout)], "fs_filter", None);
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ray frame"),
            size: 288,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            deformation: super::deformation::Deformation::new(device, material_layout),
            geometry: Geometry::new(device, scene),
            uniform,
            trace,
            composite,
            filter,
            layout,
            composite_layout,
            history: Self::targets(device, size, HDR_FORMAT),
            history_geometry: Self::targets(device, size, HDR_FORMAT),
            primary_transmission: Self::targets(device, size, wgpu::TextureFormat::R16Float),
            filtered: Self::target(device, size, HDR_FORMAT),
            frame: 0,
            previous: Mat4::IDENTITY,
            previous_eye: Vec3::ZERO,
            previous_atmosphere: None,
        }
    }
    fn targets(
        device: &wgpu::Device,
        size: wgpu::Extent3d,
        format: wgpu::TextureFormat,
    ) -> [wgpu::TextureView; 2] {
        std::array::from_fn(|_| Self::target(device, size, format))
    }
    fn target(
        device: &wgpu::Device,
        size: wgpu::Extent3d,
        format: wgpu::TextureFormat,
    ) -> wgpu::TextureView {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("ray transport-sized target"),
                size: wgpu::Extent3d {
                    width: size.width.div_ceil(resolution_scale()).max(1),
                    height: size.height.div_ceil(resolution_scale()).max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default())
    }
    pub fn ensure_size(&mut self, device: &wgpu::Device, size: wgpu::Extent3d) {
        let expected = wgpu::Extent3d {
            width: size.width.div_ceil(resolution_scale()).max(1),
            height: size.height.div_ceil(resolution_scale()).max(1),
            depth_or_array_layers: 1,
        };
        if self.history[0].texture().size() != expected {
            self.resize(device, size);
        }
    }
    pub fn resize(&mut self, device: &wgpu::Device, size: wgpu::Extent3d) {
        self.history = Self::targets(device, size, HDR_FORMAT);
        self.history_geometry = Self::targets(device, size, HDR_FORMAT);
        self.primary_transmission = Self::targets(device, size, wgpu::TextureFormat::R16Float);
        self.filtered = Self::target(device, size, HDR_FORMAT);
        self.frame = 0;
    }
    #[allow(clippy::too_many_arguments)]
    pub fn resolve(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        normal: &wgpu::TextureView,
        response: &wgpu::TextureView,
        indirect: &wgpu::TextureView,
        materials: &wgpu::BindGroup,
        matrix: Mat4,
        eye: Vec3,
        atmosphere: Atmosphere,
        profile: Option<&super::profiling::Frame>,
    ) {
        let mut data = Vec::with_capacity(72);
        data.extend(matrix.inverse().to_cols_array());
        data.extend(self.previous.to_cols_array());
        data.extend([eye.x, eye.y, eye.z, atmosphere.wind_seconds]);
        data.extend([
            atmosphere.sun.x,
            atmosphere.sun.y,
            atmosphere.sun.z,
            (atmosphere.sun.y / crate::render::SUN_DIRECTION.normalize().y).clamp(0.0, 1.0),
        ]);
        let camera = atmosphere.camera_data(matrix, eye);
        let solar = Atmosphere {
            cloud: 0.0,
            ..atmosphere
        }
        .sun_radiance();
        data.extend([solar.x, solar.y, solar.z, 0.0]);
        data.extend_from_slice(&camera[20..24]);
        data.extend_from_slice(&camera[40..44]);
        data.extend([
            atmosphere.cloud,
            atmosphere.drift[0],
            atmosphere.drift[1],
            if crate::render::sky::style_enabled() {
                1.0
            } else {
                0.0
            },
        ]);
        data.extend([
            atmosphere.rain_strength,
            atmosphere.moon_multiplier(),
            atmosphere.moon_phase as f32,
            atmosphere.presentation_seconds,
        ]);
        data.extend([
            0.0004 + atmosphere.fog * 0.003,
            1.0,
            resolution_scale() as f32,
            f32::from(std::env::var("BLOXGLOOM_GI_OPAQUE_PRECHECK").as_deref() == Ok("1")),
        ]);
        data.extend([
            self.previous_eye.x,
            self.previous_eye.y,
            self.previous_eye.z,
            0.0,
        ]);
        let history = self.frame > 0
            && eye.distance(self.previous_eye) < 1.5
            && self.previous_atmosphere.is_some_and(|old| {
                old.sun.dot(atmosphere.sun) > 0.9995
                    && (old.cloud - atmosphere.cloud).abs() < 0.03
                    && (old.fog - atmosphere.fog).abs() < 0.03
                    && (old.rain_strength - atmosphere.rain_strength).abs() < 0.03
                    && old.moon_phase == atmosphere.moon_phase
                    && (old.sun_radiance() - atmosphere.sun_radiance()).length() < 0.1
            });
        data.extend([
            f32::from_bits(self.geometry.count),
            f32::from_bits(0),
            f32::from_bits(self.frame),
            f32::from_bits(u32::from(history)),
        ]);
        queue.write_buffer(&self.uniform, 0, bytemuck::cast_slice(&data));
        self.deformation.encode(
            device,
            encoder,
            &self.uniform,
            &self.geometry.source_triangles,
            &self.geometry.triangles,
            materials,
            profile.map(super::profiling::Frame::compute),
        );
        let old = (self.frame as usize + 1) % 2;
        let current = self.frame as usize % 2;
        let binding = |binding, resource| wgpu::BindGroupEntry { binding, resource };
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ray scene frame"),
            layout: &self.layout,
            entries: &[
                binding(0, self.uniform.as_entire_binding()),
                binding(1, self.geometry.nodes.as_entire_binding()),
                binding(2, self.geometry.triangles.as_entire_binding()),
                binding(3, wgpu::BindingResource::TextureView(depth)),
                binding(4, wgpu::BindingResource::TextureView(normal)),
                binding(5, wgpu::BindingResource::TextureView(response)),
                binding(6, wgpu::BindingResource::TextureView(indirect)),
                binding(7, wgpu::BindingResource::TextureView(&self.history[old])),
                binding(
                    8,
                    wgpu::BindingResource::TextureView(&self.history_geometry[old]),
                ),
                binding(9, wgpu::BindingResource::TextureView(scene)),
                binding(10, self.geometry.coverage.as_entire_binding()),
            ],
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("multi-bounce ray transport"),
                timestamp_writes: profile.map(|frame| frame.render(1)),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment {
                        view: &self.history[current],
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: &self.history_geometry[current],
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: &self.primary_transmission[current],
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                ],
                ..Default::default()
            });
            pass.set_pipeline(&self.trace);
            pass.set_bind_group(0, &group, &[]);
            pass.set_bind_group(1, materials, &[]);
            pass.draw(0..3, 0..1);
        }
        let compose_group = |radiance: &wgpu::TextureView| {
            let binding = |binding, resource| wgpu::BindGroupEntry { binding, resource };
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ray composite"),
                layout: &self.composite_layout,
                entries: &[
                    binding(0, wgpu::BindingResource::TextureView(radiance)),
                    binding(1, wgpu::BindingResource::TextureView(normal)),
                    binding(2, self.uniform.as_entire_binding()),
                    binding(
                        3,
                        wgpu::BindingResource::TextureView(&self.history_geometry[current]),
                    ),
                    binding(4, wgpu::BindingResource::TextureView(indirect)),
                    binding(5, wgpu::BindingResource::TextureView(depth)),
                    binding(6, wgpu::BindingResource::TextureView(response)),
                    binding(
                        7,
                        wgpu::BindingResource::TextureView(&self.primary_transmission[current]),
                    ),
                ],
            })
        };
        let filtering = compose_group(&self.history[current]);
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("low resolution ray radiance filter"),
                timestamp_writes: profile.map(|frame| frame.render(2)),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.filtered,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.filter);
            pass.set_bind_group(0, &filtering, &[]);
            pass.draw(0..3, 0..1);
        }
        let compose = compose_group(&self.filtered);
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ray radiance reconstruction"),
                timestamp_writes: profile.map(|frame| frame.render(3)),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: scene,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.composite);
            pass.set_bind_group(0, &compose, &[]);
            pass.draw(0..3, 0..1);
        }
        self.frame = self.frame.wrapping_add(1);
        self.previous = matrix;
        self.previous_eye = eye;
        self.previous_atmosphere = Some(atmosphere);
    }
}
