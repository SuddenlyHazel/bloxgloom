//! World-space sky and true volume clouds. The quarter-resolution cloud pass
//! integrates extinction and solar shadowing before the full-resolution sky draw.
mod camera;
mod celestial;
mod clouds;
mod pipeline;
mod reference_celestial;
mod reference_clouds;
pub(crate) use reference_clouds::Noise as ReferenceNoise;

pub(crate) use camera::{
    sky_camera_data, sky_camera_data_at_sample, sky_camera_data_at_sample_in_medium,
};
pub(crate) const CLOUD_SHADER: &str = include_str!("sky/clouds.wgsl");
pub(crate) const STYLE_SHADER: &str = include_str!("sky/bsl.wgsl");

pub(crate) fn style_enabled() -> bool {
    super::bsl_reference::enabled()
        || std::env::var("BLOXGLOOM_BSL_STYLE").map_or(true, |v| v != "0")
}

/// Binding-free functions shared with ray-traced environment and cloud shadows.
pub(crate) fn environment_shader() -> String {
    format!(
        "{}\n{}\n{}",
        STYLE_SHADER,
        CLOUD_SHADER,
        include_str!("sky/environment.wgsl")
    )
}
pub(crate) fn shader_source() -> String {
    format!(
        "{}\n{}\n{}\n{}",
        environment_shader(),
        include_str!("sky/camera.wgsl"),
        reference_celestial::SHADER,
        include_str!("sky/sky.wgsl")
    )
}

pub(crate) struct SkyRenderer {
    pub(crate) pipeline: wgpu::RenderPipeline,
    pub(crate) camera: wgpu::Buffer,
    pub(crate) group: wgpu::BindGroup,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    clouds: clouds::CloudPass,
    celestial: celestial::Celestial,
    scene_transport: bool,
}
impl SkyRenderer {
    pub(crate) fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Self {
        let camera = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sky camera basis"),
            size: 160,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = pipeline::layout(device);
        let pipeline = pipeline::create(device, format, &layout);
        let clouds = clouds::CloudPass::new(device, &camera, width, height);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("volume clouds reconstruction"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let celestial = celestial::Celestial::new(device);
        let group = Self::bind(
            device,
            &layout,
            &camera,
            &clouds.view,
            &sampler,
            &celestial,
            super::bsl_reference::enabled(),
        );
        Self {
            pipeline,
            camera,
            group,
            layout,
            sampler,
            clouds,
            celestial,
            scene_transport: false,
        }
    }
    /// A ready scene integrator supplies clouds along the primary camera ray.
    /// The raster fallback otherwise keeps its own visible cloud transport.
    pub(crate) fn configure(&mut self, atmosphere: super::daylight::Atmosphere) {
        self.scene_transport = atmosphere.scene_transport;
    }
    fn bind(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        camera: &wgpu::Buffer,
        clouds: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
        celestial: &celestial::Celestial,
        reference: bool,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sky camera and volume clouds"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(clouds),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(celestial.sun(reference)),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(celestial.moon(reference)),
                },
            ],
        })
    }
    /// Encode once per submitted camera sample, after writing the camera uniform.
    pub(crate) fn prepare(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        width: u32,
        height: u32,
    ) {
        self.prepare_timed(device, encoder, width, height, None);
    }
    pub(crate) fn prepare_timed(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        width: u32,
        height: u32,
        timestamp_writes: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) {
        if self.clouds.resize(device, width, height) {
            self.group = Self::bind(
                device,
                &self.layout,
                &self.camera,
                &self.clouds.view,
                &self.sampler,
                &self.celestial,
                super::bsl_reference::enabled(),
            );
        }
        self.celestial.upload(encoder);
        self.clouds
            .encode(encoder, timestamp_writes, !self.scene_transport);
    }
}
#[cfg(test)]
#[path = "sky/tests.rs"]
mod tests;
