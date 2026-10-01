//! Atmosphere uniforms shared by terrain, actors, fog, and the procedural sky.
use super::SUN_DIRECTION;
use glam::{Mat4, Vec3};

/// Compose the same calibrated lighting function into each opaque surface shader.
pub(super) fn shader(source: &str) -> String {
    super::fog::shader(&format!("{}\n{source}", include_str!("daylight.wgsl")))
}

#[derive(Clone, Copy)]
pub(crate) struct Atmosphere {
    pub sun: Vec3,
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

    pub(crate) fn camera_data(self, matrix: Mat4, eye: Vec3) -> [f32; 28] {
        let mut data = [0.0; 28];
        data[..16].copy_from_slice(&matrix.to_cols_array());
        data[16..20].copy_from_slice(&[self.sun.x, self.sun.y, self.sun.z, self.strength]);
        data[20..23].copy_from_slice(&self.horizon.to_array());
        data[23] = super::fog::density(self.fog, 1.0);
        data[24..27].copy_from_slice(&eye.to_array());
        data[27] = self.fog_exposure;
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
