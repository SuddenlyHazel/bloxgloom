//! Tortuous lightning-channel thunder, adapted from NoiseMachine `noise_thunder.c`.
//! Copyright (c) 2026 kvmet. MIT; see `third-party/NoiseMachine-LICENSE`.
//! Fixed 44.1 kHz. Trigger coordinates: metres, radians clockwise from front.
use super::dsp::{Biquad, Rng, SAMPLE_RATE};
use super::reverb::Reverb;
use std::f32::consts::{PI, TAU};
const RATE: f32 = SAMPLE_RATE;
const SEGMENTS: usize = 256;
const BANDS: usize = 4;
const ECHOES: usize = 6;

#[derive(Clone, Copy, Default)]
struct Segment {
    start: f32,
    width: f32,
    gain: [f32; 2],
    roughness: f32,
    band: f32,
}
#[derive(Clone, Copy, Default)]
struct Echo {
    delay: f32,
    smear: f32,
    range_log: f32,
    gain: [f32; 2],
    first: usize,
    next: usize,
}
#[derive(Clone, Copy, Default)]
struct Reflector {
    position: [f32; 2],
    reflectivity: f32,
    smear: f32,
}
struct Voice {
    segments: [Segment; SEGMENTS],
    count: usize,
    first: usize,
    next: usize,
    elapsed: u32,
    length: u32,
    pulse: [[Biquad; 2]; BANDS],
    bass: [[Biquad; 2]; BANDS],
    air: [[[Biquad; 2]; 2]; BANDS],
    echoes: [Echo; ECHOES],
    span_log: f32,
    echo_span_log: f32,
    gain: f32,
}
impl Default for Voice {
    fn default() -> Self {
        Self {
            segments: [Segment::default(); SEGMENTS],
            count: 0,
            first: 0,
            next: 0,
            elapsed: 0,
            length: 0,
            bass: std::array::from_fn(|_| std::array::from_fn(|_| Biquad::default())),
            pulse: std::array::from_fn(|_| std::array::from_fn(|_| Biquad::default())),
            air: std::array::from_fn(|_| {
                std::array::from_fn(|_| std::array::from_fn(|_| Biquad::default()))
            }),
            echoes: [Echo::default(); ECHOES],
            span_log: 0.0,
            echo_span_log: 0.0,
            gain: 1.0,
        }
    }
}

/// Two fixed strike voices. `next` performs no allocation or locking.
/// A full pool rejects a strike rather than replacing a sounding channel.
pub struct Thunder {
    rng: Rng,
    echo_rng: Rng,
    voices: [Voice; 2],
    reflectors: [Reflector; ECHOES],
    rejected: u64,
    reverb: Reverb,
    reverb_input: f32,
    reverb_phase: usize,
    reverb_previous: [f32; 2],
    reverb_current: [f32; 2],
}
impl Thunder {
    pub fn new(seed: u32) -> Self {
        let mut terrain = Rng::new(seed, 0xbb67_ae85);
        let reflectors = std::array::from_fn(|_| {
            let distance = terrain.between(300.0, 2500.0);
            let angle = TAU * terrain.unit();
            Reflector {
                position: [distance * angle.sin(), distance * angle.cos()],
                reflectivity: terrain.between(0.3, 0.6),
                smear: terrain.between(0.1, 0.4),
            }
        });
        Self {
            rng: Rng::new(seed, 0x2545_f491),
            echo_rng: Rng::new(seed, 0x6a09_e667),
            voices: std::array::from_fn(|_| Voice::default()),
            reflectors,
            rejected: 0,
            reverb: Reverb::new([557, 719, 887, 1063, 1297, 1609], RATE / 4.0, 3.5, 0.5),
            reverb_input: 0.0,
            reverb_phase: 0,
            reverb_previous: [0.0; 2],
            reverb_current: [0.0; 2],
        }
    }
    pub fn rejected(&self) -> u64 {
        self.rejected
    }
    pub fn active_voices(&self) -> usize {
        self.voices.iter().filter(|v| v.length != 0).count()
    }
    /// Starts at the first audible arrival, as upstream does. The caller schedules
    /// an overall lightning-to-thunder delay if needed; the channel itself retains
    /// all relative acoustic travel times and distance-dependent spectral losses.
    pub fn trigger(&mut self, distance: f32, angle: f32) -> bool {
        self.trigger_gain(distance, angle, 1.0)
    }
    pub fn trigger_gain(&mut self, distance: f32, angle: f32, gain: f32) -> bool {
        if !(200.0..=15_000.0).contains(&distance)
            || !angle.is_finite()
            || !gain.is_finite()
            || !(0.0..=1.0).contains(&gain)
        {
            self.rejected = self.rejected.saturating_add(1);
            return false;
        }
        let Some(voice) = self.voices.iter_mut().find(|v| v.length == 0) else {
            self.rejected = self.rejected.saturating_add(1);
            return false;
        };
        *voice = Voice::default();
        build_voice(voice, &mut self.rng, &self.reflectors, distance, angle);
        voice.gain = gain;
        true
    }
    pub fn next(&mut self) -> [f32; 2] {
        let mut sum = [0.0; 2];
        for voice in &mut self.voices {
            if voice.length == 0 {
                continue;
            }
            let out = voice.next(&mut self.rng, &mut self.echo_rng);
            for channel in 0..2 {
                sum[channel] += 1.93 * out[channel] * voice.gain;
            }
        }
        self.reverb_input += sum[0] + sum[1];
        self.reverb_phase += 1;
        if self.reverb_phase == 4 {
            self.reverb_phase = 0;
            self.reverb_previous = self.reverb_current;
            self.reverb_current = self.reverb.next(self.reverb_input / 4.0);
            self.reverb_input = 0.0;
        }
        let blend = self.reverb_phase as f32 / 4.0;
        for (channel, sample) in sum.iter_mut().enumerate() {
            *sample += 0.5
                * (self.reverb_previous[channel]
                    + blend * (self.reverb_current[channel] - self.reverb_previous[channel]));
        }
        sum.map(limit)
    }
}

fn length(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}
struct Build<'a> {
    voice: &'a mut Voice,
    rng: &'a mut Rng,
    step: f32,
    centroid: [f32; 3],
    weight: f32,
}
impl Build<'_> {
    fn segment(&mut self, from: [f32; 3], to: [f32; 3], metres: f32, weight: f32) {
        let mid = std::array::from_fn(|k| 0.5 * (from[k] + to[k]));
        let near = length(from).min(length(to));
        let far = length(from).max(length(to));
        let width =
            ((far - near).powi(2) + (0.28 * (metres * 3.0).sqrt()).powi(2)).sqrt() * RATE / 343.0;
        let amplitude = 8.9 * weight * metres * 1000.0 / length(mid) / width;
        let horizontal = mid[0].hypot(mid[1]);
        let pan = 0.25
            * PI
            * (1.0
                + if horizontal > 0.0 {
                    mid[0] / horizontal
                } else {
                    0.0
                });
        self.voice.segments[self.voice.count] = Segment {
            start: near * RATE / 343.0,
            width,
            gain: [amplitude * pan.cos(), amplitude * pan.sin()],
            roughness: 0.3 / (metres / (3.0 * width)).sqrt(),
            band: length(mid),
        };
        self.voice.count += 1;
        for (k, value) in mid.iter().enumerate() {
            self.centroid[k] += weight * metres * value;
        }
        self.weight += weight * metres;
    }
    // Gaussian direction changes with attraction toward a preferred heading.
    fn walk(
        &mut self,
        point: &mut [f32; 3],
        direction: &mut [f32; 3],
        preferred: [f32; 3],
        path: f32,
        weight: f32,
        fade: u8,
    ) {
        let start = self.voice.count;
        let mut travelled = 0.0;
        while travelled < path && self.voice.count < SEGMENTS {
            let step = self.step * self.rng.between(0.5, 1.5);
            for k in 0..3 {
                direction[k] += 0.25 * self.rng.gaussian() + 0.2 * preferred[k];
            }
            let norm = length(*direction).max(1e-6);
            for d in direction.iter_mut() {
                *d /= norm;
            }
            let next = std::array::from_fn(|k| point[k] + step * direction[k]);
            self.segment(*point, next, step, weight);
            *point = next;
            travelled += step;
        }
        let end = self.voice.count;
        if fade == 0 || end - start < 2 {
            return;
        }
        let segments = &mut self.voice.segments;
        let root_late =
            fade == 1 && segments[start].start > segments[(start + 3).min(end - 1)].start;
        let tip_late = segments[end - 1].start > segments[end.saturating_sub(4).max(start)].start;
        for (offset, segment) in segments[start..end].iter_mut().enumerate() {
            let done = (offset as f32 + 0.5) / (end - start) as f32;
            let fade =
                (if root_late { done } else { 1.0 }).min(if tip_late { 1.0 - done } else { 1.0 });
            for gain in &mut segment.gain {
                *gain *= (fade / 0.4).min(1.0);
            }
        }
    }
}
fn build_voice(
    voice: &mut Voice,
    rng: &mut Rng,
    reflectors: &[Reflector; ECHOES],
    distance: f32,
    angle: f32,
) {
    let main_path = 1.15 * rng.between(1500.0, 4000.0);
    let cloud_path = rng.between(1500.0, 5000.0);
    let branches = 1 + (rng.next_u32() % 3) as usize;
    let arms = 2 + (rng.next_u32() % 2) as usize;
    let mut at = [0.0; 3];
    let mut paths = [0.0; 3];
    let mut total = main_path + arms as f32 * cloud_path;
    for i in 0..branches {
        at[i] = rng.between(0.2, 0.9);
        paths[i] = rng.between(200.0, 1200.0);
        total += paths[i];
    }
    at[..branches].sort_unstable_by(f32::total_cmp);
    let mut build = Build {
        voice,
        rng,
        step: total / (0.95 * SEGMENTS as f32),
        centroid: [0.0; 3],
        weight: 0.0,
    };
    let mut point = [distance * angle.sin(), distance * angle.cos(), 0.0];
    let mut direction = [0.0, 0.0, 1.0];
    let mut branch_points = [[0.0; 3]; 3];
    let mut branch_directions = [[0.0; 3]; 3];
    let mut walked = 0.0;
    for i in 0..branches {
        build.walk(
            &mut point,
            &mut direction,
            [0.0, 0.0, 1.0],
            at[i] * main_path - walked,
            1.0,
            0,
        );
        walked = at[i] * main_path;
        branch_points[i] = point;
        branch_directions[i] = direction;
    }
    build.walk(
        &mut point,
        &mut direction,
        [0.0, 0.0, 1.0],
        main_path - walked,
        1.0,
        2,
    );
    let top = point;
    let heading = TAU * build.rng.unit();
    for arm in 0..arms {
        let h = heading + TAU * (arm as f32 + build.rng.between(-0.25, 0.25)) / arms as f32;
        let level = [h.cos(), h.sin(), 0.0];
        point = top;
        direction = level;
        build.walk(
            &mut point,
            &mut direction,
            level,
            cloud_path,
            0.6 / (arms as f32).sqrt(),
            1,
        );
    }
    for i in 0..branches {
        let outward = TAU * build.rng.unit();
        let down = [0.64 * outward.cos(), 0.64 * outward.sin(), -0.77];
        build.walk(
            &mut branch_points[i],
            &mut branch_directions[i],
            down,
            paths[i],
            0.4,
            1,
        );
    }
    let centre = build.centroid.map(|x| x / build.weight);
    let voice = build.voice;
    voice.segments[..voice.count].sort_unstable_by(|a, b| a.start.total_cmp(&b.start));
    let first = voice.segments[0].start;
    let mut last: f32 = 0.0;
    let mut nearest = f32::MAX;
    let mut farthest: f32 = 0.0;
    for segment in &mut voice.segments[..voice.count] {
        segment.start -= first;
        last = last.max(segment.start + segment.width);
        nearest = nearest.min(segment.band);
        farthest = farthest.max(segment.band);
    }
    voice.span_log = (farthest / nearest).ln();
    let scale = if voice.span_log > 0.0 {
        2.0 / voice.span_log
    } else {
        0.0
    };
    for segment in &mut voice.segments[..voice.count] {
        segment.band = scale * (segment.band / nearest).ln();
    }
    let direct = length(centre);
    let mut widest: f32 = 1.0;
    let mut latest: f32 = 0.0;
    for (echo, reflector) in voice.echoes.iter_mut().zip(reflectors) {
        let ground = [reflector.position[0], reflector.position[1], 0.0];
        let out = std::array::from_fn(|k| centre[k] - ground[k]);
        let path = length(out) + length(ground);
        echo.delay = ((path - direct) * RATE / 343.0).max(0.0);
        echo.smear = reflector.smear * RATE;
        echo.range_log = (path / direct).ln();
        let amplitude = reflector.reflectivity * direct / path;
        let pan = 0.25 * PI * (1.0 + ground[0] / length(ground));
        echo.gain = [amplitude * pan.cos(), amplitude * pan.sin()];
        widest = widest.max(path / direct);
        latest = latest.max(echo.delay + echo.smear);
    }
    voice.echo_span_log = widest.ln().max(1e-6);
    let period = 0.001 * build.rng.between(6.0, 14.0);
    for band in 0..BANDS {
        let metres = distance
            * (if band < 3 {
                voice.span_log * band as f32 / 2.0
            } else {
                voice.span_log + voice.echo_span_log
            })
            .exp();
        let period = period * (metres / 1000.0).sqrt().sqrt();
        let cutoff = (1000.0 * (1000.0 / metres).powf(0.6)).clamp(150.0, 6000.0);
        for channel in 0..2 {
            voice.pulse[band][channel].tune(true, 1.0 / period, 0.7);
            // A broad low-frequency body shares the physical strike excitation,
            // stereo position and echoes, rather than adding an unrelated tone.
            voice.bass[band][channel].tune(
                true,
                (65.0 * (1000.0 / metres).powf(0.15)).clamp(35.0, 85.0),
                0.6,
            );
            voice.air[band][channel][0].tune(false, cutoff, 0.541_196_1);
            voice.air[band][channel][1].tune(false, cutoff, 1.306_563);
        }
    }
    voice.length = (last + latest) as u32 + 4096;
}
fn add_bands(excitation: &mut [[f32; 2]; BANDS], position: f32, amount: [f32; 2]) {
    let position = position.clamp(0.0, 3.0);
    let band = (position as usize).min(2);
    let upper = position - band as f32;
    for (channel, amount) in amount.into_iter().enumerate() {
        excitation[band][channel] += (1.0 - upper) * amount;
        excitation[band + 1][channel] += upper * amount;
    }
}
impl Voice {
    fn next(&mut self, rng: &mut Rng, echo_rng: &mut Rng) -> [f32; 2] {
        let t = self.elapsed as f32;
        while self.next < self.count && self.segments[self.next].start < t + 1.0 {
            self.next += 1;
        }
        while self.first < self.next
            && t >= self.segments[self.first].start + self.segments[self.first].width
        {
            self.first += 1;
        }
        let mut excitation = [[0.0; 2]; BANDS];
        for segment in &self.segments[self.first..self.next] {
            let x = t - segment.start;
            let overlap = (x + 1.0).min(segment.width) - x.max(0.0);
            if overlap <= 0.0 {
                continue;
            }
            let share = overlap + segment.roughness * rng.gaussian() * overlap.sqrt();
            add_bands(
                &mut excitation,
                segment.band,
                segment.gain.map(|x| x * share),
            );
        }
        for echo in &mut self.echoes {
            let te = t - echo.delay;
            while echo.next < self.count && self.segments[echo.next].start < te + 1.0 {
                echo.next += 1;
            }
            while echo.first < echo.next
                && te
                    >= self.segments[echo.first].start
                        + self.segments[echo.first].width
                        + echo.smear
            {
                echo.first += 1;
            }
            for segment in &self.segments[echo.first..echo.next] {
                let width = segment.width + echo.smear;
                let x = te - segment.start;
                let overlap = (x + 1.0).min(width) - x.max(0.0);
                if overlap <= 0.0 {
                    continue;
                }
                let share = segment.width / width
                    * (overlap + segment.roughness * echo_rng.gaussian() * overlap.sqrt());
                let level = segment.gain[0].hypot(segment.gain[1]) * share;
                let u = segment.band * self.span_log / 2.0 + echo.range_log;
                let band = if u <= self.span_log {
                    if self.span_log > 0.0 {
                        2.0 * u / self.span_log
                    } else {
                        0.0
                    }
                } else {
                    2.0 + (u - self.span_log) / self.echo_span_log
                };
                add_bands(&mut excitation, band, echo.gain.map(|x| x * level));
            }
        }
        let mut out = [0.0; 2];
        for (channel, sample) in out.iter_mut().enumerate() {
            for (band, excitation) in excitation.iter().enumerate() {
                let pulse = 0.65 * self.pulse[band][channel].next(excitation[channel])
                    + 2.4 * self.bass[band][channel].next(excitation[channel]);
                let air = self.air[band][channel][0].next(pulse);
                *sample += self.air[band][channel][1].next(air);
            }
        }
        self.elapsed += 1;
        if self.elapsed >= self.length {
            self.length = 0;
        }
        out
    }
}
fn limit(x: f32) -> f32 {
    let magnitude = x.abs();
    if magnitude <= 0.5 {
        x
    } else {
        (0.5 + 0.5 * (2.0 * (magnitude - 0.5)).tanh()).copysign(x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn thunder_has_low_frequency_body_without_clipping_or_dc() {
        for seed in [7, 27, 99] {
            let mut thunder = Thunder::new(seed);
            assert!(thunder.trigger(600.0, 0.0));
            let mut bass = Biquad::bandpass(80.0, 0.6);
            let mut treble = Biquad::bandpass(2000.0, 0.6);
            let mut low_energy = 0.0_f64;
            let mut high_energy = 0.0_f64;
            let mut mean = 0.0_f64;
            for _ in 0..44_100 * 12 {
                let frame = thunder.next();
                assert!(frame.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
                let sample = (frame[0] + frame[1]) * 0.5;
                low_energy += f64::from(bass.next(sample)).powi(2);
                high_energy += f64::from(treble.next(sample)).powi(2);
                mean += f64::from(sample);
            }
            assert!(
                low_energy > 4.0 * high_energy,
                "seed {seed}: bass {low_energy}, treble {high_energy}"
            );
            assert!((mean / (44_100.0 * 12.0)).abs() < 0.01);
        }
    }
    #[test]
    fn strike_pool_rejects_overload_and_invalid_coordinates() {
        let mut thunder = Thunder::new(13);
        assert!(!thunder.trigger(f32::NAN, 0.0));
        assert!(!thunder.trigger(199.0, 0.0));
        assert!(!thunder.trigger(15_001.0, 0.0));
        assert!(!thunder.trigger(200.0, f32::INFINITY));
        assert!(thunder.trigger(200.0, 0.0));
        assert!(thunder.trigger(15_000.0, PI));
        assert!(!thunder.trigger(1000.0, 0.0));
        assert_eq!(thunder.active_voices(), 2);
        assert_eq!(thunder.rejected(), 5);
        for voice in &thunder.voices {
            assert!(voice.count > 100 && voice.count <= SEGMENTS);
            assert!(
                voice.segments[..voice.count]
                    .windows(2)
                    .all(|w| w[0].start <= w[1].start)
            );
        }
    }
    #[test]
    fn seeded_near_and_far_strikes_are_finite_deterministic_and_retire() {
        for distance in [200.0, 15_000.0] {
            let mut a = Thunder::new(27);
            let mut b = Thunder::new(27);
            assert!(a.trigger(distance, PI / 2.0));
            assert!(b.trigger(distance, PI / 2.0));
            let frames = a.voices[0].length as usize;
            assert!(frames < 40 * 44_100);
            let mut energy = [0.0_f64; 2];
            for _ in 0..frames + 4096 {
                let left = a.next();
                assert_eq!(left, b.next());
                for channel in 0..2 {
                    assert!(left[channel].is_finite() && left[channel].abs() <= 1.0);
                    energy[channel] += f64::from(left[channel]).powi(2);
                }
            }
            assert!(energy[1] > 0.01 && energy[1] > energy[0]);
            assert_eq!(a.active_voices(), 0);
            assert!(a.trigger(distance, 0.0));
        }
    }
}
