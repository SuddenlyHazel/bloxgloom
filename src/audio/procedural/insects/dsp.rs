//! Oscillator and tymbal resonator from NoiseMachine (MIT, kvmet 2026).
use super::super::dsp::SAMPLE_RATE;
use std::f32::consts::TAU;
#[derive(Clone, Copy, Default)]
pub(super) struct Oscillator {
    pub coefficient: f32,
    current: f32,
    previous: f32,
}
impl Oscillator {
    pub fn new(frequency: f32) -> Self {
        let phase = TAU * frequency / SAMPLE_RATE;
        Self {
            coefficient: 2.0 * phase.cos(),
            current: 0.0,
            previous: -phase.sin(),
        }
    }
    pub fn next(&mut self) -> f32 {
        let value = self.current;
        let next = self.coefficient * self.current - self.previous;
        self.previous = self.current;
        self.current = next;
        value
    }
}
#[derive(Clone, Copy, Default)]
pub(super) struct Resonator {
    pub coefficient: f32,
    pub radius_squared: f32,
    state: [f32; 2],
    input: [f32; 2],
}
impl Resonator {
    pub fn new(frequency: f32, q: f32) -> Self {
        let phase = TAU * frequency / SAMPLE_RATE;
        let radius = (-std::f32::consts::PI * frequency / (q * SAMPLE_RATE)).exp();
        Self {
            coefficient: 2.0 * radius * phase.cos(),
            radius_squared: radius * radius,
            ..Self::default()
        }
    }
    pub fn next(&mut self, input: f32) -> f32 {
        let output = self.coefficient * self.state[0] - self.radius_squared * self.state[1] + input
            - self.input[1];
        self.input[1] = self.input[0];
        self.input[0] = input;
        self.state[1] = self.state[0];
        self.state[0] = output;
        output
    }
}
