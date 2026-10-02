//! Persistent local rain profiles and companion ambient gains. Never affects world weather.
use crate::audio::rain_scene::RAIN_MATERIALS;
mod advanced;
pub use advanced::*;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SurfaceMode {
    pub frequency_hz: f32,
    pub damping_per_s: f32,
    pub gain: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Surface {
    #[serde(skip)]
    pub name: &'static str,
    pub coverage: f32,
    pub gain: f32,
    pub vertical: bool,
    pub click_gain: [f32; 2],
    pub click_frequency_hz: [f32; 2],
    pub click_damping_ratio: f32,
    pub modes: [SurfaceMode; 2],
    pub detune: f32,
    pub lowpass_hz: f32,
    pub bubble_probability: f32,
    pub bubble_radius_m: [f32; 2],
    pub bubble_gain: [f32; 2],
    pub bubble_decay: [f32; 2],
    pub bubble_delay_s: f32,
}
impl Surface {
    pub(crate) const fn solid(
        name: &'static str,
        coverage: f32,
        click: f32,
        frequencies: [f32; 2],
        damping: [f32; 2],
        resonance: f32,
        lowpass: f32,
    ) -> Self {
        Self {
            name,
            coverage,
            gain: 1.0,
            vertical: false,
            click_gain: [click; 2],
            click_frequency_hz: [1000.0, 16000.0],
            click_damping_ratio: 2.0,
            modes: [
                SurfaceMode {
                    frequency_hz: frequencies[0],
                    damping_per_s: damping[0],
                    gain: resonance,
                },
                SurfaceMode {
                    frequency_hz: frequencies[1],
                    damping_per_s: damping[1],
                    gain: resonance * 0.5,
                },
            ],
            detune: 0.15,
            lowpass_hz: lowpass,
            bubble_probability: 0.0,
            bubble_radius_m: [0.00035, 0.0016],
            bubble_gain: [1.0; 2],
            bubble_decay: [1.0; 2],
            bubble_delay_s: 0.002,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RainConfig {
    #[serde(default)]
    pub advanced: Advanced,
    pub gain: f32,
    pub drop_gain: f32,
    pub reverb_gain: f32,
    #[serde(default = "default_wind_gain")]
    pub wind_gain: f32,
    #[serde(default = "default_insect_gain")]
    pub insect_gain: f32,
    pub use_block_profiles: bool,
    pub max_drops_per_s: f32,
    pub bed_gain: f32,
    pub sheet_depth: f32,
    pub min_distance_m: f32,
    pub max_distance_m: f32,
    pub surfaces: [Surface; RAIN_MATERIALS],
}
const fn default_wind_gain() -> f32 {
    0.12
}
const fn default_insect_gain() -> f32 {
    1.0
}

impl Default for RainConfig {
    fn default() -> Self {
        let mut water = Surface::solid("Water", 0.37, 0.15, [1000.0; 2], [1000.0; 2], 0.0, 0.0);
        water.click_gain = [0.15, 0.5];
        water.detune = 0.0;
        water.bubble_probability = 0.85;
        water.bubble_gain = [1.2, 2.5];
        water.bubble_decay = [3.0, 8.0];
        Self {
            advanced: Advanced::default(),
            gain: 2.35,
            drop_gain: 3.25,
            reverb_gain: 1.0,
            wind_gain: default_wind_gain(),
            insect_gain: default_insect_gain(),
            use_block_profiles: true,
            max_drops_per_s: 1200.0,
            // Keep the continuous far-rain wash behind the discrete surface impacts.
            bed_gain: 0.20,
            sheet_depth: 0.87,
            min_distance_m: 0.31,
            max_distance_m: 4.5,
            surfaces: [
                water,
                Surface::solid(
                    "Dirt",
                    0.21,
                    1.0,
                    [450.0, 1100.0],
                    [1200.0, 1800.0],
                    0.35,
                    0.0,
                ),
                Surface::solid(
                    "Leaf",
                    0.26,
                    1.0,
                    [1800.0, 4200.0],
                    [800.0, 1400.0],
                    0.5,
                    0.0,
                ),
                Surface::solid(
                    "Concrete",
                    0.15,
                    1.0,
                    [1400.0, 3700.0],
                    [1400.0, 2200.0],
                    0.45,
                    0.0,
                ),
                Surface::solid(
                    "Glass",
                    0.005,
                    1.0,
                    [3200.0, 7100.0],
                    [160.0, 260.0],
                    0.325,
                    0.0,
                ),
                Surface::solid(
                    "Metal",
                    0.005,
                    1.0,
                    [1700.0, 4300.0],
                    [90.0, 150.0],
                    0.4,
                    0.0,
                ),
                Surface::solid(
                    "Plastic",
                    0.0,
                    0.5,
                    [220.0, 650.0],
                    [110.0, 220.0],
                    0.65,
                    1600.0,
                ),
                Surface::solid(
                    "Asphalt",
                    0.0,
                    0.3,
                    [300.0, 900.0],
                    [1600.0, 2600.0],
                    0.25,
                    0.0,
                ),
                Surface::solid(
                    "Asphalt roof",
                    0.0,
                    0.25,
                    [140.0, 420.0],
                    [300.0, 700.0],
                    0.4,
                    900.0,
                ),
                // Native wooden blocks need a less metallic, damped body.
                Surface::solid(
                    "Wood",
                    0.0,
                    0.65,
                    [320.0, 850.0],
                    [220.0, 480.0],
                    0.7,
                    4200.0,
                ),
            ],
        }
    }
}
pub(crate) fn range(value: f32, low: f32, high: f32) -> bool {
    value.is_finite() && (low..=high).contains(&value)
}
impl RainConfig {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if !self.advanced.valid()
            || !range(self.wind_gain, 0.0, 4.0)
            || !range(self.insect_gain, 0.0, 4.0)
            || !range(self.drop_gain, 0.0, 4.0)
            || !range(self.reverb_gain, 0.0, 4.0)
            || !range(self.gain, 0.0, 4.0)
            || !range(self.max_drops_per_s, 0.0, 2000.0)
            || !range(self.bed_gain, 0.0, 4.0)
            || !range(self.sheet_depth, 0.0, 2.0)
            || !range(self.min_distance_m, 0.25, 100.0)
            || !range(self.max_distance_m, self.min_distance_m, 100.0)
        {
            return Err("invalid rain configuration");
        }
        let mut coverage = 0.0;
        for s in &self.surfaces {
            if s.name.len() >= 16
                || !range(s.coverage, 0.0, 1000.0)
                || !range(s.gain, 0.0, 4.0)
                || !range(s.click_gain[0], 0.0, 2.0)
                || !range(s.click_gain[1], s.click_gain[0], 2.0)
                || !range(s.click_frequency_hz[0], 20.0, 20000.0)
                || !range(s.click_frequency_hz[1], s.click_frequency_hz[0], 20000.0)
                || !range(s.click_damping_ratio, 0.05, 50.0)
                || !range(s.detune, 0.0, 0.5)
                || !(s.lowpass_hz == 0.0 || range(s.lowpass_hz, 20.0, 20000.0))
                || !range(s.bubble_probability, 0.0, 1.0)
                || !range(s.bubble_radius_m[0], 0.00016, 0.004)
                || !range(s.bubble_radius_m[1], s.bubble_radius_m[0], 0.004)
                || !range(s.bubble_gain[0], 0.0, 8.0)
                || !range(s.bubble_gain[1], s.bubble_gain[0], 8.0)
                || !range(s.bubble_decay[0], 0.25, 20.0)
                || !range(s.bubble_decay[1], s.bubble_decay[0], 20.0)
                || !range(s.bubble_delay_s, 0.0, 0.1)
                || s.modes.iter().any(|m| {
                    !range(m.frequency_hz, 20.0, 20000.0)
                        || !range(m.damping_per_s, 1.0, 20000.0)
                        || !range(m.gain, 0.0, 4.0)
                })
            {
                return Err("invalid rain surface");
            }
            coverage += s.coverage;
        }
        if coverage <= 0.0 {
            return Err("rain surface coverage is empty");
        }
        Ok(())
    }
}

impl RainConfig {
    /// Restore display names and reject unsafe/nonfinite DSP parameters as one profile.
    pub(crate) fn sanitized(mut self) -> Self {
        self.advanced = self.advanced.sanitized();
        for (surface, default) in self.surfaces.iter_mut().zip(Self::default().surfaces) {
            surface.name = default.name;
        }
        if self.validate().is_ok() {
            self
        } else {
            Self::default()
        }
    }

    pub(crate) fn mute_ambient(&mut self) {
        self.gain = 0.0;
        self.bed_gain = 0.0;
        self.drop_gain = 0.0;
        self.reverb_gain = 0.0;
        self.wind_gain = 0.0;
        self.insect_gain = 0.0;
    }

    pub(crate) fn export(self, master: f32, ambient: f32, effects: f32) -> String {
        serde_json::to_string_pretty(&serde_json::json!({
            "format": "bloxgloom-rain-audio-v1",
            "volumes": {"master": master, "ambient": ambient, "effects": effects},
            "rain": self.sanitized(),
            "material_order": Self::default().surfaces.map(|s| s.name),
        }))
        .expect("finite local audio settings")
    }
}

#[cfg(test)]
mod tests;
