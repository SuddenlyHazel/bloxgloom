//! Four persistent cricket individuals adapted from NoiseMachine's noise_crickets.c.
//! MIT Copyright (c) 2026 kvmet. See third-party/NoiseMachine-LICENSE.
use super::super::{
    dsp::{Rng, SAMPLE_RATE},
    spatial::{Bus, Listener, Spatial},
};
use super::{VOICES, dsp::Oscillator, rest};
use std::f32::consts::TAU;
struct Cricket {
    pub spatial: Spatial,
    oscillator: Oscillator,
    glide: f32,
    pitch_offset: f32,
    period_scale: f32,
    pulses: u32,
    pulse_samples: u32,
    sounding_samples: u32,
    chirp_samples: u32,
    until_chirp: u32,
    singing: bool,
    bout_samples: u32,
}
impl Cricket {
    fn period(&self, rng: &mut Rng, rate: f32) -> u32 {
        ((self.period_scale * rng.between(0.97, 1.03) * SAMPLE_RATE / rate) as u32)
            .max(self.pulses * self.pulse_samples)
    }
    fn new(rng: &mut Rng) -> Self {
        let pulse_samples = (SAMPLE_RATE * rng.between(0.026, 0.036)) as u32;
        let sounding_samples = (pulse_samples as f32 * rng.between(0.55, 0.70)) as u32;
        let pulses = 3 + rng.next_u32() % 3;
        let singing = rng.unit() < 0.75;
        Self {
            spatial: Spatial::default(),
            oscillator: Oscillator::default(),
            glide: 0.0,
            pitch_offset: rng.between(-1.0, 1.0),
            period_scale: rng.between(0.9, 1.1),
            pulses,
            pulse_samples,
            sounding_samples,
            chirp_samples: pulses * pulse_samples,
            until_chirp: (rng.unit() * SAMPLE_RATE) as u32,
            singing,
            bout_samples: rest(rng, if singing { 30.0 } else { 10.0 }),
        }
    }
}
pub(super) struct Crickets {
    config: crate::audio::rain_tuning::CricketTone,
    listener: Listener,
    rng: Rng,
    voices: [Cricket; VOICES],
}
impl Crickets {
    pub fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed, 0xb54cda58);
        let mut voices = std::array::from_fn(|_| Cricket::new(&mut rng));
        voices[0].singing = true;
        voices[0].until_chirp = 0;
        Self {
            config: Default::default(),
            listener: Default::default(),
            rng,
            voices,
        }
    }
    pub fn wake(&mut self) {
        self.voices[0].singing = true;
        self.voices[0].until_chirp = 0;
    }
    pub fn configure(&mut self, config: crate::audio::rain_tuning::CricketTone) {
        self.config = config;
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
        rate: f32,
        gain: f32,
        bus: &mut Bus,
    ) -> f32 {
        let mut send = 0.0;
        for (voice, enabled) in self.voices.iter_mut().zip(enabled) {
            voice.bout_samples -= 1;
            if voice.bout_samples == 0 {
                voice.singing = !voice.singing;
                voice.bout_samples = rest(&mut self.rng, if voice.singing { 30.0 } else { 10.0 });
            }
            if voice.until_chirp == 0 {
                voice.until_chirp = voice.period(&mut self.rng, rate);
                if voice.singing && *enabled && allowed {
                    voice.chirp_samples = 0;
                }
            }
            voice.until_chirp -= 1;
            let mut sample = 0.0;
            if voice.chirp_samples < voice.pulses * voice.pulse_samples {
                let within = voice.chirp_samples % voice.pulse_samples;
                voice.chirp_samples += 1;
                if within == 0 {
                    let pitch = self.config.pitch_hz
                        * (1.0 + 0.3 * self.config.pitch_variation * voice.pitch_offset);
                    voice.oscillator = Oscillator::new(pitch);
                    let end = 2.0 * (TAU * pitch * 0.97 / SAMPLE_RATE).cos();
                    voice.glide =
                        (end - voice.oscillator.coefficient) / voice.sounding_samples as f32;
                }
                if within < voice.sounding_samples {
                    let carrier = voice.oscillator.next();
                    voice.oscillator.coefficient += voice.glide;
                    let attack = voice.sounding_samples / 4;
                    let envelope = if within < attack {
                        within as f32 / attack as f32
                    } else {
                        (voice.sounding_samples - within) as f32
                            / (voice.sounding_samples - attack) as f32
                    };
                    sample = if *enabled {
                        1.315
                            * gain
                            * self.config.gain
                            * envelope
                            * (carrier - 0.22 * carrier.powi(3))
                    } else {
                        0.0
                    };
                }
            }
            send += voice.spatial.emit(self.listener, bus, sample);
        }
        send
    }
}
