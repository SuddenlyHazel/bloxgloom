//! Bounded weather presentation; simulation and shelter queries stay outside rendering.
use super::{Camera, daylight::Atmosphere};
use glam::Vec3;
pub(crate) const MAX_STREAKS: usize = 512;
#[derive(Clone, Copy)]
pub(crate) struct Presentation {
    cloud: f32,
    rain: f32,
    wind: [f32; 2],
    exposure: f32,
    seconds: f32,
    flash: f32,
    cover: Option<([i32; 2], [f32; 256])>,
}
impl Default for Presentation {
    fn default() -> Self {
        Self::new(0.0, 0.0, [0.0; 2], 1.0, 0.0, 0.0)
    }
}
impl Presentation {
    pub(crate) fn new(
        cloud: f32,
        rain: f32,
        wind: [f32; 2],
        exposure: f32,
        seconds: f32,
        flash: f32,
    ) -> Self {
        fn finite(v: f32, low: f32, high: f32) -> f32 {
            if v.is_finite() {
                v.clamp(low, high)
            } else {
                low
            }
        }
        Self {
            cloud: finite(cloud, 0.0, 1.0),
            rain: finite(rain, 0.0, 1.0),
            wind: wind.map(|v| finite(v, -18.0, 18.0)),
            exposure: finite(exposure, 0.0, 1.0),
            seconds: finite(seconds, 0.0, f32::MAX),
            flash: finite(flash, 0.0, 1.0),
            cover: None,
        }
    }
    pub(crate) fn set_cover(&mut self, origin: [i32; 2], heights: [f32; 256]) {
        self.cover = Some((origin, heights));
    }
    pub(crate) fn atmosphere(self, mut a: Atmosphere) -> Atmosphere {
        a.cloud = self.cloud;
        a.drift = self
            .wind
            .map(|v| v * self.seconds.rem_euclid(3600.0) * 0.02);
        let gray = Vec3::new(0.19, 0.23, 0.28) * (0.12 + a.strength * 0.88);
        a.horizon = a.horizon.lerp(gray, self.cloud * 0.72);
        a.zenith = a.zenith.lerp(gray * 0.72, self.cloud * 0.85);
        // Flash changes skylight only. Sealed caves have zero sky visibility.
        a.strength = (a.strength * (1.0 - self.cloud * 0.38)).max(self.flash);
        a.horizon += Vec3::new(0.65, 0.72, 0.85) * self.flash;
        a.zenith += Vec3::new(0.72, 0.8, 1.0) * self.flash;
        a
    }
    pub(crate) fn vertices(self, camera: Camera) -> Vec<f32> {
        let visibility = if self.cover.is_some() {
            1.0
        } else {
            self.exposure
        };
        let intensity = self.rain * visibility;
        if intensity <= 0.001 {
            return Vec::new();
        }
        let count = (MAX_STREAKS as f32 * intensity).ceil() as usize;
        let mut out = Vec::with_capacity(count * 27);
        let right = Vec3::new(-camera.yaw.sin(), 0.0, camera.yaw.cos()) * 0.012;
        let time = self.seconds.rem_euclid(1024.0);
        for i in 0..count.min(MAX_STREAKS) {
            let seed = i as u32;
            let x = random(seed.wrapping_mul(3)) * 16.0 - 8.0;
            let z = random(seed.wrapping_mul(3).wrapping_add(1)) * 16.0 - 8.0;
            let y = (random(seed.wrapping_mul(3).wrapping_add(2)) * 12.0 - time * 13.0)
                .rem_euclid(12.0)
                - 3.0;
            let top = camera.position + Vec3::new(x, y, z);
            let mut bottom = top + Vec3::new(self.wind[0] * 0.014, -0.55, self.wind[1] * 0.014);
            if let Some((origin, heights)) = &self.cover {
                let cx = top.x.floor() as i32 - origin[0];
                let cz = top.z.floor() as i32 - origin[1];
                if !(0..16).contains(&cx) || !(0..16).contains(&cz) {
                    continue;
                }
                let roof = heights[cz as usize * 16 + cx as usize];
                if roof.is_nan() || top.y <= roof {
                    continue;
                }
                bottom.y = bottom.y.max(roof);
            }
            let alpha = 0.34 * (1.0 - (x * x + z * z).sqrt() / 12.0).clamp(0.0, 1.0);
            for (point, v) in [(top - right, 0.0), (bottom, 0.8), (top + right, 0.0)] {
                out.extend_from_slice(&[
                    point.x, point.y, point.z, 0.5, v, 0.55, 0.68, 0.82, alpha,
                ]);
            }
        }
        out
    }
}
fn random(seed: u32) -> f32 {
    let mut v = seed.wrapping_add(0x9e3779b9);
    v = (v ^ (v >> 16)).wrapping_mul(0x7feb352d);
    v = (v ^ (v >> 15)).wrapping_mul(0x846ca68b);
    (v ^ (v >> 16)) as f32 / u32::MAX as f32
}
#[cfg(test)]
#[path = "weather/tests.rs"]
mod tests;
