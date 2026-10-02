//! Per-voice transmission filtering; all smoothing follows the audio sample clock.
use super::SAMPLE_RATE;

pub(super) struct Obstruction {
    gain: f32,
    target_gain: f32,
    coefficient: f32,
    target_coefficient: f32,
    filtered: [f32; 2],
}

impl Default for Obstruction {
    fn default() -> Self {
        Self {
            gain: 1.0,
            target_gain: 1.0,
            coefficient: 1.0,
            target_coefficient: 1.0,
            filtered: [0.0; 2],
        }
    }
}

impl Obstruction {
    pub fn initial(gain: f32, lowpass_hz: f32) -> Self {
        let mut obstruction = Self::default();
        obstruction.set(gain, lowpass_hz);
        obstruction.gain = obstruction.target_gain;
        obstruction.coefficient = obstruction.target_coefficient;
        obstruction
    }

    pub fn valid(gain: f32, lowpass_hz: f32) -> bool {
        gain.is_finite()
            && (0.0..=1.0).contains(&gain)
            && lowpass_hz.is_finite()
            && (200.0..=20_000.0).contains(&lowpass_hz)
    }

    pub fn set(&mut self, gain: f32, lowpass_hz: f32) {
        self.target_gain = gain;
        // The open path is an exact bypass rather than a permanent 20 kHz filter.
        self.target_coefficient = if lowpass_hz >= 20_000.0 {
            1.0
        } else {
            1.0 - (-std::f32::consts::TAU * lowpass_hz / SAMPLE_RATE as f32).exp()
        };
    }

    pub fn next(&mut self, sample: [f32; 2]) -> [f32; 2] {
        let smoothing = 1.0 / (0.1 * SAMPLE_RATE as f32);
        self.gain += (self.target_gain - self.gain) * smoothing;
        self.coefficient += (self.target_coefficient - self.coefficient) * smoothing;
        std::array::from_fn(|ear| {
            self.filtered[ear] += self.coefficient * (sample[ear] - self.filtered[ear]);
            self.filtered[ear] * self.gain
        })
    }
}
