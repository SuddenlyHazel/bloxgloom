//! Spherical-head shelf and fractional ear delays adapted from NoiseMachine.
//! MIT Copyright 2026 kvmet; see third-party/NoiseMachine-LICENSE.
use super::dsp::SAMPLE_RATE;
use std::f32::consts::PI;
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Listener {
    pub width_m: f32,
    pub head_amount: f32,
    pub rear_amount: f32,
}
impl From<crate::audio::rain_tuning::ListenerProfile> for Listener {
    fn from(p: crate::audio::rain_tuning::ListenerProfile) -> Self {
        Self {
            width_m: p.width_m,
            head_amount: p.head_amount,
            rear_amount: p.rear_amount,
        }
    }
}
impl Default for Listener {
    fn default() -> Self {
        Self {
            width_m: 0.18,
            head_amount: 1.0,
            rear_amount: 1.0,
        }
    }
}
#[derive(Clone, Copy, Default)]
pub(super) struct Spatial {
    ear_gain: [f32; 2],
    b0: [f32; 2],
    b1: [f32; 2],
    feedback: f32,
    state: [f32; 2],
    previous: f32,
    delay: [usize; 2],
    weight: [[f32; 4]; 2],
    lowpass: f32,
    alpha: f32,
    reverb: f32,
}
pub(super) struct Bus {
    direct: [[f32; 128]; 2],
    position: usize,
}
impl Default for Bus {
    fn default() -> Self {
        Self {
            direct: [[0.0; 128]; 2],
            position: 0,
        }
    }
}
impl Bus {
    pub fn add(&mut self, frame: [f32; 2]) {
        for (ear, sample) in frame.into_iter().enumerate() {
            self.direct[ear][self.position] += sample;
        }
    }
    pub fn next(&mut self) -> [f32; 2] {
        let out = [self.direct[0][self.position], self.direct[1][self.position]];
        self.direct[0][self.position] = 0.0;
        self.direct[1][self.position] = 0.0;
        self.position = (self.position + 1) % 128;
        out
    }
}
impl Spatial {
    pub fn new(distance: f32, angle: f32, listener: Listener) -> Self {
        let radius = 0.5 * listener.width_m;
        let distance = distance.max(radius + 0.001);
        let lateral = angle.sin();
        let k = SAMPLE_RATE * radius / 343.0;
        let head = radius > 0.0 && listener.head_amount > 0.0;
        let mut value = Self {
            feedback: if head { (k - 1.0) / (k + 1.0) } else { 0.0 },
            reverb: 1.0 / distance.max(1.0).sqrt(),
            ..Self::default()
        };
        let mut path = [0.0; 2];
        for (ear, p) in path.iter_mut().enumerate() {
            let cosine = if ear == 0 { -lateral } else { lateral };
            let gap = distance - radius;
            let straight = (gap * gap + 2.0 * distance * radius * (1.0 - cosine)).sqrt();
            *p = straight;
            value.ear_gain[ear] = std::f32::consts::FRAC_1_SQRT_2 / straight.max(1.0);
            value.b0[ear] = 1.0;
            if head {
                let theta = cosine.clamp(-1.0, 1.0).acos();
                let tangent = (radius / distance).acos();
                if theta > tangent {
                    let around = ((distance - radius) * (distance + radius)).sqrt()
                        + radius * (theta - tangent);
                    *p += listener.head_amount * (around - straight);
                }
                let alpha = 1.05 + 0.95 * (theta * 1.2).cos();
                let alpha = 1.0 + listener.head_amount * (alpha - 1.0);
                value.b0[ear] = (1.0 + alpha * k) / (1.0 + k);
                value.b1[ear] = (1.0 - alpha * k) / (1.0 + k);
            }
        }
        let first = path[0].min(path[1]);
        for (ear, p) in path.into_iter().enumerate() {
            let delay = (p - first) * (SAMPLE_RATE / 343.0);
            value.delay[ear] = delay as usize;
            let f = delay - value.delay[ear] as f32;
            value.weight[ear] = [
                -f * (f - 1.0) * (f - 2.0) / 6.0,
                (f + 1.0) * (f - 1.0) * (f - 2.0) / 2.0,
                -(f + 1.0) * f * (f - 2.0) / 2.0,
                (f + 1.0) * f * (f - 1.0) / 6.0,
            ];
        }
        let rear = 0.5 * (1.0 - angle.cos());
        let cutoff = 18_000.0 - 15_000.0 * rear;
        value.alpha = -(-2.0 * PI * cutoff / SAMPLE_RATE).exp_m1();
        value
    }
    pub fn retarget(&mut self, distance: f32, angle: f32, listener: Listener) {
        *self = Self {
            state: self.state,
            previous: self.previous,
            lowpass: self.lowpass,
            ..Self::new(distance, angle, listener)
        };
    }
    pub fn emit(&mut self, listener: Listener, bus: &mut Bus, source: f32) -> f32 {
        self.lowpass += self.alpha * (source - self.lowpass);
        let direct = source + listener.rear_amount * (self.lowpass - source);
        for ear in 0..2 {
            let filtered = self.b0[ear] * direct
                + self.b1[ear] * self.previous
                + self.feedback * self.state[ear];
            self.state[ear] = filtered;
            for tap in 0..4 {
                bus.direct[ear][(bus.position + self.delay[ear] + tap) % 128] +=
                    filtered * self.ear_gain[ear] * self.weight[ear][tap];
            }
        }
        self.previous = direct;
        self.reverb * source
    }
}
