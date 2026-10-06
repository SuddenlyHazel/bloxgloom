//! Checked-in default depth AO, applied to full surface radiance before fog.
//! No enhanced AO settings or material/indirect masks enter this comparison.
use glam::{Mat4, Vec3};
mod gpu;
#[cfg(test)]
mod tests;
const SHADER: &str = include_str!("reference_ao/ao.wgsl");
pub(super) fn shader() -> String {
    // Pure shared fog equations; reference constants are explicit for fixtures.
    format!(
        "{}\nconst BG_FOG_BSL_STYLE:bool=true;\nconst BG_FOG_REFERENCE:bool=true;\nconst BG_FOG_NOON_HEIGHT:f32=1.0;\n{}\n{}\n{SHADER}",
        crate::render::sky::STYLE_SHADER,
        include_str!("../fog/reference.wgsl"),
        include_str!("../fog.wgsl")
    )
}
pub(super) struct ReferenceAo {
    gpu: gpu::Gpu,
    visibility: wgpu::TextureView,
    camera: Option<([f32; 80], f32)>,
    sample: u32,
}
impl ReferenceAo {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        Self {
            gpu: gpu::Gpu::new(device),
            visibility: gpu::target(device, width, height),
            camera: None,
            sample: 0,
        }
    }
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.visibility = gpu::target(device, width, height);
        self.camera = None;
        self.sample = 0;
    }
    pub fn configure(&mut self, data: [f32; 80], fov: f32) {
        self.camera = Some((data, fov));
    }
    pub fn resolve(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        temporal: bool,
    ) {
        let Some((camera, fov)) = self.camera.take() else {
            return;
        };
        // Missing restricted noise is explicit; never silently substitute the
        // enhanced AO model, deterministic random pixels or an embedded asset.
        if !self.gpu.noise.available {
            return;
        }
        let matrix = Mat4::from_cols_array(camera[..16].try_into().unwrap());
        let eye = Vec3::from_slice(&camera[24..27]);
        let inverse = (matrix * Mat4::from_translation(eye)).inverse();
        let forward = matrix.transpose().w_axis.truncate().normalize();
        let size = scene.texture().size();
        let mut settings = [0.0f32; 24];
        settings[..16].copy_from_slice(&inverse.to_cols_array());
        settings[16..20].copy_from_slice(&[
            size.width as f32,
            size.height as f32,
            1.0 / (fov * 0.5).tan() / 1.37,
            self.sample as f32,
        ]);
        settings[20..23].copy_from_slice(&forward.to_array());
        settings[23] = f32::from(temporal);
        queue.write_buffer(&self.gpu.options, 0, bytemuck::cast_slice(&settings));
        queue.write_buffer(&self.gpu.camera, 0, bytemuck::cast_slice(&camera));
        self.gpu.noise.upload(encoder);
        self.gpu
            .draw(device, encoder, scene, depth, &self.visibility);
        self.sample = self.sample.wrapping_add(1);
    }
}
