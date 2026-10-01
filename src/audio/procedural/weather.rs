//! Default squall-cell model adapted from NoiseMachine (MIT, 2026 kvmet).
//! This is local sound synthesis; it does not establish authoritative game weather.
use super::dsp::{Rng, SAMPLE_RATE};
#[derive(Clone, Copy, Default)]
pub(super) struct Weather {
    pub rain: f32,
    pub wind: f32,
    pub mean_wind: f32,
    pub bearing: f32,
    pub lightning: f32,
    pub distance: f32,
    pub angle: f32,
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
}
fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}
fn bell(x: f32, width: f32) -> f32 {
    (-x * x / (2.0 * width * width)).exp()
}
impl Storm {
    pub fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed, 0x78dd_e6e4);
        let a = std::f32::consts::TAU * rng.unit();
        let mut s = Self {
            rng,
            gust_rng: Rng::new(seed, 0x510e_527f),
            prevailing: [a.sin(), a.cos()],
            cells: [Cell::default(); 4],
            gust: 0.0,
        };
        let along = s.rng.between(-4000.0, 1000.0);
        s.spawn(0, along, 2000.0);
        s
    }
    fn spawn(&mut self, index: usize, along: f32, miss_limit: f32) {
        let a = self.prevailing[0].atan2(self.prevailing[1]) + 0.35 * self.rng.gaussian();
        let miss = self.rng.between(-miss_limit, miss_limit);
        let h = [a.sin(), a.cos()];
        self.cells[index] = Cell {
            active: true,
            position: [-along * h[0] + miss * h[1], -along * h[1] - miss * h[0]],
            heading: h,
            travelled: 40_000.0 - along,
            severity: self.rng.between(0.2, 0.8),
        };
    }
    pub fn tick(&mut self) -> Weather {
        // Preview time scale60: make a passing storm audible during a short test.
        let dt = 60.0 * 441.0 / SAMPLE_RATE;
        let step = 10.0 * dt;
        for c in &mut self.cells {
            if c.active {
                for axis in 0..2 {
                    c.position[axis] += step * c.heading[axis];
                }
                c.travelled += step;
                if c.travelled >= 80_000.0 {
                    c.active = false;
                }
            }
        }
        if self.rng.unit() < 0.5 * dt / 3600.0
            && let Some(index) = self.cells.iter().position(|c| !c.active)
        {
            self.spawn(index, 40_000.0, 10_000.0);
        }
        let keep = (-0.01f32 / 4.0).exp();
        self.gust = keep * self.gust + 0.3 * (1.0 - keep * keep).sqrt() * self.gust_rng.gaussian();
        let mut w = Weather::default();
        let mut wind = self.prevailing.map(|x| 2.0 * x);
        let mut nearest = f32::INFINITY;
        for c in &self.cells {
            if !c.active {
                continue;
            }
            let along = -(c.position[0] * c.heading[0] + c.position[1] * c.heading[1]);
            let across = c.position[0] * c.heading[1] - c.position[1] * c.heading[0];
            let u = c.travelled / 80_000.0;
            let stage = if u < 0.3 {
                smooth(u / 0.3)
            } else {
                smooth((1.0 - u) / 0.35)
            };
            let core = bell(along, 3000.0) * bell(across, 10_000.0);
            let tail = if along < 0.0 {
                0.1 * (1.0 - bell(along, 3000.0))
                    * (along / 15_000.0).exp()
                    * bell(across, 15_000.0)
            } else {
                0.0
            };
            w.rain += stage * 2.0 * 75.0f32.powf(c.severity) * (core + tail);
            let front = 4000.0 + 4000.0 * c.severity;
            let lead = if along > front {
                -((along - front) / 1500.0).powi(2)
            } else {
                0.0
            };
            let outflow = if along >= 0.0 {
                lead.exp()
            } else {
                (along / 6000.0).exp()
            };
            let speed = stage * (4.0 + 20.0 * c.severity) * outflow * bell(across, 12_000.0);
            let distance = c.position[0].hypot(c.position[1]).max(1.0);
            for (axis, v) in wind.iter_mut().enumerate() {
                *v -= speed * c.position[axis] / distance;
            }
            if distance < nearest {
                nearest = distance;
                w.distance = distance.max(200.0);
                w.angle = c.position[0].atan2(c.position[1]);
                w.lightning = stage * stage * (0.5 + 11.5 * c.severity * c.severity);
            }
        }
        w.rain = w.rain.min(200.0);
        w.mean_wind = wind[0].hypot(wind[1]).min(40.0);
        w.wind = (w.mean_wind * (1.0 + self.gust)).clamp(0.0, 40.0);
        w.bearing = (-wind[0]).atan2(-wind[1]);
        w
    }
}
