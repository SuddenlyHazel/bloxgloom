//! Local listening state. The preview never infers authoritative world weather.
use crate::{
    audio::{Command, Controls, Preset, output::AudioOutput},
    config::Config,
};
use std::time::{Duration, Instant};

pub(super) struct State {
    output: Option<AudioOutput>,
    preset: Preset,
    next_voice: u64,
    last_poll: Option<Instant>,
    last_listener: Option<([f32; 3], f32)>,
}
impl State {
    pub(super) fn new(config: &Config) -> Self {
        #[cfg(not(test))]
        let output = Some(AudioOutput::start(volumes(config, Preset::Off)));
        #[cfg(test)]
        let output = {
            let _ = config;
            None
        };
        Self {
            output,
            preset: Preset::Off,
            next_voice: 1,
            last_poll: None,
            last_listener: None,
        }
    }
    pub(super) fn preset(&self) -> Preset {
        self.preset
    }
    pub(super) fn set_volumes(&self, config: &Config) {
        if let Some(output) = &self.output {
            output.set_controls(volumes(config, self.preset));
        }
    }
    pub(super) fn change_preview(&mut self, increase: bool, config: &Config) {
        self.preset = Preset::from_index(((self.preset as u8) + if increase { 1 } else { 3 }) % 4);
        self.set_volumes(config);
    }
    pub(super) fn test_sound(&mut self) {
        let Some(next) = self.next_voice.checked_add(1) else {
            return;
        };
        let id = self.next_voice;
        self.next_voice = next;
        if let Some(output) = &self.output {
            output.try_send(Command::Click(id));
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
        self.preset = Preset::Off;
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
