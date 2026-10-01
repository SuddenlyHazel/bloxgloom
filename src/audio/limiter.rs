//! Stereo-linked lookahead limiter; fixed storage and sample-count timing.
//! Adapted from NoiseMachine (MIT, Copyright 2026 kvmet).
const FRAMES: usize = 128;
const QUEUE: usize = FRAMES + 1;
const UNIT: u32 = 16_777_216;
pub(super) struct Limiter {
    delay: [[f32; 2]; FRAMES],
    minima: [u32; FRAMES],
    sum: u64,
    gains: [f32; QUEUE],
    frames: [u64; QUEUE],
    head: usize,
    count: usize,
    position: usize,
    frame: u64,
    gain: f32,
}
impl Default for Limiter {
    fn default() -> Self {
        Self {
            delay: [[0.0; 2]; FRAMES],
            minima: [UNIT; FRAMES],
            sum: FRAMES as u64 * UNIT as u64,
            gains: [0.0; QUEUE],
            frames: [0; QUEUE],
            head: 0,
            count: 0,
            position: 0,
            frame: 0,
            gain: 1.0,
        }
    }
}
impl Limiter {
    pub fn next(&mut self, input: [f32; 2]) -> [f32; 2] {
        let input = input.map(|x| if x.is_finite() { x } else { 0.0 });
        let peak = input[0].abs().max(input[1].abs());
        let need = if peak > 0.98 { 0.98 / peak } else { 1.0 };
        // Expire before inserting, so even a strictly increasing gain stream fits.
        while self.count > 0 && self.frame - self.frames[self.head] > FRAMES as u64 {
            self.head = (self.head + 1) % QUEUE;
            self.count -= 1;
        }
        while self.count > 0 {
            let last = (self.head + self.count - 1) % QUEUE;
            if self.gains[last] < need {
                break;
            }
            self.count -= 1;
        }
        let tail = (self.head + self.count) % QUEUE;
        self.gains[tail] = need;
        self.frames[tail] = self.frame;
        self.count += 1;
        let minimum = (self.gains[self.head] * UNIT as f32) as u32;
        self.sum = self.sum - self.minima[self.position] as u64 + minimum as u64;
        self.minima[self.position] = minimum;
        let target = self.sum as f32 / (FRAMES as f32 * UNIT as f32);
        self.gain = if target < self.gain {
            target
        } else {
            target.min(self.gain + (target - self.gain) / (0.1 * 44_100.0) + 1e-6)
        };
        let delayed = self.delay[self.position];
        self.delay[self.position] = input;
        self.position = (self.position + 1) % FRAMES;
        self.frame += 1;
        delayed.map(|x| (x * self.gain).clamp(-0.98, 0.98))
    }
}
