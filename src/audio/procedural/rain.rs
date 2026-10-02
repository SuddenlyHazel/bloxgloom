//! Material-specific rain adapted from NoiseMachine's `noise_rain.c`.
//! Copyright (c) 2026 kvmet. MIT licensed; see `third-party/NoiseMachine-LICENSE`.
//! Fixed 44,100 Hz; Marshall–Palmer arrivals, click/resonance/bubble voices,
//! wind-driven wall impacts, advected gust sheets and spectrum-matched far rain.

use crate::audio::rain_scene::{RAIN_MATERIALS, RainScene};
use std::f32::consts::{PI, TAU};
use std::sync::Arc;
mod scene;
use scene::SceneSampler;
const MAX_DROPS: usize = 128;
const SIZE_BINS: usize = 50;
const BED_BANDS: usize = 15;
const SHEET_HISTORY: usize = 512;

#[derive(Clone, Copy, Debug)]
pub(super) struct SurfaceMode {
    pub frequency_hz: f32,
    pub damping_per_s: f32,
    pub gain: f32,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Surface {
    pub name: &'static str,
    pub coverage: f32,
    pub gain: f32,
    pub vertical: bool,
    pub click_gain: [f32; 2],
    pub click_frequency_hz: [f32; 2],
    pub click_damping_ratio: f32,
    pub modes: [SurfaceMode; 2],
    pub detune: f32,
    pub lowpass_hz: f32,
    pub bubble_probability: f32,
    pub bubble_radius_m: [f32; 2],
    pub bubble_gain: [f32; 2],
    pub bubble_decay: [f32; 2],
    pub bubble_delay_s: f32,
}
impl Surface {
    const fn solid(
        name: &'static str,
        coverage: f32,
        click: f32,
        frequencies: [f32; 2],
        damping: [f32; 2],
        resonance: f32,
        lowpass: f32,
    ) -> Self {
        Self {
            name,
            coverage,
            gain: 1.0,
            vertical: false,
            click_gain: [click; 2],
            click_frequency_hz: [1000.0, 16000.0],
            click_damping_ratio: 2.0,
            modes: [
                SurfaceMode {
                    frequency_hz: frequencies[0],
                    damping_per_s: damping[0],
                    gain: resonance,
                },
                SurfaceMode {
                    frequency_hz: frequencies[1],
                    damping_per_s: damping[1],
                    gain: resonance * 0.5,
                },
            ],
            detune: 0.15,
            lowpass_hz: lowpass,
            bubble_probability: 0.0,
            bubble_radius_m: [0.00035, 0.0016],
            bubble_gain: [1.0; 2],
            bubble_decay: [1.0; 2],
            bubble_delay_s: 0.002,
        }
    }
}
#[derive(Clone, Debug)]
pub(super) struct RainConfig {
    pub gain: f32,
    pub max_drops_per_s: f32,
    pub bed_gain: f32,
    pub sheet_depth: f32,
    pub min_distance_m: f32,
    pub max_distance_m: f32,
    pub surfaces: [Surface; RAIN_MATERIALS],
}
impl Default for RainConfig {
    fn default() -> Self {
        let mut water = Surface::solid("Water", 0.37, 0.15, [1000.0; 2], [1000.0; 2], 0.0, 0.0);
        water.click_gain = [0.15, 0.5];
        water.detune = 0.0;
        water.bubble_probability = 0.85;
        water.bubble_gain = [1.2, 2.5];
        water.bubble_decay = [3.0, 8.0];
        Self {
            gain: 0.5,
            max_drops_per_s: 900.0,
            // Keep the continuous far-rain wash behind the discrete surface impacts.
            bed_gain: 0.25,
            sheet_depth: 1.0,
            min_distance_m: 0.75,
            max_distance_m: 5.0,
            surfaces: [
                water,
                Surface::solid(
                    "Dirt",
                    0.21,
                    1.0,
                    [450.0, 1100.0],
                    [1200.0, 1800.0],
                    0.35,
                    0.0,
                ),
                Surface::solid(
                    "Leaf",
                    0.26,
                    1.0,
                    [1800.0, 4200.0],
                    [800.0, 1400.0],
                    0.5,
                    0.0,
                ),
                Surface::solid(
                    "Concrete",
                    0.15,
                    1.0,
                    [1400.0, 3700.0],
                    [1400.0, 2200.0],
                    0.45,
                    0.0,
                ),
                Surface::solid(
                    "Glass",
                    0.005,
                    1.0,
                    [3200.0, 7100.0],
                    [160.0, 260.0],
                    0.325,
                    0.0,
                ),
                Surface::solid(
                    "Metal",
                    0.005,
                    1.0,
                    [1700.0, 4300.0],
                    [90.0, 150.0],
                    0.4,
                    0.0,
                ),
                Surface::solid(
                    "Plastic",
                    0.0,
                    0.5,
                    [220.0, 650.0],
                    [110.0, 220.0],
                    0.65,
                    1600.0,
                ),
                Surface::solid(
                    "Asphalt",
                    0.0,
                    0.3,
                    [300.0, 900.0],
                    [1600.0, 2600.0],
                    0.25,
                    0.0,
                ),
                Surface::solid(
                    "Asphalt roof",
                    0.0,
                    0.25,
                    [140.0, 420.0],
                    [300.0, 700.0],
                    0.4,
                    900.0,
                ),
                // Native wooden blocks need a less metallic, damped body.
                Surface::solid(
                    "Wood",
                    0.0,
                    0.65,
                    [320.0, 850.0],
                    [220.0, 480.0],
                    0.7,
                    4200.0,
                ),
            ],
        }
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct RainWeather {
    pub rain_mm_h: f32,
    pub wind_m_s: f32,
    pub wind_mean_m_s: f32,
    pub wind_bearing_rad: f32,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Droplet {
    pub surface: usize,
    pub radius_m: f32,
    pub velocity_m_s: f32,
    pub bubble_radius_m: f32,
    pub distance_m: f32,
    pub angle_rad: f32,
}
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct RainStats {
    pub generated: u64,
    pub dropped: u64,
    pub peak_active: usize,
}
fn terminal_speed(d: f32) -> f32 {
    0.01 * if d <= 1.4 {
        -17.8951 + d * (448.9498 + d * (16.3719 - 45.9516 * d))
    } else {
        24.166 + d * (448.8336 + d * (-75.6265 + 4.2695 * d))
    }
}
fn ring_power(r: f32) -> f32 {
    if r <= 1.0 { 0.5 * r * r } else { 0.5 + r.ln() }
}
fn diameter(cdf: &[f32; SIZE_BINS], choice: f32, offset: f32) -> f32 {
    let index = cdf
        .partition_point(|&value| value <= choice)
        .min(SIZE_BINS - 1);
    0.8 + 0.1 * (index as f32 + offset)
}

use super::dsp::{Biquad, Mode, Rng, SAMPLE_RATE};
use super::spatial::{Bus, Listener, Spatial};
struct DropVoice {
    modes: [Mode; 4],
    spatial: Spatial,
    distance_m: f32,
    world_angle_rad: f32,
    listener_yaw: f32,
    lowpass_alpha: f32,
    lowpass: [f32; 2],
    tail: u32,
}
struct Bed {
    rng: Rng,
    ratio: f32,
    analysis: [[[Biquad; 2]; BED_BANDS]; 2],
    synthesis: [[[Biquad; 2]; BED_BANDS]; 2],
    unit_power: [f32; BED_BANDS],
    plateau: f32,
    power: [[f32; BED_BANDS]; 2],
    gain: [[f32; BED_BANDS]; 2],
    gain_scale: [f32; BED_BANDS],
}
impl Bed {
    fn new(seed: u32) -> Self {
        let bank = std::array::from_fn(|_| {
            std::array::from_fn(|band| {
                [Biquad::bandpass(125.0 * std::f32::consts::SQRT_2.powi(band as i32), 1.414); 2]
            })
        });
        let mut result = Self {
            rng: Rng::new(seed, 0x9b05688c),
            ratio: 0.0,
            analysis: bank,
            synthesis: bank,
            unit_power: [0.0; BED_BANDS],
            plateau: 0.0,
            power: [[0.0; BED_BANDS]; 2],
            gain: [[0.0; BED_BANDS]; 2],
            gain_scale: [0.0; BED_BANDS],
        };
        for band in 0..BED_BANDS {
            let mut probe = result.analysis[0][band];
            for n in 0..8192 {
                let sample = band_next(&mut probe, if n == 0 { 1.0 } else { 0.0 });
                result.unit_power[band] += sample * sample;
            }
        }
        let high = 125.0 * std::f32::consts::SQRT_2.powi(BED_BANDS as i32 - 3);
        for point in 0..64 {
            let frequency = 250.0 * (high / 250.0).powf(point as f32 / 63.0);
            for band in &result.analysis[0] {
                result.plateau += band
                    .iter()
                    .map(|filter| filter.power(frequency))
                    .product::<f32>()
                    / 64.0;
            }
        }
        result
    }
    fn next(&mut self, played: [f32; 2]) -> [f32; 2] {
        std::array::from_fn(|ear| {
            let mut sum = 0.0;
            for band in 0..BED_BANDS {
                let sample = band_next(&mut self.analysis[ear][band], played[ear]);
                self.power[ear][band] +=
                    (sample * sample - self.power[ear][band]) / (0.5 * SAMPLE_RATE);
                let noise = 2.0 * self.rng.unit() - 1.0;
                sum += self.gain[ear][band] * band_next(&mut self.synthesis[ear][band], noise);
            }
            sum
        })
    }
}
fn band_next(filters: &mut [Biquad; 2], mut sample: f32) -> f32 {
    for filter in filters {
        sample = filter.next(sample);
    }
    sample
}

pub(super) struct Rain {
    config: RainConfig,
    scene: SceneSampler,
    weather: RainWeather,
    arrival_rng: Rng,
    drop_rng: Rng,
    surface_cdf: [f32; RAIN_MATERIALS],
    rain_mm_h: f32,
    flux: f32,
    concentration: f32,
    size_cdf: [f32; SIZE_BINS],
    wall_size_cdf: [f32; SIZE_BINS],
    played_per_s: f32,
    arrival_probability: f32,
    near_m: f32,
    sheet_entries_per_m: f32,
    sheet_peak: f32,
    sheet: [f32; SHEET_HISTORY],
    sheet_next: usize,
    sheet_step: u32,
    bus: Bus,
    bed: Bed,
    voices: Vec<DropVoice>,
    stats: RainStats,
}
impl Rain {
    pub(super) fn new(seed: u32) -> Self {
        Self {
            config: RainConfig::default(),
            scene: SceneSampler::default(),
            weather: RainWeather::default(),
            arrival_rng: Rng::new(seed, 0x3c6ef372),
            drop_rng: Rng::new(seed, 0xdaa66d2b),
            surface_cdf: [0.0; RAIN_MATERIALS],
            rain_mm_h: -1.0,
            flux: 0.0,
            concentration: 0.0,
            size_cdf: [0.0; SIZE_BINS],
            wall_size_cdf: [0.0; SIZE_BINS],
            played_per_s: 0.0,
            arrival_probability: 0.0,
            near_m: 0.0,
            sheet_entries_per_m: 0.0,
            sheet_peak: 1.0,
            sheet: [0.0; SHEET_HISTORY],
            sheet_next: 0,
            sheet_step: 0,
            bus: Bus::default(),
            bed: Bed::new(seed),
            voices: Vec::with_capacity(MAX_DROPS),
            stats: RainStats::default(),
        }
    }
    pub(super) fn configure(&mut self, config: RainConfig) -> Result<(), &'static str> {
        config.validate()?;
        self.config = config;
        self.follow_rates();
        Ok(())
    }
    pub(super) fn set_scene(&mut self, scene: Option<Arc<RainScene>>) {
        self.scene.scene = scene;
        self.follow_rates();
    }
    pub(super) fn set_listener(&mut self, position: [f32; 3], yaw: f32) {
        self.scene.position = position;
        self.scene.yaw = yaw;
    }
    pub(super) fn stats(&self) -> RainStats {
        self.stats
    }
    pub(super) fn active_voices(&self) -> usize {
        self.voices.len()
    }
    /// Call once per 100 Hz weather update, including during dry weather.
    pub(super) fn follow(&mut self, weather: RainWeather) -> Result<(), &'static str> {
        if !range(weather.rain_mm_h, 0.0, 500.0)
            || !range(weather.wind_m_s, 0.0, 100.0)
            || !range(weather.wind_mean_m_s, 0.0, 100.0)
            || !weather.wind_bearing_rad.is_finite()
        {
            return Err("invalid rain weather");
        }
        self.weather = weather;
        self.follow_rates();
        self.sheet_step += 1;
        if self.sheet_step == 10 {
            self.sheet_step = 0;
            self.sheet[self.sheet_next] = if weather.wind_mean_m_s > 0.0 {
                (weather.wind_m_s / weather.wind_mean_m_s).min(1.0e6) - 1.0
            } else {
                0.0
            };
            self.sheet_next = (self.sheet_next + 1) % SHEET_HISTORY;
            self.set_arrival_rate();
        }
        for band in 0..BED_BANDS {
            // Match the combined spectrum without inheriting nearby drops' direction.
            // Independent noise in each ear keeps the bed diffuse rather than mono.
            let power = 0.5 * (self.bed.power[0][band] + self.bed.power[1][band]);
            let gain = (self.bed.gain_scale[band] * power).sqrt();
            self.bed.gain[0][band] = gain;
            self.bed.gain[1][band] = gain;
        }
        Ok(())
    }
    fn follow_rates(&mut self) {
        if self.rain_mm_h != self.weather.rain_mm_h {
            self.build_sizes(self.weather.rain_mm_h);
        }
        let wall_rate = if self.flux > 0.0 {
            self.weather.wind_m_s * self.concentration / self.flux
        } else {
            0.0
        };
        let mut coverage = 0.0;
        let mut weight = 0.0;
        for (index, surface) in self.config.surfaces.iter().enumerate() {
            coverage += surface.coverage;
            weight += surface.coverage * if surface.vertical { wall_rate } else { 1.0 };
            self.surface_cdf[index] = weight;
        }
        if weight > 0.0 {
            for value in &mut self.surface_cdf {
                *value /= weight;
            }
        }
        self.scene.rebuild(self.weather, wall_rate);
        let hits = if self.scene.scene.is_some() {
            self.flux * self.scene.area
                / (PI * (self.config.max_distance_m.powi(2) - self.config.min_distance_m.powi(2)))
                    .max(1.0)
        } else {
            self.flux * weight / coverage
        };
        let near = self.config.min_distance_m;
        let far = self.config.max_distance_m;
        let arrivals = hits * PI * (far * far - near * near);
        self.played_per_s = arrivals.min(self.config.max_drops_per_s);
        self.near_m = far;
        self.bed.ratio = 0.0;
        if self.played_per_s > 0.0 && self.played_per_s < arrivals {
            self.near_m = (near * near + self.played_per_s / (PI * hits))
                .sqrt()
                .min(far);
            let played_power = ring_power(self.near_m) - ring_power(near);
            // An extremely small rate can round the played ring to zero width.
            // There is no measured near spectrum to amplify in that case.
            if played_power > 0.0 {
                self.bed.ratio = (ring_power(far) - ring_power(self.near_m)) / played_power;
            }
        }
        self.sheet_entries_per_m = 10.0 / self.weather.wind_mean_m_s.max(0.01);
        self.set_arrival_rate();
        for band in 0..BED_BANDS {
            self.bed.gain_scale[band] =
                self.bed.ratio / ((1.0 / 3.0) * self.bed.unit_power[band] * self.bed.plateau);
        }
    }
    fn build_sizes(&mut self, rain: f32) {
        self.rain_mm_h = rain;
        if rain <= 0.0 {
            self.flux = 0.0;
            self.concentration = 0.0;
            return;
        }
        let lambda = 4.1 * rain.powf(-0.21);
        let mut flux = 0.0;
        let mut count = 0.0;
        for index in 0..SIZE_BINS {
            let d = 0.8 + 0.1 * (index as f32 + 0.5);
            let n = (-lambda * d).exp();
            flux += n * terminal_speed(d);
            count += n;
            self.size_cdf[index] = flux;
            self.wall_size_cdf[index] = count;
        }
        if flux == 0.0 || count == 0.0 {
            self.flux = 0.0;
            self.concentration = 0.0;
            return;
        }
        for index in 0..SIZE_BINS {
            self.size_cdf[index] /= flux;
            self.wall_size_cdf[index] /= count;
        }
        self.size_cdf[SIZE_BINS - 1] = 1.0;
        self.wall_size_cdf[SIZE_BINS - 1] = 1.0;
        self.flux = 8000.0 * 0.1 * flux;
        self.concentration = 8000.0 * 0.1 * count;
    }
    fn sheet_factor(&self, age: f32) -> f32 {
        let age = age.clamp(0.0, (SHEET_HISTORY - 1) as f32);
        let whole = age as usize;
        let newer = (self.sheet_next + SHEET_HISTORY - 1 - whole) % SHEET_HISTORY;
        let older = (newer + SHEET_HISTORY - 1) % SHEET_HISTORY;
        let gust = self.sheet[newer] + age.fract() * (self.sheet[older] - self.sheet[newer]);
        (1.0 + self.config.sheet_depth * gust).max(0.0)
    }
    fn set_arrival_rate(&mut self) {
        self.sheet_peak = 1.0;
        if self.config.sheet_depth > 0.0 {
            let entries = ((2.0 * self.near_m * self.sheet_entries_per_m).ceil() as usize)
                .saturating_add(1)
                .min(SHEET_HISTORY);
            self.sheet_peak = (0..entries)
                .map(|age| self.sheet_factor(age as f32))
                .fold(0.0, f32::max);
        }
        self.arrival_probability = (self.played_per_s / SAMPLE_RATE) * self.sheet_peak;
    }
    #[cfg(test)]
    pub(super) fn start_drop(
        &mut self,
        drop: Droplet,
        listener: Listener,
    ) -> Result<(), &'static str> {
        self.start_profiled_drop(drop, listener, None)
    }
    fn start_profiled_drop(
        &mut self,
        drop: Droplet,
        listener: Listener,
        impact: Option<bloxgloom_host_api::content::ImpactProfile>,
    ) -> Result<(), &'static str> {
        if drop.surface >= RAIN_MATERIALS
            || !range(drop.radius_m, 0.0004, 0.0029)
            || !range(drop.velocity_m_s, 0.0, 40.0)
            || !range(drop.distance_m, 0.25, 100.0)
            || !range(drop.angle_rad, -TAU, TAU)
            || !(drop.bubble_radius_m == 0.0 || range(drop.bubble_radius_m, 0.00016, 0.004))
        {
            return Err("invalid rain droplet");
        }
        if drop.velocity_m_s == 0.0 {
            return Ok(());
        }
        if self.voices.len() == MAX_DROPS {
            self.stats.dropped = self.stats.dropped.saturating_add(1);
            return Err("rain voice capacity");
        }
        let surface = impact.map_or(self.config.surfaces[drop.surface], custom_surface);
        let radius_ratio = drop.radius_m / 0.0005;
        let amplitude = surface.gain
            * 0.004375
            * (radius_ratio * radius_ratio * radius_ratio).sqrt()
            * drop.velocity_m_s
            / 4.0;
        let click_gain = sample_range(&mut self.drop_rng, surface.click_gain);
        let click_frequency = sample_range(&mut self.drop_rng, surface.click_frequency_hz);
        let mut modes = [Mode::default(); 4];
        modes[0] = Mode::new(
            click_frequency,
            surface.click_damping_ratio * click_frequency,
            amplitude * click_gain,
            0,
        );
        if surface.modes.iter().any(|mode| mode.gain > 0.0) {
            let tuning = sample_range(
                &mut self.drop_rng,
                [1.0 - surface.detune, 1.0 + surface.detune],
            );
            for (index, mode) in surface.modes.iter().enumerate() {
                if mode.gain > 0.0 {
                    modes[index + 1] = Mode::new(
                        mode.frequency_hz * tuning,
                        mode.damping_per_s,
                        amplitude * mode.gain,
                        0,
                    );
                }
            }
        }
        if drop.bubble_radius_m > 0.0 {
            let r = drop.bubble_radius_m;
            let frequency = (3.0_f32 * 1.4 * 101_325.0 / 1000.0).sqrt() / (TAU * r);
            let damping = 0.13 / r + 0.0072 / (r * r.sqrt());
            let decay = sample_range(&mut self.drop_rng, surface.bubble_decay);
            let gain = sample_range(&mut self.drop_rng, surface.bubble_gain);
            modes[3] = Mode::new(
                frequency,
                damping / decay,
                gain * click_gain * amplitude,
                (surface.bubble_delay_s * SAMPLE_RATE) as u32,
            );
        }
        let lowpass_alpha = if surface.lowpass_hz > 0.0 {
            -(-TAU * surface.lowpass_hz / SAMPLE_RATE).exp_m1()
        } else {
            1.0
        };
        self.voices.push(DropVoice {
            modes,
            spatial: Spatial::new(drop.distance_m, drop.angle_rad, listener),
            distance_m: drop.distance_m,
            world_angle_rad: drop.angle_rad + self.scene.yaw,
            listener_yaw: self.scene.yaw,
            lowpass_alpha,
            lowpass: [0.0; 2],
            tail: 256,
        });
        self.stats.generated = self.stats.generated.saturating_add(1);
        self.stats.peak_active = self.stats.peak_active.max(self.voices.len());
        Ok(())
    }
    fn spawn(&mut self, listener: Listener) {
        let choice = self.drop_rng.unit();
        let offset = self.drop_rng.unit();
        let surface_choice = self.drop_rng.unit();
        let tile = if self.scene.scene.is_some() {
            let Some(tile) = self.scene.choose(surface_choice) else {
                return;
            };
            Some(tile)
        } else {
            None
        };
        let surface_index = tile.map_or_else(
            || {
                self.surface_cdf
                    .partition_point(|&cdf| cdf <= surface_choice)
                    .min(RAIN_MATERIALS - 1)
            },
            |tile| tile.material as usize,
        );
        let vertical = tile.map_or(self.config.surfaces[surface_index].vertical, |t| {
            t.normal != [0.0; 2]
        });
        let impact = tile.and_then(|t| t.impact);
        let surface = impact.map_or(self.config.surfaces[surface_index], custom_surface);
        let d = diameter(
            if vertical {
                &self.wall_size_cdf
            } else {
                &self.size_cdf
            },
            choice,
            offset,
        );
        let bubble = if surface.bubble_probability > 0.0
            && self.drop_rng.unit() < surface.bubble_probability
        {
            if surface.bubble_radius_m[0] < surface.bubble_radius_m[1] {
                self.drop_rng
                    .log_between(surface.bubble_radius_m[0], surface.bubble_radius_m[1])
            } else {
                surface.bubble_radius_m[0]
            }
        } else {
            0.0
        };
        let near = self.config.min_distance_m;
        let (distance, angle) = if let Some(tile) = tile {
            self.scene
                .polar(tile, self.drop_rng.unit(), self.drop_rng.unit())
        } else {
            (
                (near * near + self.drop_rng.unit() * (self.near_m * self.near_m - near * near))
                    .sqrt(),
                TAU * self.drop_rng.unit(),
            )
        };
        if self.config.sheet_depth > 0.0 {
            let upwind = distance * (angle - self.weather.wind_bearing_rad).cos();
            if self.drop_rng.unit() * self.sheet_peak
                >= self.sheet_factor((self.near_m - upwind) * self.sheet_entries_per_m)
            {
                return;
            }
        }
        let _ = self.start_profiled_drop(
            Droplet {
                surface: surface_index,
                radius_m: d * 0.0005,
                velocity_m_s: if vertical {
                    self.weather.wind_m_s
                } else {
                    terminal_speed(d)
                },
                bubble_radius_m: bubble,
                distance_m: distance,
                angle_rad: angle,
            },
            listener,
            impact,
        );
    }
    pub(super) fn next(&mut self, listener: Listener) -> ([f32; 2], f32) {
        if self.config.gain > 0.0
            && self.arrival_probability > 0.0
            && self.arrival_rng.unit() < self.arrival_probability
        {
            self.spawn(listener);
        }
        let gain = 1.175 * self.config.gain;
        let mut send = 0.0;
        let mut index = 0;
        while index < self.voices.len() {
            let voice = &mut self.voices[index];
            if voice.listener_yaw != self.scene.yaw {
                voice.spatial.retarget(
                    voice.distance_m,
                    voice.world_angle_rad - self.scene.yaw,
                    listener,
                );
                voice.listener_yaw = self.scene.yaw;
            }
            let mut source = 0.0;
            for mode in &mut voice.modes {
                source += mode.next();
            }
            let active = voice.modes.iter().any(Mode::active);
            if voice.lowpass_alpha < 1.0 {
                for state in &mut voice.lowpass {
                    *state += voice.lowpass_alpha * (source - *state);
                    source = *state;
                }
            }
            send += gain * voice.spatial.emit(listener, &mut self.bus, source);
            if !active {
                voice.tail = voice.tail.saturating_sub(1);
            }
            if !active && voice.tail == 0 {
                self.voices.swap_remove(index);
            } else {
                index += 1;
            }
        }
        let played = self.bus.next();
        let mut direct = played.map(|value| gain * value);
        if self.bed.ratio > 0.0 && self.config.bed_gain > 0.0 {
            let bed = self.bed.next(played);
            for ear in 0..2 {
                direct[ear] += gain * self.config.bed_gain * bed[ear];
            }
        }
        (direct, send)
    }
}
fn sample_range(rng: &mut Rng, bounds: [f32; 2]) -> f32 {
    if bounds[0] < bounds[1] {
        rng.between(bounds[0], bounds[1])
    } else {
        bounds[0]
    }
}
fn range(value: f32, low: f32, high: f32) -> bool {
    value.is_finite() && (low..=high).contains(&value)
}
impl RainConfig {
    fn validate(&self) -> Result<(), &'static str> {
        if !range(self.gain, 0.0, 4.0)
            || !range(self.max_drops_per_s, 0.0, 2000.0)
            || !range(self.bed_gain, 0.0, 4.0)
            || !range(self.sheet_depth, 0.0, 2.0)
            || !range(self.min_distance_m, 0.25, 100.0)
            || !range(self.max_distance_m, self.min_distance_m, 100.0)
        {
            return Err("invalid rain configuration");
        }
        let mut coverage = 0.0;
        for s in &self.surfaces {
            if s.name.len() >= 16
                || !range(s.coverage, 0.0, 1000.0)
                || !range(s.gain, 0.0, 4.0)
                || !range(s.click_gain[0], 0.0, 2.0)
                || !range(s.click_gain[1], s.click_gain[0], 2.0)
                || !range(s.click_frequency_hz[0], 20.0, 20000.0)
                || !range(s.click_frequency_hz[1], s.click_frequency_hz[0], 20000.0)
                || !range(s.click_damping_ratio, 0.05, 50.0)
                || !range(s.detune, 0.0, 0.5)
                || !(s.lowpass_hz == 0.0 || range(s.lowpass_hz, 20.0, 20000.0))
                || !range(s.bubble_probability, 0.0, 1.0)
                || !range(s.bubble_radius_m[0], 0.00016, 0.004)
                || !range(s.bubble_radius_m[1], s.bubble_radius_m[0], 0.004)
                || !range(s.bubble_gain[0], 0.0, 8.0)
                || !range(s.bubble_gain[1], s.bubble_gain[0], 8.0)
                || !range(s.bubble_decay[0], 0.25, 20.0)
                || !range(s.bubble_decay[1], s.bubble_decay[0], 20.0)
                || !range(s.bubble_delay_s, 0.0, 0.1)
                || s.modes.iter().any(|m| {
                    !range(m.frequency_hz, 20.0, 20000.0)
                        || !range(m.damping_per_s, 1.0, 20000.0)
                        || !range(m.gain, 0.0, 4.0)
                })
            {
                return Err("invalid rain surface");
            }
            coverage += s.coverage;
        }
        if coverage <= 0.0 {
            return Err("rain surface coverage is empty");
        }
        Ok(())
    }
}

fn custom_surface(p: bloxgloom_host_api::content::ImpactProfile) -> Surface {
    let mut surface = Surface::solid(
        "Custom",
        0.0,
        p.click,
        p.frequency_hz,
        p.damping_per_s,
        p.resonance,
        p.lowpass_hz,
    );
    surface.gain = p.gain;
    surface
}

#[cfg(test)]
mod tests;
