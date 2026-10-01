//! Rigid moving objects interpolate committed poses. No collision prediction,
//! gait, or extrapolation can carry them past an authoritative contact.
use super::{DELAY, Sample};
use crate::render::VisualAvatar;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub(super) struct Track {
    samples: VecDeque<Sample>,
}

impl Track {
    pub(super) fn new(avatar: VisualAvatar, now: Instant) -> Self {
        Self {
            samples: VecDeque::from([Sample { avatar, at: now }]),
        }
    }

    pub(super) fn update(&mut self, avatar: VisualAvatar, now: Instant) -> VisualAvatar {
        let prior = *self.samples.back().unwrap();
        let motion = avatar.motion.unwrap();
        let old = prior.avatar.motion.unwrap();
        if avatar.model != prior.avatar.model {
            *self = Self::new(avatar, now);
        } else if motion.revision > old.revision && motion.tick >= old.tick {
            // Stops, impacts and long gaps are discontinuities. Freeze/correct
            // immediately rather than visually continuing an earlier trajectory.
            if motion.stopped
                || prior.avatar.position.distance(avatar.position) > 4.0
                || now.saturating_duration_since(prior.at) > Duration::from_millis(200)
            {
                self.samples.clear();
            }
            let elapsed =
                Duration::from_millis(motion.tick.saturating_sub(old.tick).saturating_mul(20));
            // Tick spacing, bounded by actual arrival, avoids speeding up a
            // burst of queued commits. Older server ticks are never reinstalled.
            let at = if self.samples.is_empty() {
                now
            } else {
                prior
                    .at
                    .checked_add(elapsed)
                    .unwrap_or(now)
                    .min(now)
                    .max(prior.at)
            };
            self.samples.push_back(Sample { avatar, at });
            while self.samples.len() > 8 {
                self.samples.pop_front();
            }
        }
        let at = now.checked_sub(DELAY).unwrap_or(now);
        while self.samples.len() > 2 && self.samples[1].at <= at {
            self.samples.pop_front();
        }
        let a = self.samples[0];
        let b = *self.samples.get(1).unwrap_or(&a);
        let span = b.at.saturating_duration_since(a.at).as_secs_f32();
        let t = if span > 0.0 {
            (at.saturating_duration_since(a.at).as_secs_f32() / span).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let mut visual = if t < 1.0 { a.avatar } else { b.avatar };
        visual.position = a.avatar.position.lerp(b.avatar.position, t);
        let orientation = glam::Quat::from_array(a.avatar.motion.unwrap().orientation)
            .normalize()
            .slerp(
                glam::Quat::from_array(b.avatar.motion.unwrap().orientation).normalize(),
                t,
            );
        visual.motion.as_mut().unwrap().orientation = orientation.to_array();
        visual.motion.as_mut().unwrap().velocity =
            glam::Vec3::from_array(a.avatar.motion.unwrap().velocity)
                .lerp(glam::Vec3::from_array(b.avatar.motion.unwrap().velocity), t)
                .to_array();
        visual.pose = [0.0; 4];
        visual.character_pose = [0.0; 4];
        visual
    }
}

#[cfg(test)]
mod tests;
