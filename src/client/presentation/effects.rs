//! Session-local bounded embers attached to offered public entity visuals.
use crate::render::{VisualAvatar, VisualFire};
use glam::Vec3;
use std::time::{Duration, Instant};

const LIFE: Duration = Duration::from_millis(850);
const MAX_EMBERS: usize = 32;
#[cfg(test)]
#[path = "effects/tests.rs"]
mod tests;

#[derive(Debug, Default)]
pub(crate) struct EffectBuffer {
    embers: Vec<(u64, [f32; 3], Instant)>,
}

impl EffectBuffer {
    pub(crate) fn push(&mut self, id: u64, offset: [f32; 3]) {
        if self.embers.len() == MAX_EMBERS {
            self.embers.remove(0);
        }
        self.embers.push((id, offset, Instant::now()));
    }

    pub(crate) fn clear(&mut self) {
        self.embers.clear();
    }

    pub(crate) fn visuals(&self, now: Instant, avatars: &[VisualAvatar]) -> Vec<VisualFire> {
        self.embers
            .iter()
            .filter_map(|(id, offset, at)| {
                if now.saturating_duration_since(*at) >= LIFE {
                    return None;
                }
                let avatar = avatars.iter().find(|avatar| avatar.id == *id)?;
                Some(VisualFire {
                    center: avatar.position + Vec3::from(*offset),
                    age: (now.saturating_duration_since(*at).as_secs_f32() / LIFE.as_secs_f32())
                        .clamp(0.0, 1.0),
                })
            })
            .collect()
    }
}
