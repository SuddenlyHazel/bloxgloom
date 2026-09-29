//! Transient presentation for server-confirmed burned cells; never predicts spread.
use crate::render::VisualFire;
use glam::Vec3;
use std::time::{Duration, Instant};

const LIFE: Duration = Duration::from_millis(850);
const MAX_FIRES: usize = 128;
const MAX_DISTANCE_SQUARED: f32 = 48.0 * 48.0;

pub(super) struct FireAnimator {
    burns: Vec<(Vec3, Instant)>,
}

impl FireAnimator {
    pub(super) fn new() -> Self {
        Self { burns: Vec::new() }
    }

    pub(super) fn confirmed_burns(&mut self, cells: &[[i32; 3]], now: Instant) {
        for cell in cells.iter().take(crate::protocol::MAX_FIRE_BURSTS) {
            let center = Vec3::new(
                cell[0] as f32 + 0.5,
                cell[1] as f32 + 0.5,
                cell[2] as f32 + 0.5,
            );
            if let Some(existing) = self.burns.iter_mut().find(|(p, _)| *p == center) {
                existing.1 = now;
            } else {
                if self.burns.len() == MAX_FIRES {
                    self.burns.remove(0);
                }
                self.burns.push((center, now));
            }
        }
    }

    pub(super) fn visuals(&mut self, now: Instant, camera: Vec3, facing: Vec3) -> Vec<VisualFire> {
        self.burns
            .retain(|(_, at)| now.saturating_duration_since(*at) < LIFE);
        self.burns
            .iter()
            .filter_map(|&(center, at)| {
                let offset = center - camera;
                if offset.length_squared() > MAX_DISTANCE_SQUARED || offset.dot(facing) < -1.0 {
                    return None;
                }
                Some(VisualFire {
                    center,
                    age: (now.saturating_duration_since(at).as_secs_f32() / LIFE.as_secs_f32())
                        .clamp(0.0, 1.0),
                    style: crate::render::fire::FireStyle::Flame,
                })
            })
            .collect()
    }
}
