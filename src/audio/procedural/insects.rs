//! Habitat-anchored insect ambience. A bounded presentation service, not entities.
mod cicadas;
mod crickets;
mod dsp;
use super::{
    dsp::{Rng, SAMPLE_RATE},
    spatial::{Bus, Listener, Spatial},
};
use crate::audio::{
    WeatherSound,
    rain_scene::{Habitat, RainScene},
};
const VOICES: usize = 4;
pub(super) fn rest(rng: &mut Rng, mean: f32) -> u32 {
    1 + (-mean * SAMPLE_RATE * (1.0 - rng.unit()).ln()) as u32
}
fn place(spatial: &mut Spatial, source: [f32; 3], eye: [f32; 3], yaw: f32) {
    let delta = std::array::from_fn::<_, 3, _>(|i| source[i] - eye[i]);
    let distance = delta.iter().map(|v| v * v).sum::<f32>().sqrt().max(0.25);
    spatial.retarget(
        distance,
        delta[2].atan2(delta[0]) - yaw,
        Listener::default(),
    );
}
fn sources(scene: &RainScene, habitat: Habitat, seed: u32) -> [Option<[f32; 3]>; VOICES] {
    // Stable world-coordinate ordering keeps individuals anchored while the
    // listener moves inside the patch. No random stream is consumed by geometry.
    let mut ranked = [(u64::MAX, None); VOICES];
    for tile in &scene.tiles {
        if tile.habitat != habitat || tile.normal != [0.0; 2] {
            continue;
        }
        let mut key = u64::from(seed);
        for v in tile.centre {
            key = key.wrapping_mul(0x9e3779b185ebca87) ^ u64::from(v.to_bits());
            key ^= key >> 29;
        }
        if let Some(index) = ranked.iter().position(|(old, _)| key < *old) {
            for i in (index + 1..VOICES).rev() {
                ranked[i] = ranked[i - 1];
            }
            ranked[index] = (key, Some(tile.centre));
        }
    }
    ranked.map(|(_, source)| source)
}
pub(super) struct Insects {
    crickets: crickets::Crickets,
    cicadas: cicadas::Cicadas,
    cricket_sources: [Option<[f32; 3]>; VOICES],
    cicada_sources: [Option<[f32; 3]>; VOICES],
    bus: Bus,
    seed: u32,
    gains: [f32; 2],
    targets: [f32; 2],
    allowed: [bool; 2],
    call_rate: f32,
}
impl Insects {
    pub fn new(seed: u32) -> Self {
        Self {
            crickets: crickets::Crickets::new(seed),
            cicadas: cicadas::Cicadas::new(seed),
            cricket_sources: [None; VOICES],
            cicada_sources: [None; VOICES],
            bus: Bus::default(),
            seed,
            gains: [0.0; 2],
            targets: [0.0; 2],
            allowed: [false; 2],
            call_rate: 1.0,
        }
    }
    pub fn set_scene(&mut self, scene: &RainScene) {
        self.cricket_sources = sources(scene, Habitat::Ground, self.seed);
        self.cicada_sources = sources(scene, Habitat::Canopy, self.seed ^ 0xa54ff53a);
    }
    pub fn follow(&mut self, weather: Option<WeatherSound>, eye: [f32; 3], yaw: f32) {
        let w = weather.unwrap_or_default();
        // Temperature is an explicit presentation profile until climate exists.
        let temperature = 18.0 + 9.0 * w.daylight;
        self.call_rate = (0.5 * (7.2 * temperature - 32.0) / 60.0).clamp(0.05, 10.0);
        self.allowed = [
            weather.is_some() && w.rain_mm_h <= 0.5 && w.wind_m_s <= 8.0 && w.daylight < 0.5,
            weather.is_some() && w.rain_mm_h <= 0.5 && temperature >= 22.0 && w.daylight > 0.5,
        ];
        self.targets = [
            if self.allowed[0] && self.cricket_sources.iter().any(Option::is_some) {
                0.15 * (1.0 - w.daylight)
            } else {
                0.0
            },
            if self.allowed[1] && self.cicada_sources.iter().any(Option::is_some) {
                0.12 * w.daylight
            } else {
                0.0
            },
        ];
        self.crickets.place(&self.cricket_sources, eye, yaw);
        self.cicadas.place(&self.cicada_sources, eye, yaw);
    }
    pub fn next(&mut self) -> ([f32; 2], f32) {
        for i in 0..2 {
            self.gains[i] += (self.targets[i] - self.gains[i]) / (0.5 * SAMPLE_RATE);
        }
        let send = self.crickets.next(
            &self.cricket_sources.map(|s| s.is_some()),
            self.allowed[0],
            self.call_rate,
            self.gains[0],
            &mut self.bus,
        ) + self.cicadas.next(
            &self.cicada_sources.map(|s| s.is_some()),
            self.allowed[1],
            self.gains[1],
            &mut self.bus,
        );
        (self.bus.next(), send)
    }
}
#[cfg(test)]
mod tests;
