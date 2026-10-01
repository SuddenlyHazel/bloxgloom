//! Six-line feedback-delay network adapted from NoiseMachine (MIT, 2026 kvmet).
pub(super) struct Reverb {
    lines: [Vec<f32>; 6],
    positions: [usize; 6],
    damping: [f32; 6],
    feedback: [f32; 6],
    alpha: f32,
}
impl Reverb {
    pub fn new(lengths: [usize; 6], rate: f32, decay: f32, alpha: f32) -> Self {
        Self {
            lines: lengths.map(|n| vec![0.0; n]),
            positions: [0; 6],
            damping: [0.0; 6],
            feedback: lengths.map(|n| 0.001f32.powf(n as f32 / (decay * rate))),
            alpha,
        }
    }
    pub fn next(&mut self, send: f32) -> [f32; 2] {
        for i in 0..6 {
            self.damping[i] += self.alpha * (self.lines[i][self.positions[i]] - self.damping[i]);
        }
        let d = self.damping;
        let feedback = [
            d[1] + d[2] + d[3] + d[4] + d[5],
            d[0] + d[2] - d[3] - d[4] + d[5],
            d[0] + d[1] + d[3] - d[4] - d[5],
            d[0] - d[1] + d[2] + d[4] - d[5],
            d[0] - d[1] - d[2] + d[3] + d[5],
            d[0] + d[1] - d[2] - d[3] + d[4],
        ];
        for (i, f) in feedback.into_iter().enumerate() {
            self.lines[i][self.positions[i]] =
                0.408_248_3 * send + self.feedback[i] * 0.447_213_6 * f;
            self.positions[i] = (self.positions[i] + 1) % self.lines[i].len();
        }
        [
            0.577_350_26 * d[0] + 0.288_675_13 * d[1]
                - 0.288_675_13 * d[2]
                - 0.577_350_26 * d[3]
                - 0.288_675_13 * d[4]
                + 0.288_675_13 * d[5],
            0.5 * d[1] + 0.5 * d[2] - 0.5 * d[4] - 0.5 * d[5],
        ]
    }
}
