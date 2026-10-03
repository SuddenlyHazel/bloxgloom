//! Persistent local mix buses. Bus order is ambient, effects, UI, music.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct CompressorConfig {
    pub enabled: bool,
    pub threshold_db: f32,
    pub ratio: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub makeup_db: f32,
    pub knee_db: f32,
}

impl Default for CompressorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold_db: -18.0,
            ratio: 4.0,
            attack_ms: 10.0,
            release_ms: 100.0,
            makeup_db: 0.0,
            knee_db: 6.0,
        }
    }
}

impl CompressorConfig {
    pub fn sanitized(self) -> Self {
        let default = Self::default();
        Self {
            enabled: self.enabled,
            threshold_db: bounded(self.threshold_db, -60.0, 0.0, default.threshold_db),
            ratio: bounded(self.ratio, 1.0, 20.0, default.ratio),
            attack_ms: bounded(self.attack_ms, 0.1, 200.0, default.attack_ms),
            release_ms: bounded(self.release_ms, 5.0, 2000.0, default.release_ms),
            makeup_db: bounded(self.makeup_db, 0.0, 24.0, default.makeup_db),
            knee_db: bounded(self.knee_db, 0.0, 24.0, default.knee_db),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct BusConfig {
    pub gain: f32,
    pub compressor: CompressorConfig,
}

impl Default for BusConfig {
    fn default() -> Self {
        Self {
            gain: 1.0,
            compressor: CompressorConfig::default(),
        }
    }
}

impl BusConfig {
    pub fn sanitized(self) -> Self {
        Self {
            gain: bounded(self.gain, 0.0, 4.0, 1.0),
            compressor: self.compressor.sanitized(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct MixConfig {
    pub buses: [BusConfig; 4],
    pub master: CompressorConfig,
}

impl MixConfig {
    pub fn sanitized(self) -> Self {
        Self {
            buses: self.buses.map(BusConfig::sanitized),
            master: self.master.sanitized(),
        }
    }

    pub fn export(self) -> String {
        serde_json::to_string_pretty(&serde_json::json!({
            "format": "bloxgloom-audio-mix-v1",
            "bus_order": ["Ambient", "Effects", "UI", "Music"],
            "mix": self.sanitized(),
        }))
        .expect("finite mix settings serialize")
    }
}

fn bounded(value: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}

#[cfg(test)]
mod tests;
