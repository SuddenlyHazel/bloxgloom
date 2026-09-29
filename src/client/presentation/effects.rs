//! Session-local bounded transient effects attached to offered entity visuals.
use crate::render::{VisualAvatar, VisualFire, fire::FireStyle};
use glam::Vec3;
use std::time::{Duration, Instant};

const LIFE: Duration = Duration::from_millis(850);
const MAX_EMBERS: usize = 32;
#[cfg(test)]
#[path = "effects/tests.rs"]
mod tests;

#[derive(Debug, Default)]
pub(crate) struct EffectBuffer {
    entries: Vec<(u64, [f32; 3], Instant, FireStyle)>,
}

impl EffectBuffer {
    pub(crate) fn push(&mut self, id: u64, offset: [f32; 3]) {
        self.push_style(id, offset, FireStyle::Flame);
    }

    pub(crate) fn spark(&mut self, id: u64, offset: [f32; 3], color: [f32; 3]) {
        self.push_style(id, offset, FireStyle::Spark(color));
    }

    fn push_style(&mut self, id: u64, offset: [f32; 3], style: FireStyle) {
        if self.entries.len() == MAX_EMBERS {
            self.entries.remove(0);
        }
        self.entries.push((id, offset, Instant::now(), style));
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }

    pub(crate) fn visuals(&self, now: Instant, avatars: &[VisualAvatar]) -> Vec<VisualFire> {
        self.entries
            .iter()
            .filter_map(|(id, offset, at, style)| {
                if now.saturating_duration_since(*at) >= LIFE {
                    return None;
                }
                let avatar = avatars.iter().find(|avatar| avatar.id == *id)?;
                Some(VisualFire {
                    center: avatar.position + Vec3::from(*offset),
                    age: (now.saturating_duration_since(*at).as_secs_f32() / LIFE.as_secs_f32())
                        .clamp(0.0, 1.0),
                    style: *style,
                })
            })
            .collect()
    }
}
