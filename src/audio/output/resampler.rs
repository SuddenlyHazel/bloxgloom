//! Worker-side linear interpolation from the fixed mixer rate.
use super::*;

pub(super) struct Source {
    pub(super) mixer: Mixer,
    pub(super) frames: [[f32; 2]; SOURCE_FRAMES],
    pub(super) cursor: usize,
}
impl Source {
    pub(super) fn new() -> Self {
        Self {
            mixer: Mixer::new(0xb10c6100),
            frames: [[0.0; 2]; SOURCE_FRAMES],
            cursor: SOURCE_FRAMES,
        }
    }
    pub(super) fn next(&mut self) -> [f32; 2] {
        if self.cursor == SOURCE_FRAMES {
            self.mixer.render(&mut self.frames);
            self.cursor = 0;
        }
        let frame = self.frames[self.cursor];
        self.cursor += 1;
        frame
    }
    pub(super) fn reset(&mut self) {
        self.mixer.command(Command::Reset);
        self.cursor = SOURCE_FRAMES;
    }
}
pub(super) struct Resampler {
    step: f64,
    phase: f64,
    pair: Option<[[f32; 2]; 2]>,
}
impl Resampler {
    pub(super) fn new(rate: u32) -> Self {
        Self {
            step: f64::from(SAMPLE_RATE) / f64::from(rate),
            phase: 0.0,
            pair: None,
        }
    }
    pub(super) fn next(&mut self, source: &mut Source) -> [f32; 2] {
        let pair = self
            .pair
            .get_or_insert_with(|| [source.next(), source.next()]);
        let output =
            std::array::from_fn(|i| pair[0][i] + (pair[1][i] - pair[0][i]) * self.phase as f32);
        self.phase += self.step;
        while self.phase >= 1.0 {
            pair[0] = pair[1];
            pair[1] = source.next();
            self.phase -= 1.0;
        }
        output
    }
    pub(super) fn reset(&mut self) {
        self.phase = 0.0;
        self.pair = None;
    }
}
