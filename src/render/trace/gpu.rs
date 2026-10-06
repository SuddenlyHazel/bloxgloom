//! Half-resolution path transport and depth/normal-aware radiance reconstruction.
mod diagnostics;
mod empty_dynamic;
mod history;
mod lobes;
mod lod;
#[cfg(test)]
mod primary_cost;
mod scheduling;
mod shaders;
use super::scene::Scene;
use crate::render::{daylight::Atmosphere, post::HDR_FORMAT};
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;
#[cfg(test)]
pub(super) fn transport_probe_source() -> String {
    shaders::transport_with_lod_vector(false, false, false)
}
struct Geometry {
    nodes: wgpu::Buffer,
    source_triangles: wgpu::Buffer,
    triangles: wgpu::Buffer,
    coverage: wgpu::Buffer,
    count: u32,
    water_offset: u32,
}
pub(crate) struct Gpu {
    geometry: Geometry,
    lod: lod::LodGeometry,
    dynamic: super::dynamic::DynamicGpu,
    baseline: wgpu::TextureView,
    deformation: super::deformation::Deformation,
    uniform: wgpu::Buffer,
    trace: wgpu::RenderPipeline,
    empty_trace: Option<wgpu::RenderPipeline>,
    composite: wgpu::RenderPipeline,
    filter: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    composite_layout: wgpu::BindGroupLayout,
    history: [wgpu::TextureView; 2],
    history_geometry: [wgpu::TextureView; 2],
    primary_transmission: [wgpu::TextureView; 2],
    filtered: wgpu::TextureView,
    current_correction: wgpu::TextureView,
    transport_tile_edge: u32,
    transport_checkpoint: Option<scheduling::Checkpoint>,
    scheduling_error: Option<String>,
    history_samples: u32,
    water_lobes: bool,
    water_reconstruction: Option<lobes::Reconstruction>,
    #[cfg(test)]
    sample_seed: Option<u32>,
    frame: u32,
    history_valid: bool,
    pub eye_water: bool,
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
            water_offset: scene.water_offset,
        }
    }
}
impl Gpu {
    #[cfg(test)]
    pub(super) fn target_size(&self) -> wgpu::Extent3d {
        self.history[0].texture().size()
    }
    #[cfg(test)]
    pub fn new_with_lod(
        device: &wgpu::Device,
        scene: &Scene,
        lod_pages: &[Scene],
        size: wgpu::Extent3d,
        material_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        Self::new_with_lod_mode(
            device,
            scene,
            lod_pages,
            size,
            material_layout,
            lobes::configured(),
        )
    }
    pub fn new_with_lod_supported(
        device: &wgpu::Device,
        scene: &Scene,
        lod_pages: &[Scene],
        size: wgpu::Extent3d,
        material_layout: &wgpu::BindGroupLayout,
        water_support: bool,
    ) -> Self {
        Self::new_with_lod_options(
            device,
            scene,
            lod_pages,
            size,
            material_layout,
            lobes::Mode::configured(water_support),
        )
    }
    pub(in crate::render::trace) fn water_reconstruction_supported(
        adapter: &wgpu::Adapter,
    ) -> bool {
        lobes::reconstruction_supported(adapter)
    }
    pub(in crate::render::trace) fn water_reconstruction_requested() -> bool {
        lobes::reconstruction_requested()
    }
    #[cfg(test)]
    pub(super) fn new_with_lod_mode(
        device: &wgpu::Device,
        scene: &Scene,
        lod_pages: &[Scene],
        size: wgpu::Extent3d,
        material_layout: &wgpu::BindGroupLayout,
        water_lobes: bool,
    ) -> Self {
        Self::new_with_lod_options(
            device,
            scene,
            lod_pages,
            size,
            material_layout,
            lobes::Mode {
                split: water_lobes,
                reconstruction: false,
                filtering: false,
            },
        )
    }
    fn new_with_lod_options(
        device: &wgpu::Device,
        scene: &Scene,
        lod_pages: &[Scene],
        size: wgpu::Extent3d,
        material_layout: &wgpu::BindGroupLayout,
        mode: lobes::Mode,
    ) -> Self {
        let transport_size = wgpu::Extent3d {
            width: size.width.div_ceil(resolution_scale()).max(1),
            height: size.height.div_ceil(resolution_scale()).max(1),
            depth_or_array_layers: 1,
        };
        let reconstruction =
            mode.reconstruction && lobes::Reconstruction::fits(device, transport_size);
        if mode.reconstruction && !reconstruction {
            tracing::warn!("first-water reconstruction exceeds256MiB; using split-only path");
        }
        let water_lobes = mode.split || reconstruction;
        let filter_lobes = reconstruction && mode.filtering;
        let dynamic = super::dynamic::DynamicGpu::new(device);
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
        let mut entries = vec![
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
            entry(15, texture(false)),
        ];
        entries.extend(lod::LodGeometry::layout_entries());
        if reconstruction {
            entries.push(lobes::Reconstruction::storage_entry());
        }
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("path transport inputs"),
            entries: &entries,
        });
        let mut composite_entries = vec![
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
            entry(8, texture(false)),
            entry(9, texture(false)),
        ];
        if filter_lobes {
            composite_entries.extend([entry(10, texture(false)), entry(11, texture(false))]);
        }
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ray radiance reconstruction"),
            entries: &composite_entries,
        });
        let source = shaders::transport_for_modes(water_lobes, reconstruction);
        let empty_source = empty_dynamic::configured(water_lobes)
            .then(|| empty_dynamic::source_for(&source, true));
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("path transport"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let composition_source = shaders::composition(water_lobes, filter_lobes);
        let composition = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ray radiance reconstruction"),
            source: wgpu::ShaderSource::Wgsl(composition_source.into()),
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
                        [
                            HDR_FORMAT,
                            HDR_FORMAT,
                            lobes::format(water_lobes),
                            HDR_FORMAT,
                        ]
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
            &[Some(&layout), Some(material_layout), Some(&dynamic.layout)],
            "fs_transport",
            None,
        );
        let empty_trace = empty_source.map(|source| {
            empty_dynamic::pipeline(
                device,
                &source,
                &layout,
                material_layout,
                &dynamic.layout,
                water_lobes,
            )
        });
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
            size: 304,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            lod: lod::LodGeometry::new(device, lod_pages),
            dynamic,
            baseline: Self::baseline(device, size),
            deformation: super::deformation::Deformation::new(device, material_layout),
            geometry: Geometry::new(device, scene),
            uniform,
            trace,
            empty_trace,
            composite,
            filter,
            layout,
            composite_layout,
            history: Self::targets(device, size, HDR_FORMAT),
            history_geometry: Self::targets(device, size, HDR_FORMAT),
            primary_transmission: Self::targets(device, size, lobes::format(water_lobes)),
            filtered: Self::target(device, size, HDR_FORMAT),
            current_correction: Self::target(device, size, HDR_FORMAT),
            transport_tile_edge: scheduling::configured_edge(),
            transport_checkpoint: None,
            scheduling_error: None,
            history_samples: history::configured_samples(),
            water_lobes,
            water_reconstruction: reconstruction
                .then(|| lobes::Reconstruction::new(device, transport_size, mode.filtering)),
            #[cfg(test)]
            sample_seed: None,
            frame: 0,
            history_valid: false,
            eye_water: false,
            previous: Mat4::IDENTITY,
            previous_eye: Vec3::ZERO,
            previous_atmosphere: None,
        }
    }
    fn baseline(device: &wgpu::Device, size: wgpu::Extent3d) -> wgpu::TextureView {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("immutable pre-trace HDR baseline"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: HDR_FORMAT,
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default())
    }
    pub fn dynamic_bytes(&self) -> usize {
        self.dynamic.byte_len()
    }
    pub fn prepare_dynamic(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        targets: &super::dynamic::DynamicTargets,
    ) -> bool {
        let ready = self.dynamic.set(device, queue, targets);
        if self.dynamic.history_changed() {
            self.history_valid = false;
        }
        ready
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
        if self.history[0].texture().size() != expected || self.baseline.texture().size() != size {
            self.resize(device, size);
        }
    }
    pub fn resize(&mut self, device: &wgpu::Device, size: wgpu::Extent3d) {
        self.history = Self::targets(device, size, HDR_FORMAT);
        self.history_geometry = Self::targets(device, size, HDR_FORMAT);
        self.primary_transmission = Self::targets(device, size, lobes::format(self.water_lobes));
        self.filtered = Self::target(device, size, HDR_FORMAT);
        self.current_correction = Self::target(device, size, HDR_FORMAT);
        self.baseline = Self::baseline(device, size);
        if let Some(reconstruction) = &mut self.water_reconstruction {
            reconstruction.resize(device, self.history[0].texture().size());
        }
        self.frame = 0;
        self.history_valid = false;
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
        water_time: f32,
        profile: Option<&super::profiling::Frame>,
    ) {
        let mut data = Vec::with_capacity(76);
        // Remove world translation before inversion: far-point subtraction at
        // kilometre-scale origins otherwise perturbs grazing camera rays.
        data.extend(
            (matrix * Mat4::from_translation(eye))
                .inverse()
                .to_cols_array(),
        );
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
            self.history_samples as f32,
        ]);
        let history = self.history_valid
            && self.frame > 0
            && eye.distance(self.previous_eye) < 1.5
            && self.previous_atmosphere.is_some_and(|old| {
                old.sun.dot(atmosphere.sun) > 0.9995
                    && (old.cloud - atmosphere.cloud).abs() < 0.03
                    && (old.fog - atmosphere.fog).abs() < 0.03
                    && (old.rain_strength - atmosphere.rain_strength).abs() < 0.03
                    && old.moon_phase == atmosphere.moon_phase
                    && (old.sun_radiance() - atmosphere.sun_radiance()).length() < 0.1
            });
        #[cfg(test)]
        let sample_seed = self.sample_seed.unwrap_or(self.frame);
        #[cfg(not(test))]
        let sample_seed = self.frame;
        data.extend([
            f32::from_bits(self.geometry.count),
            f32::from_bits(0),
            f32::from_bits(sample_seed),
            f32::from_bits(u32::from(history)),
        ]);
        data.extend([
            water_time,
            f32::from_bits(self.geometry.water_offset),
            f32::from(self.eye_water),
            f32::from(std::env::var("BLOXGLOOM_GI_WATER_PLANE_HISTORY").as_deref() != Ok("0")),
        ]);
        queue.write_buffer(&self.uniform, 0, bytemuck::cast_slice(&data));
        encoder.copy_texture_to_texture(
            scene.texture().as_image_copy(),
            self.baseline.texture().as_image_copy(),
            scene.texture().size(),
        );
        self.dynamic.encode(encoder);
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
        let mut entries = vec![
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
            binding(9, wgpu::BindingResource::TextureView(&self.baseline)),
            binding(10, self.geometry.coverage.as_entire_binding()),
            binding(
                15,
                wgpu::BindingResource::TextureView(&self.primary_transmission[old]),
            ),
        ];
        entries.extend(self.lod.entries());
        if let Some(reconstruction) = &self.water_reconstruction {
            entries.push(binding(
                16,
                wgpu::BindingResource::TextureView(&reconstruction.raw),
            ));
        }
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ray scene frame"),
            layout: &self.layout,
            entries: &entries,
        });
        #[cfg(test)]
        if self.primary_cost_probe(device, queue, encoder, &group, materials) {
            return;
        }
        if let Err(error) =
            self.encode_transport(device, queue, encoder, &group, materials, current, profile)
        {
            // The current attachments may be partially written. A later retry
            // clears them in its first tile and cannot reuse temporal history.
            self.history_valid = false;
            self.scheduling_error = Some(error);
            return;
        }
        if let Some(reconstruction) = &self.water_reconstruction {
            reconstruction.encode(
                device,
                encoder,
                &self.history_geometry[current],
                old,
                current,
            );
        }
        let compose_group = |radiance: &wgpu::TextureView| {
            let binding = |binding, resource| wgpu::BindGroupEntry { binding, resource };
            let mut entries = vec![
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
                binding(8, wgpu::BindingResource::TextureView(&self.baseline)),
                binding(
                    9,
                    wgpu::BindingResource::TextureView(&self.current_correction),
                ),
            ];
            if let Some(reconstruction) = &self.water_reconstruction
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
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ray composite"),
                layout: &self.composite_layout,
                entries: &entries,
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
        self.history_valid = true;
        self.previous = matrix;
        self.previous_eye = eye;
        self.previous_atmosphere = Some(atmosphere);
    }
}

#[cfg(test)]
mod paired_tests;
