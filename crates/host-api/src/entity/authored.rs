//! Bounded presentation controls for packaged models. These never affect physics.
use super::Error;

pub const MAX_VARIANTS: usize = 16;
pub const MAX_LAYERS: usize = 32;
pub const MAX_TINTS: usize = 16;
pub const MAX_VISUAL_BYTES: usize = 320;

#[derive(Clone, Debug, PartialEq)]
pub struct AuthoredModel {
    pub key: String,
    pub scale: f32,
    pub idle: Option<String>,
    pub walk: Option<String>,
    pub run: Option<String>,
}
#[derive(Clone, Debug, Default)]
pub struct VisualSchema {
    pub clips: Vec<String>,
    pub clip_loops: Vec<bool>,
    pub variants: Vec<(String, Vec<String>)>,
    pub layers: Vec<String>,
    pub tints: Vec<String>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipPlayback {
    pub clip: u16,
    pub speed: f32,
    pub looping: bool,
    pub crossfade_s: f32,
    pub started_tick: u64,
    /// Retrigger identity also distinguishes two restarts in one tick.
    pub sequence: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TintMode {
    Multiply,
    Replace,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tint {
    pub rgb: [u8; 3],
    pub mode: TintMode,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VisualState {
    pub playback: Option<ClipPlayback>,
    pub sample_tick: u64,
    pub sequence: u32,
    pub transition_s: f32,
    /// 255 leaves the asset default intact.
    pub variants: [u8; MAX_VARIANTS],
    /// -1 leaves the asset default intact, 0 hides, 1 shows.
    pub layers: [i8; MAX_LAYERS],
    pub tints: [Option<Tint>; MAX_TINTS],
}
impl Default for VisualState {
    fn default() -> Self {
        Self {
            playback: None,
            sample_tick: 0,
            sequence: 0,
            transition_s: 0.2,
            variants: [255; MAX_VARIANTS],
            layers: [-1; MAX_LAYERS],
            tints: [None; MAX_TINTS],
        }
    }
}
impl VisualSchema {
    pub fn valid(&self) -> bool {
        self.clips.len() <= 256
            && self.clip_loops.len() == self.clips.len()
            && self.variants.len() <= MAX_VARIANTS
            && self.layers.len() <= MAX_LAYERS
            && self.tints.len() <= MAX_TINTS
            && self
                .variants
                .iter()
                .all(|(_, o)| !o.is_empty() && o.len() <= 32)
    }
    pub fn accepts(&self, state: &VisualState) -> bool {
        self.valid()
            && state.transition_s.is_finite()
            && (0.0..=5.0).contains(&state.transition_s)
            && state.playback.is_none_or(|p| {
                (p.clip as usize) < self.clips.len()
                    && p.speed.is_finite()
                    && (0.0..=8.0).contains(&p.speed)
                    && p.crossfade_s.is_finite()
                    && (0.0..=5.0).contains(&p.crossfade_s)
                    && p.started_tick <= state.sample_tick
                    && p.sequence == state.sequence
            })
            && state.variants.iter().enumerate().all(|(i, v)| {
                *v == 255
                    || self
                        .variants
                        .get(i)
                        .is_some_and(|(_, o)| (*v as usize) < o.len())
            })
            && state
                .layers
                .iter()
                .enumerate()
                .all(|(i, v)| *v == -1 || (i < self.layers.len() && (0..=1).contains(v)))
            && state
                .tints
                .iter()
                .enumerate()
                .all(|(i, v)| v.is_none() || i < self.tints.len())
    }
}
impl VisualState {
    /// Fixed, canonical encoding has no allocation proportional to user input.
    pub fn encode(&self, schema: &VisualSchema) -> Result<Vec<u8>, Error> {
        if !schema.accepts(self) {
            return Err(Error::InvalidState);
        }
        let mut out = Vec::with_capacity(MAX_VISUAL_BYTES);
        out.push(1);
        out.extend(self.sample_tick.to_le_bytes());
        out.extend(self.sequence.to_le_bytes());
        out.extend(self.transition_s.to_le_bytes());
        out.push(u8::from(self.playback.is_some()));
        if let Some(p) = self.playback {
            out.extend(p.clip.to_le_bytes());
            out.extend(p.speed.to_le_bytes());
            out.push(u8::from(p.looping));
            out.extend(p.crossfade_s.to_le_bytes());
            out.extend(p.started_tick.to_le_bytes());
            out.extend(p.sequence.to_le_bytes());
        }
        out.extend(self.variants);
        out.extend(self.layers.map(|v| (v + 1) as u8));
        for tint in self.tints {
            out.push(u8::from(tint.is_some()));
            if let Some(t) = tint {
                out.extend(t.rgb);
                out.push(u8::from(t.mode == TintMode::Replace));
            }
        }
        Ok(out)
    }
    pub fn decode(bytes: &[u8], schema: &VisualSchema) -> Result<Self, Error> {
        let mut cursor = Cursor(bytes);
        if cursor.byte()? != 1 {
            return Err(Error::InvalidState);
        }
        let sample_tick = u64::from_le_bytes(cursor.take()?);
        let sequence = u32::from_le_bytes(cursor.take()?);
        let transition_s = f32::from_le_bytes(cursor.take()?);
        let playback = match cursor.byte()? {
            0 => None,
            1 => Some(ClipPlayback {
                clip: u16::from_le_bytes(cursor.take()?),
                speed: f32::from_le_bytes(cursor.take()?),
                looping: match cursor.byte()? {
                    0 => false,
                    1 => true,
                    _ => return Err(Error::InvalidState),
                },
                crossfade_s: f32::from_le_bytes(cursor.take()?),
                started_tick: u64::from_le_bytes(cursor.take()?),
                sequence: u32::from_le_bytes(cursor.take()?),
            }),
            _ => return Err(Error::InvalidState),
        };
        let variants = cursor.take()?;
        let mut layers = [-1; MAX_LAYERS];
        for layer in &mut layers {
            *layer = match cursor.byte()? {
                0 => -1,
                1 => 0,
                2 => 1,
                _ => return Err(Error::InvalidState),
            };
        }
        let mut tints = [None; MAX_TINTS];
        for tint in &mut tints {
            *tint = match cursor.byte()? {
                0 => None,
                1 => Some(Tint {
                    rgb: cursor.take()?,
                    mode: match cursor.byte()? {
                        0 => TintMode::Multiply,
                        1 => TintMode::Replace,
                        _ => return Err(Error::InvalidState),
                    },
                }),
                _ => return Err(Error::InvalidState),
            };
        }
        let state = Self {
            playback,
            sample_tick,
            sequence,
            transition_s,
            variants,
            layers,
            tints,
        };
        if !cursor.0.is_empty() || !schema.accepts(&state) {
            return Err(Error::InvalidState);
        }
        Ok(state)
    }
}
struct Cursor<'a>(&'a [u8]);
impl Cursor<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        if self.0.len() < N {
            return Err(Error::InvalidState);
        }
        let (head, tail) = self.0.split_at(N);
        self.0 = tail;
        Ok(head.try_into().unwrap())
    }
    fn byte(&mut self) -> Result<u8, Error> {
        Ok(self.take::<1>()?[0])
    }
}

#[cfg(test)]
mod tests;
