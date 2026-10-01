//! Dog-day cicada tymbal synthesis adapted from NoiseMachine's noise_cicadas.c.
//! MIT Copyright (c) 2026 kvmet. See third-party/NoiseMachine-LICENSE.
use super::super::{
    dsp::{Rng, SAMPLE_RATE},
    spatial::{Bus, Listener, Spatial},
};
use super::{
    VOICES,
    dsp::{Oscillator, Resonator},
    rest,
};
use std::f32::consts::TAU;
struct Cicada {
    spatial: Spatial,
    body: Resonator,
    throb: Oscillator,
    pitch: f32,
    glide: f32,
    until_click: f32,
    elapsed: u32,
    length: u32,
    until_call: u32,
}
pub(super) struct Cicadas {
    rng: Rng,
    voices: [Cicada; VOICES],
    chorus: [Resonator; 2],
    swell: f32,
    swell_target: f32,
    swell_frames: u32,
}
impl Cicadas {
    pub fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed, 0x94d049bb);
        let mut voices = std::array::from_fn(|_| Cicada {
            spatial: Spatial::default(),
            body: Resonator::default(),
            throb: Oscillator::default(),
            pitch: 5000.0 * (1.0 + 0.05 * rng.between(-1.0, 1.0)),
            glide: 0.0,
            until_click: 0.0,
            elapsed: 0,
            length: 0,
            until_call: rest(&mut rng, 20.0),
        });
        voices[0].until_call = 1;
        Self {
            rng,
            voices,
            chorus: [Resonator::new(5000.0, 3.0); 2],
            swell: 0.5,
            swell_target: 0.5,
            swell_frames: 0,
        }
    }
    pub fn place(&mut self, sources: &[Option<[f32; 3]>; VOICES], eye: [f32; 3], yaw: f32) {
        for (voice, source) in self.voices.iter_mut().zip(sources) {
            if let Some(source) = source {
                super::place(&mut voice.spatial, *source, eye, yaw);
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
        for (voice, enabled) in self.voices.iter_mut().zip(enabled) {
            if voice.length == 0 {
                voice.until_call -= 1;
                if voice.until_call == 0 {
                    if *enabled && allowed {
                        voice.length = (self.rng.between(10.0, 18.0) * SAMPLE_RATE) as u32;
                        voice.elapsed = 0;
                        voice.until_click = 0.0;
                        voice.body = Resonator::new(voice.pitch, 6.0);
                        let end = 2.0
                            * voice.body.radius_squared.sqrt()
                            * (TAU * voice.pitch * 0.85 / SAMPLE_RATE).cos();
                        voice.glide = (end - voice.body.coefficient) / (2.0 * SAMPLE_RATE);
                        voice.throb = Oscillator::new(self.rng.between(2.0, 4.0));
                    } else {
                        voice.until_call = rest(&mut self.rng, 20.0);
                    }
                }
            }
            let mut sample = 0.0;
            if voice.length > 0 {
                let remaining = (voice.length - voice.elapsed - 1) as f32;
                let swell = (voice.length / 4).min(SAMPLE_RATE as u32) as f32;
                let wind_down = (voice.length / 2).min(2 * SAMPLE_RATE as u32) as f32;
                let mut envelope = (voice.elapsed as f32 / swell).min(1.0);
                let mut click_rate = 1.0;
                if remaining < wind_down {
                    let fraction = remaining / wind_down;
                    envelope *= fraction;
                    click_rate -= 0.15 * (1.0 - fraction);
                    voice.body.coefficient += voice.glide;
                }
                envelope *= 1.0 - 0.4 * 0.5 * (1.0 + voice.throb.next());
                voice.until_click -= click_rate;
                let impulse = if voice.until_click <= 0.0 {
                    voice.until_click += SAMPLE_RATE / 300.0 * self.rng.between(0.99, 1.01);
                    0.5
                } else {
                    0.0
                };
                sample = if *enabled {
                    1.38 * 2.0 * gain * envelope * voice.body.next(impulse)
                } else {
                    0.0
                };
                voice.elapsed += 1;
                if voice.elapsed == voice.length {
                    voice.length = 0;
                    voice.until_call = rest(&mut self.rng, 20.0);
                }
            }
            send += voice.spatial.emit(Listener::default(), bus, sample);
        }
        if self.swell_frames == 0 {
            self.swell_target = self.rng.between(0.3, 1.0);
            self.swell_frames = 4 * SAMPLE_RATE as u32;
        }
        self.swell_frames -= 1;
        self.swell += (self.swell_target - self.swell) / (2.0 * SAMPLE_RATE);
        let level = 1.38 * 0.015 * gain * 0.35 * self.swell;
        let chorus = std::array::from_fn::<_, 2, _>(|ear| {
            level * self.chorus[ear].next(2.0 * self.rng.unit() - 1.0)
        });
        bus.add(chorus);
        send
    }
}
