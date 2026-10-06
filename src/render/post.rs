//! Linear HDR scene, quarter-resolution bloom, and fixed-exposure display mapping.
//! Shared by the window renderer, image previews, and GPU benchmark.
use wgpu::util::DeviceExt;

mod reference_ao;
mod reference_bloom;
mod reference_display;
mod reference_light_shafts;
mod reference_underwater;
mod targets;
pub(crate) mod temporal;

#[cfg(test)]
mod tests;

pub(crate) const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// Temporal stability is the normal rendering path. Explicit zero preserves
/// single-frame comparisons and callers can independently opt out (benchmarks).
pub(crate) fn temporal_requested() -> bool {
    std::env::var("BLOXGLOOM_TAA").map_or(true, |value| value.trim() != "0")
}

pub(crate) struct PostProcess {
    pub scene: wgpu::TextureView,
    temporal: Option<temporal::Temporal>,
    reduced_resolution: bool,
    pub(crate) ambient: super::scene_ao::AmbientOcclusion,
    pub(crate) reflections: super::reflections::Reflections,
    pub(crate) trace: super::trace::TraceLighting,
    atmosphere: super::atmosphere::AtmospherePass,
    reference_ao: Option<reference_ao::ReferenceAo>,
    reference_bloom: Option<reference_bloom::ReferenceBloom>,
    reference_display: Option<reference_display::ReferenceDisplay>,
    reference_underwater: Option<reference_underwater::Underwater>,
    reference_light_shafts: Option<reference_light_shafts::LightShafts>,
    bloom: [wgpu::TextureView; 2],
    groups: [wgpu::BindGroup; 3],
    composite_group: wgpu::BindGroup,
    extract: wgpu::RenderPipeline,
    horizontal: wgpu::RenderPipeline,
    vertical: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    settings: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    output_srgb: bool,
    exposure: f32,
    enabled: bool,
    bloom_strength: f32,
    effect: Option<super::effects::Effect>,
}

impl PostProcess {
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        output: wgpu::TextureFormat,
    ) -> Self {
        Self::new_with_reference(
            device,
            width,
            height,
            output,
            super::bsl_reference::enabled(),
        )
    }

    fn new_with_reference(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        output: wgpu::TextureFormat,
        reference: bool,
    ) -> Self {
        let texture_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post processing inputs"),
            entries: &[
                texture_entry(0),
                texture_entry(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("post linear clamp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let settings = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("exposure and bloom"),
            contents: bytemuck::cast_slice(&[
                1.0f32,
                0.12,
                if output.is_srgb() { 0.0 } else { 1.0 },
                1.0,
            ]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let targets =
            targets::Targets::new(device, width, height, &layout, &sampler, &settings, None);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("HDR and bloom shader"),
            source: wgpu::ShaderSource::Wgsl(
                format!(
                    "{}\nconst BG_BSL_STYLE: bool = {};\nconst BG_REFERENCE_BLOOM: bool = {};\nconst BG_REFERENCE_DISPLAY: bool = {};\n{}\n{}\n{}",
                    super::sky::STYLE_SHADER,
                    super::sky::style_enabled(),
                    reference,
                    reference,
                    reference_bloom::COMMON,
                    include_str!("post.wgsl"),
                    reference_bloom::COMPOSITE
                )
                .into(),
            ),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("post pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry, format| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
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
        };
        let reference_bloom = reference.then(|| {
            reference_bloom::ReferenceBloom::new(device, &targets.scene, &layout, &settings)
        });
        let reference_display = reference
            .then(|| reference_display::ReferenceDisplay::new(device, width, height, output));
        Self {
            reference_ao: reference.then(|| reference_ao::ReferenceAo::new(device, width, height)),
            reference_bloom,
            reference_display,
            reference_light_shafts: reference
                .then(|| reference_light_shafts::LightShafts::new(device, width, height)),
            reference_underwater: reference
                .then(|| reference_underwater::Underwater::new(device, width, height)),
            scene: targets.scene,
            temporal: None,
            reduced_resolution: false,
            ambient: super::scene_ao::AmbientOcclusion::new(device, width, height),
            reflections: super::reflections::Reflections::new(device, width, height),
            trace: super::trace::TraceLighting::new(device),
            atmosphere: super::atmosphere::AtmospherePass::new(device, width, height),
            bloom: targets.bloom,
            groups: targets.groups,
            extract: pipeline("extract", HDR_FORMAT),
            horizontal: pipeline("horizontal", HDR_FORMAT),
            vertical: pipeline("vertical", HDR_FORMAT),
            composite: pipeline("composite", if reference { HDR_FORMAT } else { output }),
            settings,
            layout,
            sampler,
            output_srgb: output.is_srgb(),
            exposure: 1.0,
            enabled: true,
            bloom_strength: 0.12,
            composite_group: targets.composite_group,
            effect: None,
        }
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let targets = targets::Targets::new(
            device,
            width,
            height,
            &self.layout,
            &self.sampler,
            &self.settings,
            self.effect.as_mut(),
        );
        if self.temporal.is_some() {
            let mut temporal = temporal::Temporal::new(device, width, height);
            temporal.reduced_resolution(self.reduced_resolution);
            self.temporal = Some(temporal);
        }
        if let Some(display) = &mut self.reference_display {
            display.resize(device, width, height);
        }
        if let Some(underwater) = &mut self.reference_underwater {
            underwater.resize(device, width, height);
        }
        if let Some(shafts) = &mut self.reference_light_shafts {
            shafts.resize(device, width, height);
        }
        self.ambient.resize(device, width, height);
        if let Some(ao) = &mut self.reference_ao {
            ao.resize(device, width, height);
        }
        self.reflections.resize(device, width, height);
        self.trace.resize(device, targets.scene.texture().size());
        self.atmosphere.resize(device, width, height);
        if self.reference_bloom.is_some() {
            let input = self
                .effect
                .as_ref()
                .map_or(&targets.scene, |effect| effect.output());
            self.reference_bloom = Some(reference_bloom::ReferenceBloom::new(
                device,
                input,
                &self.layout,
                &self.settings,
            ));
        }
        self.scene = targets.scene;
        self.bloom = targets.bloom;
        self.groups = targets.groups;
        self.composite_group = targets.composite_group;
    }

    /// Callers may disable temporal accumulation for single-frame captures.
    pub(crate) fn enable_temporal(&mut self, device: &wgpu::Device, enabled: bool) {
        let enabled = if enabled && !temporal::supported(device) {
            eprintln!("temporal AA unavailable on GL backend; keeping single-sample rendering");
            false
        } else {
            enabled
        };
        if enabled == self.temporal.is_some() {
            return;
        }
        let size = self.scene.texture().size();
        self.temporal = enabled.then(|| temporal::Temporal::new(device, size.width, size.height));
        if let Some(temporal) = &mut self.temporal {
            temporal.reduced_resolution(self.reduced_resolution);
        }
        if let Some(display) = &mut self.reference_display {
            display.reset();
        }
    }

    pub(crate) fn temporal_enabled(&self) -> bool {
        self.temporal.is_some()
    }

    /// Preserve source-reference AA; enhanced low-resolution output uses a
    /// stable sample grid and spatial edge AA for surfaces that reject history.
    pub(crate) fn configure_reduced_resolution(&mut self, reduced: bool) {
        self.reduced_resolution = reduced
            && self.reference_display.is_none()
            && std::env::var("BLOXGLOOM_LOW_RES_AA").as_deref() != Ok("0");
        if let Some(temporal) = &mut self.temporal {
            temporal.reduced_resolution(self.reduced_resolution);
        }
    }

    pub(crate) fn prepare_temporal(
        &mut self,
        queue: &wgpu::Queue,
        camera: super::Camera,
    ) -> (glam::Mat4, glam::Vec2) {
        let size = self.scene.texture().size();
        self.temporal.as_mut().map_or_else(
            || {
                (
                    super::view_projection(camera, size.width, size.height),
                    glam::Vec2::ZERO,
                )
            },
            |temporal| temporal.prepare(queue, camera, size.width, size.height),
        )
    }

    pub(crate) fn resolve_ambient(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        depth: &wgpu::TextureView,
        matrix: glam::Mat4,
    ) {
        if !self.trace.ready() {
            if let Some(ao) = &mut self.reference_ao {
                ao.resolve(
                    device,
                    queue,
                    encoder,
                    &self.scene,
                    depth,
                    self.temporal.is_some(),
                );
            } else {
                self.ambient
                    .resolve(device, queue, encoder, &self.scene, depth, matrix);
            }
            self.reflections
                .capture_opaque(device, encoder, &self.scene, depth);
        }
    }

    pub(crate) fn resolve_reflections(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        depth: &wgpu::TextureView,
        matrix: glam::Mat4,
    ) {
        self.reflections.resolve(
            device,
            queue,
            encoder,
            &self.scene,
            &self.ambient.indirect,
            depth,
            matrix,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn resolve_transport(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        depth: &wgpu::TextureView,
        matrix: glam::Mat4,
        eye: glam::Vec3,
        atmosphere: super::daylight::Atmosphere,
        pipeline: &wgpu::RenderPipeline,
        materials: &wgpu::BindGroup,
    ) {
        if self.trace.ready() {
            self.trace.resolve(
                device,
                queue,
                encoder,
                &self.scene,
                depth,
                &self.reflections.normal,
                &self.reflections.response,
                &self.ambient.indirect,
                pipeline,
                materials,
                matrix,
                eye,
                atmosphere,
            );
        } else {
            self.resolve_reflections(device, queue, encoder, depth, matrix);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn resolve_atmosphere(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        depth: &wgpu::TextureView,
        matrix: glam::Mat4,
        eye: glam::Vec3,
        atmosphere: super::daylight::Atmosphere,
        shadows: &super::sun_shadow::SunShadows,
    ) {
        if let Some(shafts) = &mut self.reference_light_shafts {
            shafts.capture(depth, matrix, eye, atmosphere, shadows);
            return;
        }
        if self.trace.ready() {
            return;
        }
        self.atmosphere.resolve(
            device,
            queue,
            encoder,
            &self.scene,
            depth,
            &self.reflections.normal,
            matrix,
            eye,
            atmosphere,
            shadows,
        );
    }

    pub(crate) fn draw_motion(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        depth: &wgpu::TextureView,
        avatars: Option<&super::avatars::AvatarRenderer>,
    ) {
        let Some(temporal) = &self.temporal else {
            return;
        };
        let Some(frame) = &temporal.motion_frame else {
            return;
        };
        if let Some(avatars) = avatars {
            avatars.prepare_motion(queue, frame);
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("object temporal motion"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &temporal.motion,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: None,
                stencil_ops: None,
            }),
            ..Default::default()
        });
        if let Some(avatars) = avatars {
            avatars.draw_motion(&mut pass);
        }
    }

    pub(crate) fn submitted(&mut self) {
        if let Some(shafts) = &mut self.reference_light_shafts {
            shafts.submitted();
        }
        if let Some(display) = &mut self.reference_display {
            display.submitted();
        }
        if let Some(temporal) = &mut self.temporal {
            temporal.submitted();
        }
    }

    pub(crate) fn resolve_temporal(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        depth: &wgpu::TextureView,
    ) {
        if let Some(display) = &mut self.reference_display {
            display.depth = Some(depth.clone());
            return;
        }
        if let Some(temporal) = &mut self.temporal {
            temporal.resolve(
                device,
                encoder,
                &self.scene,
                depth,
                Some(&self.ambient.indirect),
            );
        }
    }

    pub(crate) fn install_effect(
        &mut self,
        device: &wgpu::Device,
        prepared: &super::effects::Prepared,
    ) -> Result<(), String> {
        let effect = super::effects::Effect::prepare(device, prepared)?;
        self.install_prepared_effect(device, effect);
        Ok(())
    }

    pub(super) fn install_prepared_effect(
        &mut self,
        device: &wgpu::Device,
        effect: super::effects::Effect,
    ) {
        self.effect = Some(effect);
        let size = self.scene.texture().size();
        self.resize(device, size.width, size.height);
    }

    pub(crate) fn set_parameter(
        &mut self,
        update: &super::parameters::Update,
    ) -> Result<bool, String> {
        self.effect.as_mut().map_or(Ok(false), |effect| {
            effect.set(&update.resource, &update.name, &update.value)
        })
    }

    pub(crate) fn configure_reference_water_depth(&mut self, depth: Option<&wgpu::TextureView>) {
        if let Some(underwater) = &mut self.reference_underwater {
            underwater.depth = depth.cloned();
        }
        if let Some(display) = &mut self.reference_display {
            display.front_depth = depth.cloned();
        }
    }

    pub(crate) fn configure_reference_ao(&mut self, data: [f32; 80], fov: f32) {
        if let Some(ao) = &mut self.reference_ao {
            ao.configure(data, fov);
        }
    }

    pub(crate) fn configure_reference_lens(
        &mut self,
        matrix: glam::Mat4,
        eye: glam::Vec3,
        atmosphere: super::daylight::Atmosphere,
        frame_time: f32,
        eye_in_water: bool,
    ) {
        if let Some(display) = &mut self.reference_display {
            display.configure_lens(matrix, eye, atmosphere, frame_time, eye_in_water);
        }
        if let Some(underwater) = &mut self.reference_underwater {
            underwater.configure(matrix, eye, atmosphere, eye_in_water);
        }
        if let Some(shafts) = &mut self.reference_light_shafts {
            shafts.medium(eye_in_water);
        }
    }

    pub fn configure(
        &mut self,
        queue: &wgpu::Queue,
        enabled: bool,
        exposure: f32,
        bloom_strength: f32,
    ) {
        if self.enabled == enabled
            && self.exposure == exposure
            && self.bloom_strength == bloom_strength
        {
            return;
        }
        if self.reference_display.is_some()
            && (self.enabled != enabled
                || self.exposure != exposure
                || self.bloom_strength != bloom_strength)
        {
            if let Some(display) = &mut self.reference_display {
                display.reset();
            }
            if let Some(temporal) = &mut self.temporal {
                temporal.reference_reset();
            }
        }
        self.exposure = exposure;
        self.enabled = enabled;
        self.bloom_strength = bloom_strength;
        queue.write_buffer(
            &self.settings,
            0,
            bytemuck::cast_slice(&[
                exposure,
                bloom_strength,
                if self.output_srgb { 0.0 } else { 1.0 },
                if enabled { 1.0 } else { 0.0 },
            ]),
        );
    }

    pub fn encode(
        &mut self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        output: &wgpu::TextureView,
    ) {
        if let Some(effect) = &self.effect {
            effect.encode(queue, encoder);
        }
        let post_scene = self
            .effect
            .as_ref()
            .map_or(&self.scene, |effect| effect.output());
        if let Some(underwater) = &self.reference_underwater {
            underwater.resolve(_device, queue, encoder, post_scene, self.enabled);
        }
        if let Some(shafts) = &mut self.reference_light_shafts {
            shafts.encode(
                _device,
                queue,
                encoder,
                post_scene,
                self.reference_underwater
                    .as_ref()
                    .and_then(|water| water.depth.as_ref()),
                &self.ambient.indirect,
                self.enabled,
            );
        }
        if self.enabled
            && self.bloom_strength > 0.0
            && let Some(reference) = &self.reference_bloom
        {
            reference.encode(encoder);
        }
        let mut draw = |label,
                        pipeline: &wgpu::RenderPipeline,
                        group: &wgpu::BindGroup,
                        target: &wgpu::TextureView| {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(label),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.draw(0..3, 0..1);
        };
        if self.enabled && self.bloom_strength > 0.0 && self.reference_bloom.is_none() {
            draw(
                "bloom extract",
                &self.extract,
                &self.groups[0],
                &self.bloom[0],
            );
            draw(
                "bloom horizontal",
                &self.horizontal,
                &self.groups[1],
                &self.bloom[1],
            );
            draw(
                "bloom vertical",
                &self.vertical,
                &self.groups[2],
                &self.bloom[0],
            );
        }
        let distorted = self.enabled
            && self
                .reference_underwater
                .as_ref()
                .is_some_and(|underwater| {
                    underwater.composite(
                        _device,
                        encoder,
                        post_scene,
                        &self.reference_bloom.as_ref().unwrap().atlas,
                        &self.settings,
                        &self.reference_display.as_ref().unwrap().linear,
                    )
                });
        if !distorted {
            reference_display::draw(
                encoder,
                &self.composite,
                self.reference_bloom
                    .as_ref()
                    .map_or(&self.composite_group, |reference| {
                        &reference.composite_group
                    }),
                &[self
                    .reference_display
                    .as_ref()
                    .map_or(output, |display| &display.linear)],
            );
        }
        if let Some(display) = &mut self.reference_display {
            display.encode(
                _device,
                queue,
                encoder,
                output,
                self.temporal.as_mut(),
                &self.ambient.indirect,
                self.enabled,
            );
        }
    }
}
