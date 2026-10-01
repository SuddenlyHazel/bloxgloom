//! Sample-clock mixer with bounded native voices and smoothed playback controls.
use super::{
    Clip, Command, Controls, Preset, SAMPLE_RATE, WeatherSound, limiter::Limiter,
    procedural::Procedural,
};
use std::sync::Arc;
const MAX_VOICES: usize = 32;
struct Voice {
    clip: Arc<Clip>,
    cursor: f64,
    position: Option<[f32; 3]>,
    gain: f32,
    target_gain: f32,
    pitch: f32,
    target_pitch: f32,
    looping: bool,
    id: u64,
    ears: [f32; 2],
    age: u32,
    stop_remaining: Option<u32>,
}
pub(crate) struct Mixer {
    voices: Vec<Voice>,
    listener: [f32; 3],
    yaw: f32,
    controls: Controls,
    levels: [f32; 3],
    synth: Procedural,
    desired_world: Option<WeatherSound>,
    preset: Preset,
    fade: f32,
    limiter: Limiter,
    seed: u32,
    pub rejected: u64,
    click: Arc<Clip>,
}
fn gains(position: Option<[f32; 3]>, listener: [f32; 3], yaw: f32) -> [f32; 2] {
    let Some(p) = position else {
        return [1.0; 2];
    };
    let delta = std::array::from_fn::<_, 3, _>(|i| p[i] - listener[i]);
    let distance = delta.iter().map(|x| x * x).sum::<f32>().sqrt();
    // Game forward is +X at yaw0; positive yaw points toward +Z.
    let right = -delta[0] * yaw.sin() + delta[2] * yaw.cos();
    let pan = (right / distance.max(0.001)).clamp(-1.0, 1.0);
    let angle = std::f32::consts::FRAC_PI_4 * (pan + 1.0);
    let atten = 1.0 / distance.max(1.0);
    [angle.cos() * atten, angle.sin() * atten]
}
impl Mixer {
    pub fn new(seed: u32) -> Self {
        Self {
            voices: Vec::with_capacity(MAX_VOICES),
            listener: [0.0; 3],
            yaw: 0.0,
            controls: Controls::default(),
            levels: [0.0; 3],
            synth: Procedural::new(seed),
            desired_world: None,
            preset: Preset::Off,
            fade: 0.0,
            limiter: Limiter::default(),
            seed,
            rejected: 0,
            click: Arc::new(Clip::click()),
        }
    }
    pub fn diagnostics(&self) -> (u64, u64, usize, usize, u64) {
        self.synth.stats()
    }
    pub fn set_controls(&mut self, controls: Controls) {
        self.controls = controls.sanitized();
    }
    pub fn command(&mut self, command: Command) -> bool {
        if let Command::Click(id) = command {
            return self.command(Command::Play {
                clip: self.click.clone(),
                position: None,
                gain: 1.0,
                pitch: 1.0,
                looping: false,
                id,
            });
        }
        let accepted = match command {
            Command::Play {
                clip,
                position,
                gain,
                pitch,
                looping,
                id,
            } => {
                if id == 0
                    || !pitch.is_finite()
                    || !(0.25..=4.0).contains(&pitch)
                    || !gain.is_finite()
                    || !(0.0..=4.0).contains(&gain)
                    || position.is_some_and(|p| {
                        p.into_iter()
                            .any(|x| !x.is_finite() || x.abs() > 16_000_000.0)
                    })
                    || clip.frames.is_empty()
                    || self.voices.len() >= MAX_VOICES
                    || self.voices.iter().any(|v| v.id == id)
                {
                    false
                } else {
                    let ears = gains(position, self.listener, self.yaw);
                    self.voices.push(Voice {
                        clip,
                        cursor: 0.0,
                        position,
                        gain,
                        target_gain: gain,
                        pitch,
                        target_pitch: pitch,
                        looping,
                        id,
                        ears,
                        age: 0,
                        stop_remaining: None,
                    });
                    true
                }
            }
            Command::Update {
                id,
                position,
                gain,
                pitch,
            } => {
                if !gain.is_finite()
                    || !(0.0..=4.0).contains(&gain)
                    || !pitch.is_finite()
                    || !(0.25..=4.0).contains(&pitch)
                    || position
                        .is_some_and(|p| p.iter().any(|x| !x.is_finite() || x.abs() > 16_000_000.0))
                {
                    false
                } else if let Some(voice) = self
                    .voices
                    .iter_mut()
                    .find(|v| v.id == id && v.stop_remaining.is_none())
                {
                    voice.position = position;
                    voice.target_gain = gain;
                    voice.target_pitch = pitch;
                    true
                } else {
                    false
                }
            }
            Command::Click(_) => unreachable!("handled above"),
            Command::Stop(id) => {
                for voice in &mut self.voices {
                    if voice.id == id {
                        voice.stop_remaining.get_or_insert(882);
                    }
                }
                true
            }
            Command::Listener { position, yaw } => {
                if position
                    .into_iter()
                    .all(|x| x.is_finite() && x.abs() <= 16_000_000.0)
                    && yaw.is_finite()
                {
                    self.listener = position;
                    self.yaw = yaw;
                    true
                } else {
                    false
                }
            }
            Command::Thunder { distance, angle } => self.synth.trigger_thunder(distance, angle),
            Command::Weather(weather) => {
                self.desired_world = weather.map(WeatherSound::sanitized);
                // Continuous inputs may change immediately within the active
                // world source. Switching source itself waits for fade-out.
                if self.synth.world_active() && self.desired_world.is_some() {
                    self.synth.set_world(self.desired_world);
                }
                true
            }
            Command::WorldThunder {
                distance,
                angle,
                exposure,
            } => self.synth.trigger_world_thunder(distance, angle, exposure),
            Command::Reset => {
                self.voices.clear();
                self.synth = Procedural::new(self.seed);
                self.desired_world = None;
                self.preset = Preset::Off;
                self.fade = 0.0;
                self.controls.preset = Preset::Off;
                self.limiter = Limiter::default();
                true
            }
        };
        if !accepted {
            self.rejected = self.rejected.saturating_add(1);
        }
        accepted
    }
    pub fn render(&mut self, output: &mut [[f32; 2]]) {
        for out in output {
            let target = [
                self.controls.master,
                self.controls.ambient,
                self.controls.effects,
            ];
            for (level, target) in self.levels.iter_mut().zip(target) {
                *level += (target - *level) / (0.02 * SAMPLE_RATE as f32);
            }
            let changing_preset = self.preset != self.controls.preset;
            let changing_world = self.desired_world.is_some() != self.synth.world_active();
            let changing = changing_preset || changing_world;
            let fade_target =
                if changing || (self.preset == Preset::Off && !self.synth.world_active()) {
                    0.0
                } else {
                    1.0
                };
            self.fade += (fade_target - self.fade) / (0.02 * SAMPLE_RATE as f32);
            if changing && self.fade < 0.0001 {
                self.synth.set_world(self.desired_world);
                if changing_preset {
                    self.preset = self.controls.preset;
                    self.synth.set_preset(self.preset);
                }
            }
            let (ambient, thunder) = self.synth.next(self.preset);
            let mut mix = std::array::from_fn::<_, 2, _>(|i| {
                ambient[i] * self.fade * self.levels[1] + 0.5 * thunder[i] * self.levels[2]
            });
            let mut index = 0;
            while index < self.voices.len() {
                if self.voices[index].stop_remaining == Some(0) {
                    self.voices.swap_remove(index);
                    continue;
                }
                let voice = &mut self.voices[index];
                let length = voice.clip.frames.len();
                let start = voice.cursor as usize;
                if start >= length {
                    if voice.looping {
                        voice.cursor %= length as f64;
                        continue;
                    }
                    self.voices.swap_remove(index);
                    continue;
                }
                let frac = (voice.cursor - start as f64) as f32;
                let end = if start + 1 < length {
                    start + 1
                } else if voice.looping {
                    0
                } else {
                    start
                };
                let mut sample = std::array::from_fn::<_, 2, _>(|ear| {
                    voice.clip.frames[start][ear]
                        + frac * (voice.clip.frames[end][ear] - voice.clip.frames[start][ear])
                });
                if voice.position.is_some() {
                    let mono = 0.5 * (sample[0] + sample[1]);
                    sample = [mono; 2];
                }
                let target = gains(voice.position, self.listener, self.yaw);
                voice.gain += (voice.target_gain - voice.gain) / (0.02 * SAMPLE_RATE as f32);
                voice.pitch += (voice.target_pitch - voice.pitch) / (0.02 * SAMPLE_RATE as f32);
                let mut fade = (voice.age as f32 / 64.0).min(1.0);
                voice.age = voice.age.saturating_add(1);
                if let Some(remaining) = &mut voice.stop_remaining {
                    fade *= *remaining as f32 / 882.0;
                    *remaining = remaining.saturating_sub(1);
                }
                for ear in 0..2 {
                    voice.ears[ear] +=
                        (target[ear] - voice.ears[ear]) / (0.02 * SAMPLE_RATE as f32);
                    mix[ear] += sample[ear] * voice.ears[ear] * voice.gain * fade * self.levels[2];
                }
                voice.cursor +=
                    voice.clip.rate as f64 / SAMPLE_RATE as f64 * f64::from(voice.pitch);
                index += 1;
            }
            *out = self.limiter.next(mix.map(|x| x * self.levels[0]));
        }
    }
}

#[cfg(test)]
#[path = "mixer/tests.rs"]
mod tests;
