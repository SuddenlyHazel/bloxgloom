//! Atmosphere uniforms shared by terrain, actors, fog, and the procedural sky.
use super::SUN_DIRECTION;
use glam::{Mat4, Vec3};

/// Compose calibrated lighting and fog without reserving shadow bindings.
/// Distant terrain shares this basis but owns a separate coverage bind group.
pub(super) fn surface_shader(source: &str) -> String {
    super::fog::shader(&format!(
        "{}\n{}\n{source}",
        include_str!("daylight.wgsl"),
        include_str!("scene_ao_output.wgsl")
    ))
}

/// Near opaque surfaces additionally sample the camera-local sun shadow map.
pub(super) fn shader(source: &str) -> String {
    surface_shader(&format!(
        "{}\n{}\n{}\n{source}",
        include_str!("local_shadow.wgsl"),
        super::sun_shadow::SHADER,
        include_str!("scene_contact.wgsl")
    ))
}

#[derive(Clone, Copy)]
pub(crate) struct Atmosphere {
    pub sun: Vec3,
    pub sun_color: Vec3,
    pub lighting: crate::config::lighting::Lighting,
    pub strength: f32,
    pub horizon: Vec3,
    pub zenith: Vec3,
    pub cloud: f32,
    pub fog: f32,
    pub fog_exposure: f32,
    pub drift: [f32; 2],
}

impl Atmosphere {
    pub(crate) fn at(time: u64) -> Self {
        let angle = crate::daylight::phase(time) * std::f32::consts::TAU;
        let midday = SUN_DIRECTION.normalize();
        let tangent = Vec3::new(midday.z, 0.0, -midday.x).normalize();
        let sun = (midday * angle.sin() + tangent * angle.cos()).normalize();
        let day = smooth(-0.12, 0.18, sun.y);
        let twilight = (1.0 - (sun.y / 0.22).abs()).max(0.0) * day;
        Self {
            sun,
            sun_color: Vec3::new(0.90, 0.30, 0.10)
                .lerp(Vec3::new(0.72, 0.67, 0.56), smooth(0.0, 0.40, sun.y)),
            lighting: Default::default(),
            cloud: 0.0,
            fog: 0.0,
            fog_exposure: 1.0,
            drift: [0.0; 2],
            strength: 0.035 + 0.965 * day,
            horizon: Vec3::new(0.012, 0.018, 0.045)
                .lerp(Vec3::new(0.59, 0.72, 0.82), day)
                .lerp(Vec3::new(0.80, 0.30, 0.14), twilight * 0.55),
            zenith: Vec3::new(0.002, 0.005, 0.020).lerp(Vec3::new(0.20, 0.45, 0.75), day),
        }
    }

    /// Linear radiance, shared by the solar disc, diffuse and specular response.
    pub(crate) fn sun_radiance(self) -> Vec3 {
        self.sun_color
            * self.strength
            * smooth(-0.02, 0.12, self.sun.y)
            * (1.0 - self.cloud).powi(2)
            * self.lighting.sun_intensity
            * 2.4
    }

    pub(crate) fn ambient(self) -> (Vec3, Vec3) {
        // A broad hemispherical convolution desaturates the sky. Calibrate its
        // clear-noon energy once; do not adapt exposure as the scene darkens.
        fn convolve(color: Vec3, reference: Vec3, energy: Vec3) -> Vec3 {
            let weights = Vec3::new(0.2126, 0.7152, 0.0722);
            let luminance = color.dot(weights) / reference.dot(weights);
            energy * Vec3::splat(luminance).lerp(color / reference, 0.35)
        }
        let horizon = Vec3::new(0.59, 0.72, 0.82);
        let zenith = Vec3::new(0.20, 0.45, 0.75);
        let lower = convolve(self.horizon, horizon, Vec3::new(0.36, 0.335, 0.30));
        let upper = convolve(
            self.zenith.lerp(self.horizon, 0.35),
            zenith.lerp(horizon, 0.35),
            Vec3::new(0.55, 0.57, 0.60),
        );
        (
            lower * self.lighting.ambient_intensity,
            upper * self.lighting.ambient_intensity,
        )
    }

    pub(crate) fn camera_data(self, matrix: Mat4, eye: Vec3) -> [f32; 52] {
        let mut data = [0.0; 52];
        data[..16].copy_from_slice(&matrix.to_cols_array());
        data[16..20].copy_from_slice(&[self.sun.x, self.sun.y, self.sun.z, self.strength]);
        data[20..23].copy_from_slice(&self.horizon.to_array());
        data[23] = super::fog::density(self.fog, 1.0);
        data[24..27].copy_from_slice(&eye.to_array());
        data[27] = self.fog_exposure;
        data[28] = 38.0;
        data[29] = 135.0;
        // Preserve the terrain parallax slot at byte 128 for every camera.
        data[32..36].copy_from_slice(&crate::config::parallax::Parallax::default().uniform());
        data[36..39].copy_from_slice(&self.sun_radiance().to_array());
        data[40..43].copy_from_slice(&self.zenith.to_array());
        data[43] = self.lighting.environment_intensity;
        let (lower, upper) = self.ambient();
        data[44..47].copy_from_slice(&lower.to_array());
        data[47] = (0.65 * self.lighting.local_directionality).clamp(0.0, 1.0);
        data[48..51].copy_from_slice(&upper.to_array());
        data
    }
}

fn smooth(low: f32, high: f32, value: f32) -> f32 {
    let t = ((value - low) / (high - low)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
#[path = "daylight/tests.rs"]
mod tests;
