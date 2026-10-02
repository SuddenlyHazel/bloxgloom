//! Wind filters adapted from NoiseMachine (MIT, Copyright 2026 kvmet).
use super::dsp::{Rng, SAMPLE_RATE};
pub(super) struct Wind {
    profile: crate::audio::rain_tuning::WindProfile,
    rng: Rng,
    air: [f32; 2],
    rumble: [f32; 2],
    level: [f32; 2],
    target: [f32; 2],
    alpha: f32,
}
impl Wind {
    pub fn new(seed: u32) -> Self {
        Self {
            profile: Default::default(),
            rng: Rng::new(seed, 0x1715_609d),
            air: [0.0; 2],
            rumble: [0.0; 2],
            level: [0.0; 2],
            target: [0.0; 2],
            alpha: 0.0,
        }
    }
    pub fn configure(&mut self, profile: crate::audio::rain_tuning::WindProfile) {
        self.profile = profile;
    }
    pub fn follow(&mut self, speed: f32, bearing: f32) {
        let level = 0.4315 * 0.5 * speed.min(35.0) / 10.0;
        let lateral = bearing.sin();
        self.target = [
            level * (1.0 - self.profile.balance * lateral).sqrt(),
            level * (1.0 + self.profile.balance * lateral).sqrt(),
        ];
        let cutoff = self.profile.brightness * 400.0 * 20.0f32.powf((speed / 30.0).min(1.0));
        self.alpha = -(-std::f32::consts::TAU * cutoff / SAMPLE_RATE).exp_m1();
    }
    pub fn next(&mut self) -> [f32; 2] {
        let common = 2.0 * self.rng.unit() - 1.0;
        let mut out = [0.0; 2];
        for (channel, value) in out.iter_mut().enumerate() {
            let side = 2.0 * self.rng.unit() - 1.0;
            let input =
                (1.0 - self.profile.stereo_width) * common + self.profile.stereo_width * side;
            self.air[channel] += self.alpha * (input - self.air[channel]);
            self.rumble[channel] += 0.016_953_3 * (input - self.rumble[channel]);
            self.level[channel] +=
                (self.target[channel] - self.level[channel]) / (0.02 * SAMPLE_RATE);
            *value = self.level[channel]
                * (0.55 * self.air[channel] + 0.30 * self.profile.rumble * self.rumble[channel]);
        }
        out
    }
}

#[cfg(test)]
#[path = "wind/tests.rs"]
mod tests;
