//! Ten cicada songs adapted from NoiseMachine's noise_cicadas.c.
//! MIT Copyright (c) 2026 kvmet. See third-party/NoiseMachine-LICENSE.
mod songs;
use super::super::{
    dsp::{Rng, SAMPLE_RATE},
    spatial::{Bus, Listener, Spatial},
};
use super::{
    VOICES,
    dsp::{Oscillator, Resonator},
    rest,
};
use crate::audio::rain_tuning::CicadaProfile;
use songs::{Song, song};
use std::f32::consts::TAU;
struct Cicada {
    spatial: Spatial,
    body: Resonator,
    throb: Oscillator,
    pitch_offset: f32,
    glide: f32,
    until_click: f32,
    elapsed: u32,
    length: u32,
    sounding: u32,
    until_call: u32,
    holding: bool,
    syllable: u32,
    syllables: u32,
}
impl Cicada {
    fn tune(&mut self, config: CicadaProfile, song: Song, drop: f32, frames: u32) {
        let pitch = config.tone.pitch_hz * (1.0 + 0.05 * self.pitch_offset);
        self.body.tune(pitch, song.q);
        let end = 2.0
            * self.body.radius_squared.sqrt()
            * (TAU * pitch * (1.0 - drop) / SAMPLE_RATE).cos();
        self.glide = if frames > 0 {
            (end - self.body.coefficient) / frames as f32
        } else {
            0.0
        };
    }
    fn rest(&mut self, rng: &mut Rng, song: Song) {
        self.length = 0;
        self.until_call = rest(rng, song.gap);
    }
    fn note(&mut self, rng: &mut Rng, config: CicadaProfile, song: Song) {
        self.elapsed = 0;
        if self.syllable < self.syllables {
            let progress = if self.syllables > 1 {
                self.syllable as f32 / (self.syllables - 1) as f32
            } else {
                0.0
            };
            let rate = song.rate[0] + progress * (song.rate[1] - song.rate[0]);
            self.length = (SAMPLE_RATE / rate) as u32;
            self.sounding = (song.duty * self.length as f32) as u32;
            self.holding = false;
            self.tune(config, song, song.drop, self.sounding);
        } else {
            let hold = rng.between(song.hold[0], song.hold[1]);
            if hold <= 0.0 {
                self.rest(rng, song);
                return;
            }
            self.length = (hold * SAMPLE_RATE) as u32;
            self.sounding = self.length;
            self.holding = true;
            self.tune(
                config,
                song,
                0.15,
                (self.length / 2).min(2 * SAMPLE_RATE as u32),
            );
            self.throb = Oscillator::new(rng.between(2.0, 4.0));
        }
    }
}
pub(super) struct Cicadas {
    rng: Rng,
    voices: [Cicada; VOICES],
    chorus: [Resonator; 2],
    swell: f32,
    swell_target: f32,
    swell_frames: u32,
    config: CicadaProfile,
    listener: Listener,
}
impl Cicadas {
    pub fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed, 0x94d049bb);
        let mut voices = std::array::from_fn(|_| Cicada {
            spatial: Spatial::default(),
            body: Resonator::default(),
            throb: Oscillator::default(),
            pitch_offset: rng.between(-1.0, 1.0),
            glide: 0.0,
            until_click: 0.0,
            elapsed: 0,
            length: 0,
            sounding: 0,
            until_call: rest(&mut rng, 20.0),
            holding: false,
            syllable: 0,
            syllables: 0,
        });
        voices[0].until_call = 1;
        Self {
            rng,
            voices,
            chorus: [Resonator::new(5000.0, 3.0); 2],
            swell: 0.5,
            swell_target: 0.5,
            swell_frames: 0,
            config: Default::default(),
            listener: Default::default(),
        }
    }
    pub fn wake(&mut self) {
        if self.voices[0].length == 0 {
            self.voices[0].until_call = 1;
        }
    }
    pub fn configure(&mut self, config: CicadaProfile) {
        let changed = self.config.species != config.species;
        self.config = config;
        let song = song(config.species);
        for chorus in &mut self.chorus {
            chorus.tune(config.tone.pitch_hz, (song.q * 0.5).max(3.0));
        }
        for voice in &mut self.voices {
            if changed {
                voice.length = 0;
                voice.until_call = 1;
            } else if voice.length > 0 {
                let drop = if voice.holding { 0.15 } else { song.drop };
                voice.tune(
                    config,
                    song,
                    drop,
                    voice.sounding.saturating_sub(voice.elapsed),
                );
            }
        }
    }
    pub fn place(
        &mut self,
        sources: &[Option<[f32; 3]>; VOICES],
        eye: [f32; 3],
        yaw: f32,
        listener: Listener,
    ) {
        self.listener = listener;
        for (voice, source) in self.voices.iter_mut().zip(sources) {
            if let Some(source) = source {
                super::place(&mut voice.spatial, *source, eye, yaw, listener);
            }
        }
    }
    pub fn next(
        &mut self,
        enabled: &[bool; VOICES],
        allowed: bool,
        gain: f32,
        bus: &mut Bus,
    ) -> f32 {
        let mut send = 0.0;
        let c = self.config;
        let song = song(c.species);
        let gain = gain * c.tone.gain;
        for (voice, enabled) in self.voices.iter_mut().zip(enabled) {
            if voice.length == 0 {
                voice.until_call -= 1;
                if voice.until_call == 0 {
                    if *enabled && allowed {
                        voice.syllables = song.count[0]
                            + self.rng.next_u32() % (song.count[1] - song.count[0] + 1);
                        voice.syllable = 0;
                        voice.body = Resonator::default();
                        voice.until_click = 0.0;
                        voice.note(&mut self.rng, c, song);
                    } else {
                        voice.rest(&mut self.rng, song);
                    }
                }
            }
            let mut sample = 0.0;
            if voice.length > 0 {
                let t = voice.elapsed;
                voice.elapsed += 1;
                let mut envelope = 0.0;
                let mut click_rate = 1.0;
                if t < voice.sounding {
                    if voice.holding {
                        let remaining = (voice.sounding - voice.elapsed) as f32;
                        let swell = (voice.sounding / 4).min(SAMPLE_RATE as u32).max(1) as f32;
                        let wind_down =
                            (voice.sounding / 2).min(2 * SAMPLE_RATE as u32).max(1) as f32;
                        envelope = (t as f32 / swell).min(1.0);
                        if remaining < wind_down {
                            let fraction = remaining / wind_down;
                            envelope *= fraction;
                            click_rate -= 0.15 * (1.0 - fraction);
                            voice.body.coefficient += voice.glide;
                        }
                        envelope *= 1.0 - song.throb * 0.5 * (1.0 + voice.throb.next());
                    } else {
                        let x = (t as f32 + 0.5) / voice.sounding as f32;
                        let level = if voice.syllables > 1 {
                            1.0 + (song.fade - 1.0) * voice.syllable as f32
                                / (voice.syllables - 1) as f32
                        } else {
                            1.0
                        };
                        envelope = 4.0 * x * (1.0 - x) * level;
                        voice.body.coefficient += voice.glide;
                    }
                    voice.until_click -= click_rate;
                }
                let impulse = if t < voice.sounding && voice.until_click <= 0.0 {
                    voice.until_click += SAMPLE_RATE / (song.click * c.tone.click_rate_scale)
                        * self.rng.between(0.99, 1.01);
                    0.5
                } else {
                    0.0
                };
                let body = voice.body.next(impulse);
                if *enabled {
                    sample = 1.38 * 2.0 * gain * envelope * body;
                }
                if voice.elapsed == voice.length {
                    if voice.holding {
                        voice.rest(&mut self.rng, song);
                    } else {
                        voice.syllable += 1;
                        voice.note(&mut self.rng, c, song);
                    }
                }
            }
            send += voice.spatial.emit(self.listener, bus, sample);
        }
        if self.swell_frames == 0 {
            self.swell_target = self.rng.between(0.3, 1.0);
            self.swell_frames = 4 * SAMPLE_RATE as u32;
        }
        self.swell_frames -= 1;
        self.swell += (self.swell_target - self.swell) / (2.0 * SAMPLE_RATE);
        let level = 1.38
            * 0.015
            * gain
            * c.tone.chorus
            * self.swell
            * (3.0 / (song.q * 0.5).max(3.0)).sqrt();
        bus.add(std::array::from_fn(|ear| {
            level * self.chorus[ear].next(2.0 * self.rng.unit() - 1.0)
        }));
        send
    }
}
#[cfg(test)]
mod tests;
