//! Native client audio foundation. Simulation decisions stay on the server.
mod clip;
mod limiter;
mod mixer;
mod obstruction;
pub(crate) mod output;
mod preview;
mod procedural;
pub(crate) mod rain_scene;
pub mod rain_tuning;
#[cfg(test)]
mod tests;
pub(crate) use clip::Clip;
pub(crate) use mixer::Mixer;
pub(crate) use preview::{
    play_file, play_preview, render_insect_preview, render_material_preview, render_preview,
};
use std::sync::Arc;
pub(crate) const SAMPLE_RATE: u32 = 44_100;
pub(crate) const MAX_CLIP_VOICES: usize = 32;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Preset {
    #[default]
    Off = 0,
    Rain = 1,
    Storm = 2,
    Wind = 3,
}
impl Preset {
    pub fn from_index(index: u8) -> Self {
        match index {
            1 => Self::Rain,
            2 => Self::Storm,
            3 => Self::Wind,
            _ => Self::Off,
        }
    }
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "off" => Some(Self::Off),
            "rain" => Some(Self::Rain),
            "storm" => Some(Self::Storm),
            "wind" => Some(Self::Wind),
            _ => None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Rain => "Rain",
            Self::Storm => "Storm",
            Self::Wind => "Wind",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Controls {
    pub master: f32,
    pub ambient: f32,
    pub effects: f32,
    pub preset: Preset,
}
impl Default for Controls {
    fn default() -> Self {
        Self {
            master: 1.0,
            ambient: 1.0,
            effects: 1.0,
            preset: Preset::Off,
        }
    }
}
impl Controls {
    pub fn sanitized(self) -> Self {
        fn volume(v: f32) -> f32 {
            if v.is_finite() {
                v.clamp(0.0, 1.0)
            } else {
                0.0
            }
        }
        Self {
            master: volume(self.master),
            ambient: volume(self.ambient),
            effects: volume(self.effects),
            preset: self.preset,
        }
    }
}
/// Continuous presentation inputs sampled from authoritative game weather.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct WeatherSound {
    pub rain_mm_h: f32,
    pub wind_m_s: f32,
    pub bearing: f32,
    pub exposure: f32,
    pub daylight: f32,
}
impl WeatherSound {
    pub fn sanitized(self) -> Self {
        fn bounded(value: f32, maximum: f32) -> f32 {
            if value.is_finite() {
                value.clamp(0.0, maximum)
            } else {
                0.0
            }
        }
        Self {
            rain_mm_h: bounded(self.rain_mm_h, 200.0),
            wind_m_s: bounded(self.wind_m_s, 40.0),
            bearing: if self.bearing.is_finite() {
                self.bearing.rem_euclid(std::f32::consts::TAU)
            } else {
                0.0
            },
            exposure: bounded(self.exposure, 1.0),
            daylight: bounded(self.daylight, 1.0),
        }
    }
}
pub(crate) enum Command {
    Play {
        clip: Arc<Clip>,
        position: Option<[f32; 3]>,
        gain: f32,
        pitch: f32,
        looping: bool,
        id: u64,
    },
    /// Start a positional voice with transmission already resolved, so short
    /// impacts cannot finish before their first obstruction result arrives.
    PlayObstructed {
        clip: Arc<Clip>,
        position: [f32; 3],
        gain: f32,
        pitch: f32,
        looping: bool,
        id: u64,
        transmission: f32,
        lowpass_hz: f32,
    },
    Update {
        id: u64,
        position: Option<[f32; 3]>,
        gain: f32,
        pitch: f32,
    },
    /// Presentation-only transmission, computed from streamed voxel geometry.
    Obstruction {
        id: u64,
        gain: f32,
        lowpass_hz: f32,
    },
    Click(u64),
    Stop(u64),
    Listener {
        position: [f32; 3],
        yaw: f32,
    },
    Thunder {
        distance: f32,
        angle: f32,
    },
    Weather(Option<WeatherSound>),
    RainScene(Arc<rain_scene::RainScene>),
    WorldThunder {
        distance: f32,
        angle: f32,
        exposure: f32,
    },
    Reset,
}

pub(crate) mod luau;
pub(crate) mod sounds;
