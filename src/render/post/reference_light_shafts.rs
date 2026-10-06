//! Source-default camera shafts, deferred until underwater absorption is done.
//! Engine map/eye-light inputs remain explicit runtime proxies; enhanced volume
//! transport is independent and never modified by this optional comparison.
mod gpu;
#[cfg(test)]
mod tests;
struct Input {
    opaque: wgpu::TextureView,
    shadow: wgpu::TextureView,
    data: [f32; 56],
    enabled: bool,
}
pub(super) struct LightShafts {
    gpu: gpu::Gpu,
    encoded: wgpu::TextureView,
    scratch: wgpu::TextureView,
    input: Option<Input>,
    water: bool,
    frame: u32,
}
impl LightShafts {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        Self {
            gpu: gpu::Gpu::new(device),
            encoded: gpu::target(device, width, height, wgpu::TextureFormat::Rgba8Unorm),
            scratch: gpu::target(device, width, height, super::HDR_FORMAT),
            input: None,
            water: false,
            frame: 0,
        }
    }
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.encoded = gpu::target(device, width, height, wgpu::TextureFormat::Rgba8Unorm);
        self.scratch = gpu::target(device, width, height, super::HDR_FORMAT);
        self.input = None;
    }
    pub fn medium(&mut self, water: bool) {
        self.water = water;
    }
    pub fn submitted(&mut self) {
        self.frame = self.frame.wrapping_add(1);
        self.input = None;
    }
    pub fn capture(
        &mut self,
        depth: &wgpu::TextureView,
        matrix: glam::Mat4,
        eye: glam::Vec3,
        atmosphere: crate::render::daylight::Atmosphere,
        shadows: &crate::render::sun_shadow::SunShadows,
    ) {
        let mut data = [0.0_f32; 56];
        data[..16].copy_from_slice(&matrix.inverse().to_cols_array());
        data[16..32].copy_from_slice(&shadows.projection.matrix.to_cols_array());
        data[32..35].copy_from_slice(&eye.to_array());
        data[35] = atmosphere.fog_exposure.clamp(0.0, 1.0);
        data[36..39].copy_from_slice(&matrix.transpose().w_axis.truncate().normalize().to_array());
        let direction = atmosphere.sun
            * if (0.5325..=0.9675).contains(&atmosphere.time_angle) {
                -1.0
            } else {
                1.0
            };
        data[40..43].copy_from_slice(&direction.to_array());
        data[43] = atmosphere.time_brightness();
        data[44..47].copy_from_slice(
            &crate::render::bsl_reference::palettes(atmosphere)
                .0
                .to_array(),
        );
        data[47] = atmosphere.reference_shadow_fade;
        data[48] = atmosphere.rain_strength;
        data[49] = (atmosphere.sun.y * 10.0 + 0.5).clamp(0.0, 1.0);
        data[51] = 1.0 / shadows.projection.settings.resolution as f32;
        data[53] = 0.9;
        self.input = Some(Input {
            opaque: depth.clone(),
            shadow: shadows.view.clone(),
            data,
            enabled: shadows.projection.enabled,
        });
    }
    #[allow(clippy::too_many_arguments)]
    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
        front: Option<&wgpu::TextureView>,
        metadata: &wgpu::TextureView,
        enabled: bool,
    ) {
        let Some(input) = &mut self.input else {
            return;
        };
        if !enabled || !input.enabled || !self.gpu.noise.available {
            return;
        }
        if self.water && front.is_none() {
            return;
        }
        let front = front.unwrap_or(&input.opaque);
        input.data[50] = f32::from(self.water);
        input.data[52] = self.frame as f32;
        queue.write_buffer(&self.gpu.uniform, 0, bytemuck::cast_slice(&input.data));
        self.gpu.noise.upload(encoder);
        let group = |source, encoded| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("source shaft inputs"),
                layout: &self.gpu.layout,
                entries: &[
                    super::reference_display::texture_entry(0, source),
                    super::reference_display::texture_entry(1, front),
                    super::reference_display::texture_entry(2, &input.opaque),
                    super::reference_display::texture_entry(3, metadata),
                    super::reference_display::texture_entry(4, &self.gpu.noise.view),
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::Sampler(&self.gpu.noise.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: self.gpu.uniform.as_entire_binding(),
                    },
                    super::reference_display::texture_entry(7, &input.shadow),
                    wgpu::BindGroupEntry {
                        binding: 8,
                        resource: wgpu::BindingResource::Sampler(&self.gpu.comparison),
                    },
                    super::reference_display::texture_entry(9, encoded),
                    wgpu::BindGroupEntry {
                        binding: 10,
                        resource: wgpu::BindingResource::Sampler(&self.gpu.sampler),
                    },
                ],
            })
        };
        super::reference_display::draw(
            encoder,
            &self.gpu.integrate,
            &group(scene, scene),
            &[&self.encoded],
        );
        super::reference_display::draw(
            encoder,
            &self.gpu.reconstruct,
            &group(scene, &self.encoded),
            &[&self.scratch],
        );
        super::reference_display::draw(
            encoder,
            &self.gpu.copy,
            &group(&self.scratch, &self.encoded),
            &[scene],
        );
    }
}
fn shader() -> String {
    format!(
        "{}\n{}\n{}",
        crate::render::sky::STYLE_SHADER,
        include_str!("../sun_shadow/reference/equations.wgsl"),
        include_str!("reference_light_shafts/shafts.wgsl")
    )
}
