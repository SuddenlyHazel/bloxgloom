//! Rust weather-synthesis subset of NoiseMachine. No C runtime dependency.
mod dsp;
mod insects;
mod rain;
mod reverb;
mod spatial;
mod thunder;
mod weather;
mod wind;
use crate::audio::{Preset, WeatherSound, rain_scene::RainScene};
use dsp::Rng;
use rain::{Rain, RainWeather};
use reverb::Reverb;
use spatial::Listener;
use std::sync::Arc;
use thunder::Thunder;
use weather::{Storm, Weather};
use wind::Wind;
pub(super) struct Procedural {
    rain: Rain,
    rain_config: crate::audio::rain_tuning::RainConfig,
    insects: insects::Insects,
    wind: Wind,
    storm: Storm,
    thunder: Thunder,
    reverb: Reverb,
    weather: Weather,
    lightning: Rng,
    lightning_started: bool,
    frame: u32,
    seed: u32,
    world: Option<WeatherSound>,
    scene: Arc<RainScene>,
    listener_position: [f32; 3],
    listener_yaw: f32,
    world_current: WeatherSound,
    indoor_filter: [f32; 2],
    exposure: f32,
}
impl Procedural {
    pub fn new(seed: u32) -> Self {
        Self {
            rain: Rain::new(seed),
            rain_config: Default::default(),
            insects: insects::Insects::new(seed),
            wind: Wind::new(seed),
            storm: Storm::new(seed),
            thunder: Thunder::new(seed),
            reverb: Reverb::new([739, 953, 1151, 1327, 1471, 1663], 44_100.0, 0.65, 0.16),
            weather: Weather::default(),
            lightning: Rng::new(seed, 0xa54f_f53a),
            lightning_started: false,
            frame: 0,
            seed,
            world: None,
            scene: Arc::new(RainScene::default()),
            listener_position: [0.0; 3],
            listener_yaw: 0.0,
            world_current: WeatherSound::default(),
            indoor_filter: [0.0; 2],
            exposure: 1.0,
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
            .configure(self.rain_config)
            .expect("valid native rain profile");
        self.rain
            .set_listener(self.listener_position, self.listener_yaw);
        self.rain.set_scene(self.world.map(|_| self.scene.clone()));
        self.wind = Wind::new(self.seed);
        self.storm = Storm::new(self.seed);
        self.reverb = Reverb::new([739, 953, 1151, 1327, 1471, 1663], 44_100.0, 0.65, 0.16);
        self.frame = 0;
        self.lightning_started = self.thunder.active_voices() != 0;
    }
    pub fn set_world(&mut self, weather: Option<WeatherSound>) {
        self.world = weather.map(WeatherSound::sanitized);
        self.rain.set_scene(self.world.map(|_| self.scene.clone()));
    }
    pub fn set_rain_config(&mut self, config: crate::audio::rain_tuning::RainConfig) {
        self.rain_config = config.sanitized();
        self.rain
            .configure(self.rain_config)
            .expect("sanitized rain profile");
    }
    pub fn set_scene(&mut self, scene: Arc<RainScene>) -> bool {
        if !scene.valid() {
            return false;
        }
        self.insects.set_scene(&scene);
        self.scene = scene;
        if self.world.is_some() {
            self.rain.set_scene(Some(self.scene.clone()));
        }
        true
    }
    pub fn set_listener(&mut self, position: [f32; 3], yaw: f32) {
        self.listener_position = position;
        self.listener_yaw = yaw;
        self.rain.set_listener(position, yaw);
    }
    pub fn world_active(&self) -> bool {
        self.world.is_some()
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
        if let Some(target) = self.world {
            // 100 Hz control clock: half-second convergence, independent of blocks.
            self.world_current.rain_mm_h +=
                (target.rain_mm_h - self.world_current.rain_mm_h) * 0.02;
            self.world_current.wind_m_s += (target.wind_m_s - self.world_current.wind_m_s) * 0.02;
            let delta = (target.bearing - self.world_current.bearing + std::f32::consts::PI)
                .rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            self.world_current.bearing += delta * 0.02;
            self.weather = Weather {
                rain: self.world_current.rain_mm_h,
                wind: self.world_current.wind_m_s,
                mean_wind: self.world_current.wind_m_s,
                bearing: self.world_current.bearing,
                ..Weather::default()
            };
        }
        self.rain
            .set_listener(self.listener_position, self.listener_yaw);
        self.insects
            .follow(self.world, self.listener_position, self.listener_yaw);
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
    pub fn trigger_world_thunder(&mut self, distance: f32, angle: f32, exposure: f32) -> bool {
        if !exposure.is_finite() {
            return false;
        }
        self.thunder
            .trigger_gain(distance, angle, 0.35 + 0.65 * exposure.clamp(0.0, 1.0))
    }
    pub fn next(&mut self, preset: Preset) -> ([f32; 2], [f32; 2]) {
        // Manual thunder remains available with ambient preview disabled.
        let thunder = self.thunder.next();
        if preset == Preset::Off && self.world.is_none() {
            return ([0.0; 2], thunder);
        }
        if self.frame == 0 {
            self.follow(preset);
        }
        self.frame = (self.frame + 1) % 441;
        if self.world.is_none() && preset == Preset::Storm && self.weather.lightning > 0.0 {
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
        let (bugs, insect_send) = self.insects.next();
        let wet = self.reverb.next(send + insect_send);
        let target_exposure = self.world.map_or(1.0, |w| w.exposure);
        self.exposure += (target_exposure - self.exposure) / (0.3 * 44_100.0);
        let ambient = std::array::from_fn(|i| {
            let sample = rain[i] + wind[i] + bugs[i] + 0.12 * wet[i];
            // Sheltered listeners still hear muted outdoor weather; this is a
            // presentation approximation rather than voxel acoustic tracing.
            self.indoor_filter[i] += 0.06 * (sample - self.indoor_filter[i]);
            (self.exposure * sample + (1.0 - self.exposure) * self.indoor_filter[i])
                * (0.15 + 0.85 * self.exposure)
        });
        (ambient, thunder)
    }
}

fn lightning_hit(draw: u32, rate_per_min: f32) -> bool {
    let threshold = (f64::from(rate_per_min) / (60.0 * 44_100.0) * 4_294_967_296.0) as u32;
    draw < threshold
}
#[cfg(test)]
#[path = "procedural/tests.rs"]
mod tests;
