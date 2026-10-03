//! Native listening state, with explicit authoritative weather inputs and local previews.
use crate::audio::rain_scene::RainScene;
use crate::{
    audio::{Command, Controls, Preset, WeatherSound, output::AudioOutput},
    config::Config,
};
use std::sync::Arc;
use std::time::{Duration, Instant};

mod obstruction;
mod obstruction_state;
mod voices;
pub(super) struct State {
    voices: voices::Voices,
    obstruction: obstruction_state::State,
    #[cfg(test)]
    sent: std::cell::RefCell<Vec<Command>>,
    #[cfg(test)]
    blocked: std::cell::Cell<bool>,
    output: Option<AudioOutput>,
    preset: Preset,
    next_voice: u64,
    world: Option<WeatherSound>,
    rain_scene: Option<Arc<RainScene>>,
    scene_dirty: bool,
    sent_world: Option<Option<WeatherSound>>,
    last_poll: Option<Instant>,
    last_listener: Option<([f32; 3], f32)>,
}
impl State {
    pub(super) fn new(config: &Config) -> Self {
        #[cfg(not(test))]
        let output = Some(AudioOutput::start(volumes(config, Preset::Off)));
        #[cfg(test)]
        let output: Option<AudioOutput> = {
            let _ = config;
            None
        };
        if let Some(output) = &output {
            output.set_rain_config(config.rain_audio);
            output.set_mix_config(config.audio_mix);
        }
        Self {
            output,
            voices: Default::default(),
            obstruction: obstruction_state::State::new(),
            #[cfg(test)]
            sent: Default::default(),
            #[cfg(test)]
            blocked: Default::default(),
            preset: Preset::Off,
            next_voice: 1,
            world: None,
            rain_scene: None,
            scene_dirty: false,
            sent_world: None,
            last_poll: None,
            last_listener: None,
        }
    }
    fn send_command(&self, command: Command) -> bool {
        #[cfg(test)]
        {
            if self.blocked.get() {
                return false;
            }
            self.sent.borrow_mut().push(command);
            true
        }
        #[cfg(not(test))]
        {
            self.output
                .as_ref()
                .is_some_and(|output| output.try_send(command))
        }
    }
    pub(super) fn preset(&self) -> Preset {
        self.preset
    }
    pub(super) fn set_volumes(&self, config: &Config) {
        if let Some(output) = &self.output {
            output.set_controls(volumes(config, self.preset));
            output.set_rain_config(config.rain_audio);
            output.set_mix_config(config.audio_mix);
        }
    }
    pub(super) fn change_preview(&mut self, increase: bool, config: &Config) {
        self.preset = Preset::from_index(((self.preset as u8) + if increase { 1 } else { 3 }) % 4);
        self.select_preview(self.preset, config);
    }
    pub(super) fn select_preview(&mut self, preset: Preset, config: &Config) {
        self.preset = preset;
        self.set_volumes(config);
        self.sent_world = None;
        self.send_weather();
    }
    pub(super) fn test_thunder(&self, distance: f32, angle: f32) {
        if (200.0..=15000.0).contains(&distance) && angle.is_finite() {
            self.send_command(Command::Thunder { distance, angle });
        }
    }
    pub(super) fn update_rain_scene(&mut self, scene: RainScene) {
        if self.rain_scene.as_deref() != Some(&scene) {
            self.rain_scene = Some(Arc::new(scene));
            self.scene_dirty = true;
        }
    }
    fn send_weather(&mut self) {
        if self.scene_dirty
            && let Some(scene) = &self.rain_scene
            && self.send_command(Command::RainScene(scene.clone()))
        {
            self.scene_dirty = false;
        }
        let desired = if self.preset == Preset::Off {
            self.world
        } else {
            None
        };
        if self.sent_world != Some(desired) && self.send_command(Command::Weather(desired)) {
            self.sent_world = Some(desired);
        }
    }
    /// Call from the client presentation clock (at most 20 Hz). Inputs are
    /// rain in mm/hour, wind in metres/second, clockwise bearing in radians,
    /// outdoor exposure and daylight activity in 0..=1. Preview overrides
    /// ambient world sound.
    pub(super) fn update_weather(
        &mut self,
        rain_mm_h: f32,
        wind_m_s: f32,
        bearing: f32,
        exposure: f32,
        daylight: f32,
    ) {
        let sample = WeatherSound {
            rain_mm_h,
            wind_m_s,
            bearing,
            exposure,
            daylight,
        }
        .sanitized();
        self.world = Some(sample);
        self.send_weather();
    }
    /// Caller schedules propagation delay from the synchronized strike and
    /// provides a clockwise angle relative to the listener's current heading.
    pub(super) fn thunder(&self, distance: f32, angle: f32, exposure: f32) {
        if let Some(output) = &self.output {
            output.try_send(Command::WorldThunder {
                distance: distance.clamp(200.0, 15_000.0),
                angle,
                exposure,
            });
        }
    }
    pub(super) fn test_sound(&mut self) {
        let Some(next) = self.next_voice.checked_add(1) else {
            return;
        };
        let id = self.next_voice;
        self.next_voice = next;
        if let Some(output) = &self.output {
            output.try_send(Command::Click(id));
            if self.preset == Preset::Storm {
                output.try_send(Command::Thunder {
                    distance: 1200.0,
                    angle: 0.7,
                });
            }
        }
    }
    pub(super) fn poll_listener(&mut self, position: [f32; 3], yaw: f32, now: Instant) {
        if self
            .last_poll
            .is_some_and(|last| now.saturating_duration_since(last) < Duration::from_millis(50))
        {
            return;
        }
        self.last_poll = Some(now);
        if self.last_listener == Some((position, yaw)) {
            return;
        }
        if let Some(output) = &self.output
            && output.try_send(Command::Listener { position, yaw })
        {
            self.last_listener = Some((position, yaw));
        }
    }
    pub(super) fn retire_session(&mut self, config: &Config) {
        self.voices = Default::default();
        self.obstruction.retire();
        self.preset = Preset::Off;
        self.world = None;
        self.rain_scene = None;
        self.scene_dirty = false;
        self.sent_world = None;
        self.last_poll = None;
        self.last_listener = None;
        if let Some(output) = &self.output {
            output.reset();
            output.set_controls(volumes(config, Preset::Off));
        }
    }
}
fn volumes(config: &Config, preset: Preset) -> Controls {
    Controls {
        master: config.audio_master,
        ambient: config.audio_ambient,
        effects: config.audio_effects,
        preset,
    }
    .sanitized()
}

#[cfg(test)]
#[path = "audio/tests.rs"]
mod tests;
