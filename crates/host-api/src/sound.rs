//! Transient presentation, staged until the server commit barrier. Not save data.
/// Fixed presentation routes. Packages cannot add buses or change another source's controls.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Bus {
    Ambient = 0,
    #[default]
    Effects = 1,
    Ui = 2,
    Music = 3,
}
impl Bus {
    pub fn from_index(index: u8) -> Option<Self> {
        match index {
            0 => Some(Self::Ambient),
            1 => Some(Self::Effects),
            2 => Some(Self::Ui),
            3 => Some(Self::Music),
            _ => None,
        }
    }
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "ambient" => Some(Self::Ambient),
            "effects" => Some(Self::Effects),
            "ui" => Some(Self::Ui),
            "music" => Some(Self::Music),
            _ => None,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub owner: String,
    pub voice: String,
    pub kind: Kind,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Play {
        bus: Bus,
        clip: String,
        position: [f32; 3],
        entity: Option<u64>,
        gain: f32,
        pitch: f32,
        looping: bool,
    },
    Update {
        position: Option<[f32; 3]>,
        gain: f32,
        pitch: f32,
    },
    Stop,
}
pub fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-' | b'.')
        })
}
pub fn key(value: &str) -> bool {
    value
        .split_once(':')
        .is_some_and(|(a, b)| identifier(a) && identifier(b))
}
pub fn position(value: &[f32; 3]) -> bool {
    value
        .iter()
        .all(|v| v.is_finite() && v.abs() <= 16_000_000.0)
}
impl Event {
    pub fn validate(&self) -> bool {
        if !identifier(&self.owner) || !identifier(&self.voice) {
            return false;
        }
        match &self.kind {
            Kind::Play {
                clip,
                position: at,
                entity,
                gain,
                pitch,
                looping,
                ..
            } => {
                key(clip)
                    && position(at)
                    && entity.is_none_or(|v| v != 0)
                    && (!looping || entity.is_some())
                    && controls(*gain, *pitch)
            }
            Kind::Update {
                position: at,
                gain,
                pitch,
            } => at.as_ref().is_none_or(position) && controls(*gain, *pitch),
            Kind::Stop => true,
        }
    }
}
fn controls(gain: f32, pitch: f32) -> bool {
    gain.is_finite()
        && (0.0..=4.0).contains(&gain)
        && pitch.is_finite()
        && (0.25..=4.0).contains(&pitch)
}
