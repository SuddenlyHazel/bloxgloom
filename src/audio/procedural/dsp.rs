//! DSP primitives adapted from NoiseMachine (MIT, Copyright 2026 kvmet).
//! See third-party/NoiseMachine-LICENSE for the retained license.
use std::f32::consts::TAU;

pub(super) const SAMPLE_RATE: f32 = 44_100.0;

pub(super) struct Rng(u32);
impl Rng {
    pub fn new(seed: u32, tag: u32) -> Self {
        let mut x = seed.max(1).wrapping_add(tag);
        x = (x ^ (x >> 16)).wrapping_mul(0x85eb_ca6b);
        x = (x ^ (x >> 13)).wrapping_mul(0xc2b2_ae35);
        x ^= x >> 16;
        Self(if x == 0 { tag.max(1) } else { x })
    }
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
    pub fn unit(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / 16_777_216.0
    }
    pub fn between(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.unit()
    }
    pub fn log_between(&mut self, low: f32, high: f32) -> f32 {
        (low.ln() + (high.ln() - low.ln()) * self.unit()).exp()
    }
    pub fn gaussian(&mut self) -> f32 {
        let mut sum = self.unit();
        sum += self.unit();
        sum += self.unit();
        sum += self.unit();
        1.732_050_8 * (sum - 2.0)
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct Mode {
    coefficient: f32,
    radius_squared: f32,
    current: f32,
    previous: f32,
    remaining: u32,
    delay: u32,
}
impl Mode {
    pub fn new(frequency: f32, damping: f32, amplitude: f32, delay: u32) -> Self {
        let phase = TAU * frequency / SAMPLE_RATE;
        let radius = (-damping / SAMPLE_RATE).exp();
        Self {
            coefficient: 2.0 * radius * phase.cos(),
            radius_squared: radius * radius,
            current: 0.0,
            previous: -amplitude * phase.sin() / radius,
            remaining: (9.210_340_5 * SAMPLE_RATE / damping).ceil() as u32,
            delay,
        }
    }
    pub fn active(&self) -> bool {
        self.remaining > 0 || self.delay > 0
    }
    pub fn next(&mut self) -> f32 {
        if self.delay > 0 {
            self.delay -= 1;
            return 0.0;
        }
        if self.remaining == 0 {
            return 0.0;
        }
        let value = self.current;
        self.current = self.coefficient * self.current - self.radius_squared * self.previous;
        self.previous = value;
        self.remaining -= 1;
        value
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    state: [f32; 2],
}
impl Biquad {
    pub fn bandpass(frequency: f32, q: f32) -> Self {
        let mut value = Self::default();
        value.tune(true, frequency, q);
        value
    }
    pub fn tune(&mut self, bandpass: bool, frequency: f32, q: f32) {
        let phase = TAU * frequency / SAMPLE_RATE;
        let cosine = phase.cos();
        let alpha = phase.sin() / (2.0 * q);
        let norm = 1.0 / (1.0 + alpha);
        if bandpass {
            self.b0 = alpha * norm;
            self.b1 = 0.0;
            self.b2 = -alpha * norm;
        } else {
            self.b0 = 0.5 * (1.0 - cosine) * norm;
            self.b1 = (1.0 - cosine) * norm;
            self.b2 = self.b0;
        }
        self.a1 = -2.0 * cosine * norm;
        self.a2 = (1.0 - alpha) * norm;
    }
    pub fn next(&mut self, input: f32) -> f32 {
        let out = self.b0 * input + self.state[0];
        self.state[0] = self.b1 * input - self.a1 * out + self.state[1];
        self.state[1] = self.b2 * input - self.a2 * out;
        out
    }
    pub fn power(&self, frequency: f32) -> f32 {
        let w = TAU * frequency / SAMPLE_RATE;
        let (s1, c1) = w.sin_cos();
        let (s2, c2) = (2.0 * w).sin_cos();
        let nr = self.b0 + self.b1 * c1 + self.b2 * c2;
        let ni = self.b1 * s1 + self.b2 * s2;
        let dr = 1.0 + self.a1 * c1 + self.a2 * c2;
        let di = self.a1 * s1 + self.a2 * s2;
        (nr * nr + ni * ni) / (dr * dr + di * di)
    }
}
