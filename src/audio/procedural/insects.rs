//! Habitat-anchored insect ambience. A bounded presentation service, not entities.
mod cicadas;
mod crickets;
mod dsp;
mod placement;
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
fn place(spatial: &mut Spatial, source: [f32; 3], eye: [f32; 3], yaw: f32, listener: Listener) {
    let delta = std::array::from_fn::<_, 3, _>(|i| source[i] - eye[i]);
    let distance = delta.iter().map(|v| v * v).sum::<f32>().sqrt().max(0.25);
    spatial.retarget(distance, delta[2].atan2(delta[0]) - yaw, listener);
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
    config: crate::audio::rain_tuning::Advanced,
    enabled: [[bool; VOICES]; 2],
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
            config: Default::default(),
            enabled: [[false; VOICES]; 2],
        }
    }
    pub fn set_scene(&mut self, scene: &RainScene) {
        self.cricket_sources = sources(scene, Habitat::Ground, self.seed);
        self.cicada_sources = sources(scene, Habitat::Canopy, self.seed ^ 0xa54ff53a);
    }
    pub fn configure(&mut self, config: crate::audio::rain_tuning::Advanced) {
        if self.config.crickets != config.crickets {
            self.crickets.configure(config.crickets.tone);
        }
        if self.config.cicadas != config.cicadas {
            self.cicadas.configure(config.cicadas);
        }
        self.config = config;
    }
    pub fn follow(&mut self, weather: Option<WeatherSound>, eye: [f32; 3], yaw: f32) {
        let temperature = weather.map_or(18.0, |w| 18.0 + 9.0 * w.daylight);
        self.follow_weather(weather, temperature, eye, yaw, false);
    }
    pub fn preview(&mut self, weather: WeatherSound, temperature: f32, eye: [f32; 3], yaw: f32) {
        self.follow_weather(Some(weather), temperature, eye, yaw, true);
    }
    fn follow_weather(
        &mut self,
        weather: Option<WeatherSound>,
        temperature: f32,
        eye: [f32; 3],
        yaw: f32,
        preview: bool,
    ) {
        let w = weather.unwrap_or_default();
        let cricket = self.config.crickets;
        let cicada = self.config.cicadas;
        self.call_rate =
            (cricket.tone.call_rate_scale * (7.2 * temperature - 32.0) / 60.0).clamp(0.05, 10.0);
        let previous_allowed = self.allowed;
        self.allowed = [
            weather.is_some()
                && temperature >= cricket.tone.min_temperature_c
                && w.rain_mm_h <= cricket.tone.max_rain_mm_h
                && w.wind_m_s <= cricket.tone.max_wind_m_s
                && w.daylight < 0.5,
            weather.is_some()
                && temperature >= cicada.tone.min_temperature_c
                && w.rain_mm_h <= cicada.tone.max_rain_mm_h
                && w.daylight > 0.5,
        ];
        if preview && self.allowed[0] && !previous_allowed[0] {
            self.crickets.wake();
        }
        if preview && self.allowed[1] && !previous_allowed[1] {
            self.cicadas.wake();
        }
        let cricket_sources = if preview {
            placement::preview(self.seed, cricket.placement, eye)
        } else {
            self.cricket_sources
        };
        let cicada_sources = if preview {
            placement::preview(self.seed ^ 0xa54ff53a, cicada.placement, eye)
        } else {
            self.cicada_sources
        };
        self.enabled = [
            placement::audible(&cricket_sources, eye, cricket.placement),
            placement::audible(&cicada_sources, eye, cicada.placement),
        ];
        self.targets = [
            if self.allowed[0] && self.enabled[0].iter().any(|v| *v) {
                0.15 * (1.0 - w.daylight)
            } else {
                0.0
            },
            if self.allowed[1] && self.enabled[1].iter().any(|v| *v) {
                0.12 * w.daylight
            } else {
                0.0
            },
        ];
        let listener = self.config.listener.into();
        self.crickets.place(&cricket_sources, eye, yaw, listener);
        self.cicadas.place(&cicada_sources, eye, yaw, listener);
    }
    pub fn next(&mut self) -> ([f32; 2], f32) {
        for i in 0..2 {
            self.gains[i] += (self.targets[i] - self.gains[i]) / (0.5 * SAMPLE_RATE);
        }
        let send = self.crickets.next(
            &self.enabled[0],
            self.allowed[0],
            self.call_rate,
            self.gains[0],
            &mut self.bus,
        ) + self.cicadas.next(
            &self.enabled[1],
            self.allowed[1],
            self.gains[1],
            &mut self.bus,
        );
        (self.bus.next(), send)
    }
}
#[cfg(test)]
mod tests;
