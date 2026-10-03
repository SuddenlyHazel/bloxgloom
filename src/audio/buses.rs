//! Fixed-size mix buses with stereo-linked peak compressors. No audio-thread allocation.
use super::{SAMPLE_RATE, mix_tuning::CompressorConfig, mix_tuning::MixConfig};

struct Compressor {
    config: CompressorConfig,
    envelope: f32,
    attack: f32,
    release: f32,
    slope: f32,
    makeup: f32,
    knee_half: f32,
    knee_start: f32,
}

impl Compressor {
    fn new(config: CompressorConfig) -> Self {
        let mut compressor = Self {
            config: CompressorConfig::default(),
            envelope: 0.0,
            attack: 0.0,
            release: 0.0,
            slope: 0.0,
            makeup: 1.0,
            knee_half: 0.0,
            knee_start: 0.0,
        };
        compressor.set_config(config);
        compressor
    }

    fn set_config(&mut self, config: CompressorConfig) {
        if config.enabled != self.config.enabled {
            self.envelope = 0.0;
        }
        self.config = config;
        self.attack = (-1.0 / (config.attack_ms * 0.001 * SAMPLE_RATE as f32)).exp();
        self.release = (-1.0 / (config.release_ms * 0.001 * SAMPLE_RATE as f32)).exp();
        self.slope = 1.0 / config.ratio - 1.0;
        self.makeup = 10.0_f32.powf(config.makeup_db / 20.0);
        self.knee_half = config.knee_db * 0.5;
        self.knee_start = 10.0_f32.powf((config.threshold_db - self.knee_half) / 20.0);
    }

    fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        if !self.config.enabled {
            return input;
        }
        let peak = input[0].abs().max(input[1].abs());
        let coefficient = if peak > self.envelope {
            self.attack
        } else {
            self.release
        };
        self.envelope = peak + coefficient * (self.envelope - peak);
        if self.envelope < 1.0e-20 {
            self.envelope = 0.0;
        }
        if self.envelope <= self.knee_start {
            return input.map(|sample| sample * self.makeup);
        }
        let over_db = 20.0 * self.envelope.max(1.0e-20).log10() - self.config.threshold_db;
        let reduction_db = if over_db <= -self.knee_half {
            0.0
        } else if self.config.knee_db > 0.0 && over_db < self.knee_half {
            self.slope * (over_db + self.knee_half).powi(2) / (2.0 * self.config.knee_db)
        } else {
            self.slope * over_db
        };
        let gain = self.makeup * 10.0_f32.powf(reduction_db / 20.0);
        input.map(|sample| sample * gain)
    }
}

pub(crate) struct Processor {
    config: MixConfig,
    gains: [f32; 4],
    smooth: f32,
    compressors: [Compressor; 4],
    master: Compressor,
}

impl Default for Processor {
    fn default() -> Self {
        let config = MixConfig::default();
        Self {
            config,
            gains: config.buses.map(|bus| bus.gain),
            smooth: (-1.0 / (0.020 * SAMPLE_RATE as f32)).exp(),
            compressors: config.buses.map(|bus| Compressor::new(bus.compressor)),
            master: Compressor::new(config.master),
        }
    }
}

impl Processor {
    pub fn set_config(&mut self, config: MixConfig) {
        let config = config.sanitized();
        for (compressor, bus) in self.compressors.iter_mut().zip(config.buses) {
            compressor.set_config(bus.compressor);
        }
        self.master.set_config(config.master);
        self.config = config;
    }

    /// Clear history when retiring a session while preserving local mix settings.
    pub fn reset(&mut self) {
        self.gains = self.config.buses.map(|bus| bus.gain);
        for compressor in &mut self.compressors {
            compressor.envelope = 0.0;
        }
        self.master.envelope = 0.0;
    }

    /// Legacy levels are master, ambient, effects. They remain authoritative mute gates.
    pub fn process(&mut self, stems: [[f32; 2]; 4], legacy_levels: [f32; 3]) -> [f32; 2] {
        let levels = legacy_levels.map(|value| finite(value).clamp(0.0, 1.0));
        let categories = [levels[1], levels[2], levels[2], levels[1]];
        let mut mixed = [0.0; 2];
        for (index, stem) in stems.into_iter().enumerate() {
            let target = self.config.buses[index].gain;
            self.gains[index] = target + self.smooth * (self.gains[index] - target);
            if (self.gains[index] - target).abs() < 1.0e-6 {
                self.gains[index] = target;
            }
            let input = stem.map(|sample| finite(sample) * self.gains[index]);
            let output = self.compressors[index].process(input);
            for channel in 0..2 {
                mixed[channel] += output[channel] * categories[index];
            }
        }
        self.master.process(mixed).map(|sample| sample * levels[0])
    }
}

fn finite(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

#[cfg(test)]
mod tests;
