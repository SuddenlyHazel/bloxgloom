//! Configurable local squall-cell synthesis adapted from NoiseMachine (MIT, 2026 kvmet).
//! This does not establish authoritative game weather.
use super::dsp::{Rng, SAMPLE_RATE};
use crate::audio::rain_tuning::{Preview, StormShape};
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub(super) struct Weather {
    pub rain: f32,
    pub wind: f32,
    pub mean_wind: f32,
    pub bearing: f32,
    pub lightning: f32,
    pub distance: f32,
    pub angle: f32,
    pub temperature: f32,
}
#[derive(Clone, Copy, Default)]
struct Cell {
    active: bool,
    position: [f32; 2],
    heading: [f32; 2],
    travelled: f32,
    severity: f32,
}
pub(super) struct Storm {
    rng: Rng,
    gust_rng: Rng,
    prevailing: [f32; 2],
    cells: [Cell; 4],
    gust: f32,
    config: Preview,
    temperature: f32,
}
fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}
fn bell(x: f32, width: f32) -> f32 {
    (-x * x / (2.0 * width * width)).exp()
}
fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
fn front(shape: StormShape, along: f32, severity: f32) -> f32 {
    let front = mix(shape.front_min_m, shape.front_max_m, severity);
    if along > front {
        (-((along - front) / shape.front_edge_m).powi(2)).exp()
    } else {
        1.0
    }
}
impl Storm {
    pub fn new(seed: u32) -> Self {
        Self::with_config(seed, Preview::default())
    }
    pub fn with_config(seed: u32, config: Preview) -> Self {
        let mut rng = Rng::new(seed, 0x78dd_e6e4);
        let a = std::f32::consts::TAU * rng.unit();
        let mut s = Self {
            rng,
            gust_rng: Rng::new(seed, 0x510e_527f),
            prevailing: [a.sin(), a.cos()],
            cells: [Cell::default(); 4],
            gust: 0.0,
            config,
            temperature: config.climate.temperature_c,
        };
        if config.climate.storms_per_hour > 0.0 {
            let along = s.rng.between(-4000.0, 1000.0);
            s.spawn(0, along, config.shape.miss_m.min(2000.0));
        }
        s
    }
    pub fn configure(&mut self, config: Preview) {
        self.temperature += config.climate.temperature_c - self.config.climate.temperature_c;
        // Existing cells keep their positions; severity limits apply immediately.
        for cell in &mut self.cells {
            cell.severity = cell
                .severity
                .clamp(config.climate.min_severity, config.climate.max_severity);
        }
        self.config = config;
    }
    fn spawn(&mut self, index: usize, along: f32, miss_limit: f32) {
        let a = self.prevailing[0].atan2(self.prevailing[1])
            + self.config.shape.heading_spread_rad * self.rng.gaussian();
        let miss = self.rng.between(-miss_limit, miss_limit);
        let h = [a.sin(), a.cos()];
        self.cells[index] = Cell {
            active: true,
            position: [-along * h[0] + miss * h[1], -along * h[1] - miss * h[0]],
            heading: h,
            travelled: self.config.shape.approach_m - along,
            severity: self.rng.between(
                self.config.climate.min_severity,
                self.config.climate.max_severity,
            ),
        };
    }
    pub fn fixed_tick(&mut self) -> Weather {
        self.gust_tick();
        let f = self.config.fixed;
        Weather {
            rain: f.rain_mm_h,
            wind: (f.wind_m_s * (1.0 + self.gust)).clamp(0.0, 40.0),
            mean_wind: f.wind_m_s,
            bearing: f.wind_bearing_rad,
            temperature: f.temperature_c,
            lightning: f.lightning_per_min,
            distance: f.distance_m,
            angle: f.angle_rad,
        }
    }
    fn gust_tick(&mut self) {
        let keep = (-0.01 / self.config.climate.gust_time_s).exp();
        self.gust = keep * self.gust
            + self.config.climate.gust_intensity
                * (1.0 - keep * keep).sqrt()
                * self.gust_rng.gaussian();
    }
    pub fn tick(&mut self) -> Weather {
        if self.config.manual {
            return self.fixed_tick();
        }
        let climate = self.config.climate;
        let s = self.config.shape;
        let dt = climate.time_scale * 441.0 / SAMPLE_RATE;
        let step = climate.cell_speed_m_s * dt;
        for c in &mut self.cells {
            if c.active {
                for axis in 0..2 {
                    c.position[axis] += step * c.heading[axis];
                }
                c.travelled += step;
                if c.travelled >= 2.0 * s.approach_m {
                    c.active = false;
                }
            }
        }
        if self.rng.unit() < climate.storms_per_hour * dt / 3600.0
            && let Some(i) = self.cells.iter().position(|c| !c.active)
        {
            self.spawn(i, s.approach_m, s.miss_m);
        }
        self.gust_tick();
        let mut w = Weather::default();
        let mut wind = self.prevailing.map(|x| climate.breeze_m_s * x);
        let mut nearest = f32::INFINITY;
        let mut cooling = 0.0f32;
        for c in &self.cells {
            if !c.active {
                continue;
            }
            let along = -(c.position[0] * c.heading[0] + c.position[1] * c.heading[1]);
            let across = c.position[0] * c.heading[1] - c.position[1] * c.heading[0];
            let u = c.travelled / (2.0 * s.approach_m);
            let stage = if u < s.build_share {
                smooth(u / s.build_share)
            } else {
                smooth((1.0 - u) / s.decay_share)
            };
            let core = bell(along, s.core_along_m) * bell(across, s.core_across_m);
            let tail = if along < 0.0 {
                s.tail_share
                    * (1.0 - bell(along, s.core_along_m))
                    * (along / s.tail_length_m).exp()
                    * bell(across, s.tail_width_m)
            } else {
                0.0
            };
            w.rain += stage
                * s.peak_rain_min_mm_h
                * (s.peak_rain_max_mm_h / s.peak_rain_min_mm_h).powf(c.severity)
                * (core + tail);
            let lead = front(s, along, c.severity);
            let outflow = if along >= 0.0 {
                lead
            } else {
                (along / s.outflow_decay_m).exp()
            };
            let speed = stage
                * mix(s.outflow_min_m_s, s.outflow_max_m_s, c.severity)
                * outflow
                * bell(across, s.outflow_width_m);
            let distance = c.position[0].hypot(c.position[1]).max(1.0);
            for (axis, v) in wind.iter_mut().enumerate() {
                *v -= speed * c.position[axis] / distance;
            }
            let cool = if along > mix(s.front_min_m, s.front_max_m, c.severity) {
                lead
            } else {
                (along.min(0.0) / s.cooling_decay_m).exp()
            };
            cooling = cooling.max(
                stage
                    * mix(s.cooling_min_c, s.cooling_max_c, c.severity)
                    * cool
                    * bell(across, s.cooling_width_m),
            );
            if distance < nearest {
                nearest = distance;
                w.distance = distance.max(200.0);
                w.angle = c.position[0].atan2(c.position[1]);
                w.lightning = stage
                    * stage
                    * mix(
                        s.lightning_min_per_min,
                        s.lightning_max_per_min,
                        c.severity * c.severity,
                    );
            }
        }
        let target = climate.temperature_c - cooling;
        let time = if target < self.temperature {
            s.cooling_s
        } else {
            s.warming_s
        };
        self.temperature += -(-dt / time).exp_m1() * (target - self.temperature);
        w.temperature = self.temperature.clamp(-10.0, 45.0);
        w.rain = w.rain.min(200.0);
        w.mean_wind = wind[0].hypot(wind[1]).min(40.0);
        w.wind = (w.mean_wind * (1.0 + self.gust)).clamp(0.0, 40.0);
        w.bearing = (-wind[0]).atan2(-wind[1]);
        w
    }
}
#[cfg(test)]
mod tests;
