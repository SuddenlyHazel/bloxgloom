//! Rust weather-synthesis subset of NoiseMachine. No C runtime dependency.
mod dsp;
mod rain;
mod reverb;
mod spatial;
mod thunder;
mod weather;
mod wind;
use crate::audio::Preset;
use dsp::Rng;
use rain::{Rain, RainWeather};
use reverb::Reverb;
use spatial::Listener;
use thunder::Thunder;
use weather::{Storm, Weather};
use wind::Wind;
pub(super) struct Procedural {
    rain: Rain,
    wind: Wind,
    storm: Storm,
    thunder: Thunder,
    reverb: Reverb,
    weather: Weather,
    lightning: Rng,
    lightning_started: bool,
    frame: u32,
    seed: u32,
}
impl Procedural {
    pub fn new(seed: u32) -> Self {
        Self {
            rain: Rain::new(seed),
            wind: Wind::new(seed),
            storm: Storm::new(seed),
            thunder: Thunder::new(seed),
            reverb: Reverb::new([739, 953, 1151, 1327, 1471, 1663], 44_100.0, 0.65, 0.16),
            weather: Weather::default(),
            lightning: Rng::new(seed, 0xa54f_f53a),
            lightning_started: false,
            frame: 0,
            seed,
        }
    }
    pub fn stats(&self) -> (u64, u64, usize, usize, u64) {
        let r = self.rain.stats();
        (
            r.generated,
            r.dropped,
            self.rain.active_voices(),
            self.thunder.active_voices(),
            self.thunder.rejected(),
        )
    }
    pub fn set_preset(&mut self, _preset: Preset) {
        self.rain = Rain::new(self.seed);
        self.rain
            .configure(rain::RainConfig::default())
            .expect("valid native rain profile");
        self.wind = Wind::new(self.seed);
        self.storm = Storm::new(self.seed);
        self.reverb = Reverb::new([739, 953, 1151, 1327, 1471, 1663], 44_100.0, 0.65, 0.16);
        self.frame = 0;
        self.lightning_started = self.thunder.active_voices() != 0;
    }
    fn follow(&mut self, preset: Preset) {
        self.weather = match preset {
            Preset::Storm => self.storm.tick(),
            Preset::Rain => Weather {
                rain: 10.0,
                wind: 3.0,
                mean_wind: 3.0,
                ..Weather::default()
            },
            Preset::Wind => Weather {
                wind: 10.0,
                mean_wind: 10.0,
                ..Weather::default()
            },
            Preset::Off => Weather::default(),
        };
        let w = self.weather;
        // Values come from bounded native presets, not unvalidated author input.
        self.rain
            .follow(RainWeather {
                rain_mm_h: w.rain,
                wind_m_s: w.wind,
                wind_mean_m_s: w.mean_wind,
                wind_bearing_rad: w.bearing,
            })
            .expect("bounded native weather");
        self.wind.follow(w.wind, w.bearing);
    }
    pub fn trigger_thunder(&mut self, distance: f32, angle: f32) -> bool {
        let accepted = self.thunder.trigger(distance, angle);
        self.lightning_started |= accepted;
        accepted
    }
    pub fn next(&mut self, preset: Preset) -> ([f32; 2], [f32; 2]) {
        // Manual thunder remains available with ambient preview disabled.
        let thunder = self.thunder.next();
        if preset == Preset::Off {
            return ([0.0; 2], thunder);
        }
        if self.frame == 0 {
            self.follow(preset);
        }
        self.frame = (self.frame + 1) % 441;
        if preset == Preset::Storm && self.weather.lightning > 0.0 {
            let strike = !self.lightning_started
                || lightning_hit(self.lightning.next_u32(), self.weather.lightning);
            self.lightning_started = true;
            if strike {
                let x = self.weather.distance * self.weather.angle.sin()
                    + 4000.0 * self.lightning.gaussian();
                let y = self.weather.distance * self.weather.angle.cos()
                    + 4000.0 * self.lightning.gaussian();
                let distance = x.hypot(y).max(200.0);
                if distance <= 15_000.0 {
                    self.thunder.trigger(distance, x.atan2(y));
                }
            }
        }
        let (rain, send) = self.rain.next(Listener::default());
        let wind = self.wind.next();
        let wet = self.reverb.next(send);
        (
            std::array::from_fn(|i| rain[i] + wind[i] + 0.12 * wet[i]),
            thunder,
        )
    }
}

fn lightning_hit(draw: u32, rate_per_min: f32) -> bool {
    let threshold = (f64::from(rate_per_min) / (60.0 * 44_100.0) * 4_294_967_296.0) as u32;
    draw < threshold
}
#[cfg(test)]
#[path = "procedural/tests.rs"]
mod tests;
