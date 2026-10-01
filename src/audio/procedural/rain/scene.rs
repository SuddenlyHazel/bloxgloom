//! Spatial importance sampling of bounded voxel faces. Runs on the audio worker.
use super::RainWeather;
use crate::audio::rain_scene::{MAX_RAIN_TILES, RainScene, RainTile};
use std::sync::Arc;

pub(super) struct SceneSampler {
    pub scene: Option<Arc<RainScene>>,
    pub position: [f32; 3],
    pub yaw: f32,
    cdf: Vec<f32>,
    pub area: f32,
}
impl Default for SceneSampler {
    fn default() -> Self {
        Self {
            scene: None,
            position: [0.0; 3],
            yaw: 0.0,
            cdf: Vec::with_capacity(MAX_RAIN_TILES),
            area: 0.0,
        }
    }
}
impl SceneSampler {
    pub fn rebuild(&mut self, weather: RainWeather, wall_rate: f32) {
        self.cdf.clear();
        self.area = 0.0;
        let Some(scene) = &self.scene else {
            return;
        };
        let wind = [
            (weather.wind_bearing_rad + self.yaw).cos(),
            (weather.wind_bearing_rad + self.yaw).sin(),
        ];
        let mut total = 0.0;
        for tile in &scene.tiles {
            let distance_sq = tile
                .centre
                .iter()
                .zip(self.position)
                .map(|(p, eye)| (p - eye).powi(2))
                .sum::<f32>();
            let area = if distance_sq > 32.0 * 32.0 {
                0.0
            } else if tile.normal == [0.0; 2] {
                1.0
            } else {
                wall_rate * (-tile.normal[0] * wind[0] - tile.normal[1] * wind[1]).max(0.0)
            };
            self.area += area;
            // Nearby faces dominate audible impacts, while distant faces remain
            // represented. Each chosen impact still uses physical attenuation.
            total += area / distance_sq.max(1.0);
            self.cdf.push(total);
        }
        if total > 0.0 {
            for value in &mut self.cdf {
                *value /= total;
            }
        }
    }
    pub fn choose(&self, choice: f32) -> Option<RainTile> {
        let scene = self.scene.as_ref()?;
        if self.area <= 0.0 {
            return None;
        }
        scene
            .tiles
            .get(self.cdf.partition_point(|v| *v <= choice))
            .copied()
    }
    pub fn polar(&self, tile: RainTile, u: f32, v: f32) -> (f32, f32) {
        let mut p = tile.centre;
        if tile.normal == [0.0; 2] {
            p[0] += u - 0.5;
            p[2] += v - 0.5;
        } else {
            p[1] += u - 0.5;
            if tile.normal[0] != 0.0 {
                p[2] += v - 0.5;
            } else {
                p[0] += v - 0.5;
            }
        }
        let delta = std::array::from_fn::<_, 3, _>(|i| p[i] - self.position[i]);
        let distance = delta
            .iter()
            .map(|v| v * v)
            .sum::<f32>()
            .sqrt()
            .clamp(0.25, 100.0);
        (
            distance,
            (delta[2].atan2(delta[0]) - self.yaw).rem_euclid(std::f32::consts::TAU),
        )
    }
}
#[cfg(test)]
mod tests;
