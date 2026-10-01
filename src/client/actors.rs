//! Bounded presentation history for replicated actors. Never writes positions
//! back to replicas or drives server gameplay. Missing updates freeze, not drift.
use crate::render::{AvatarModel, MAX_AVATARS, VisualAvatar};
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

mod moving;

const DELAY: Duration = Duration::from_millis(80);
const STEP: Duration = Duration::from_millis(40);

#[derive(Clone, Copy)]
struct Sample {
    at: Instant,
    avatar: VisualAvatar,
}

struct Track {
    samples: VecDeque<Sample>,
    last_position: glam::Vec3,
    last_frame: Instant,
    yaw: f32,
    stride: f32,
    gait: f32,
    run: f32,
    crouch: f32,
    look: [f32; 2],
    age: f32,
    airborne: bool,
    landing: f32,
}

impl Track {
    fn new(avatar: VisualAvatar, now: Instant) -> Self {
        Self {
            samples: VecDeque::from([Sample { at: now, avatar }]),
            last_position: avatar.position,
            last_frame: now,
            yaw: avatar.pose[0],
            stride: 0.0,
            gait: 0.0,
            run: 0.0,
            crouch: avatar.character_crouch.clamp(0.0, 1.0),
            look: [0.0; 2],
            age: (avatar.id % 100) as f32,
            airborne: avatar.airborne,
            landing: 0.0,
        }
    }

    fn update(&mut self, avatar: VisualAvatar, now: Instant) -> VisualAvatar {
        self.update_mode(avatar, now, false)
    }

    fn update_mode(&mut self, avatar: VisualAvatar, now: Instant, local: bool) -> VisualAvatar {
        let previous = *self.samples.back().unwrap();
        if previous.avatar.model != avatar.model
            || previous.avatar.position.distance(avatar.position) > 4.0
        {
            *self = Self::new(avatar, now); // spawn/model replacement/teleport discontinuity
        } else if previous.avatar.position != avatar.position
            || previous.avatar.pose[0] != avatar.pose[0]
            || previous.avatar.airborne != avatar.airborne
            || previous.avatar.cosmetics != avatar.cosmetics
            || previous.avatar.character_recipe != avatar.character_recipe
        {
            if now.duration_since(previous.at) > Duration::from_millis(200) {
                self.samples.clear();
                self.samples.push_back(Sample {
                    at: now - STEP,
                    ..previous
                });
            }
            self.samples.push_back(Sample { at: now, avatar });
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
        let span = b.at.duration_since(a.at).as_secs_f32();
        let t = if span > 0.0 {
            (at.saturating_duration_since(a.at).as_secs_f32() / span).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let mut visual = if local {
            avatar
        } else if t < 1.0 {
            a.avatar
        } else {
            b.avatar
        };
        if !local {
            visual.position = a.avatar.position.lerp(b.avatar.position, t);
        }
        let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;
        let delta = visual.position - self.last_position;
        self.last_position = visual.position;
        let distance = glam::Vec2::new(delta.x, delta.z).length();
        let desired_yaw = if local {
            avatar.pose[0]
        } else if distance > 0.0001 {
            delta.x.atan2(delta.z)
        } else if visual.model == AvatarModel::Player {
            // Players currently replicate position, not a look vector. Preserve
            // their last movement heading instead of turning north when idle.
            self.yaw
        } else {
            visual.pose[0]
        };
        let angle = (desired_yaw - self.yaw + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        if local {
            self.yaw = desired_yaw;
        } else {
            self.yaw += angle * (1.0 - (-14.0 * dt).exp());
        }
        visual.pose[0] = self.yaw;
        if visual.model == AvatarModel::Player {
            // Authored locomotion is presentation-only. Distance drives walk phase;
            // a stale snapshot stops feet rather than inventing movement.
            self.age += dt;
            let moving = if distance > 0.0001 && !visual.airborne {
                1.0
            } else {
                0.0
            };
            self.gait += (moving - self.gait) * (1.0 - (-18.0 * dt).exp());
            // The authored walk cycle is 0.8 seconds. Cap playback at 1.25x
            // so fast/flying movement cannot turn it into a nine-cycle/sec blur.
            let phase_step = (distance / 4.0).min(dt * 1.25);
            self.stride = (self.stride + phase_step).rem_euclid(3600.0);
            visual.character_pose[0] = self.stride;
            visual.character_pose[1] = self.age;
            visual.character_pose[2] = self.gait;
            // No sprint intent is replicated: derive the gait from actual
            // presented ground speed. Custom movement rates remain supported.
            let speed = if dt > 0.0 { distance / dt } else { 0.0 };
            let running = ((speed - 3.5) / 3.5).clamp(0.0, 1.0) * moving;
            self.run += (running - self.run) * (1.0 - (-10.0 * dt).exp());
            visual.character_pose[3] = self.run;
            self.crouch += (avatar.character_crouch - self.crouch) * (1.0 - (-16.0 * dt).exp());
            visual.character_crouch = self.crouch;
            let heading = (desired_yaw - self.yaw + std::f32::consts::PI)
                .rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            for (axis, target) in [heading, avatar.character_look[1]].into_iter().enumerate() {
                let limit = [20.0_f32, 5.0][axis].to_radians();
                let target = if target.is_finite() {
                    target.clamp(-limit, limit)
                } else {
                    0.0
                };
                self.look[axis] += (target - self.look[axis]) * (1.0 - (-14.0 * dt).exp());
            }
            visual.character_look = self.look;
        }
        if matches!(visual.model, AvatarModel::Registered(_)) {
            let animation = visual.animation;
            self.age += dt;
            if self.airborne && !visual.airborne {
                self.landing = 1.0;
            }
            self.airborne = visual.airborne;
            self.landing = (self.landing - dt * 5.0).max(0.0);
            let moving = if !visual.airborne && distance > 0.0001 {
                1.0
            } else {
                0.0
            };
            self.gait += (moving - self.gait) * (1.0 - (-18.0 * dt).exp());
            self.stride =
                (self.stride + distance * animation.stride_rate).rem_euclid(std::f32::consts::TAU);
            visual.pose[1] = self.stride.sin() * self.gait * animation.stride_amplitude;
            visual.pose[2] = (self.age * animation.idle_rate).sin() * animation.idle_bob
                + self.stride.sin().abs() * self.gait * animation.walk_bob;
            visual.pose[3] = if visual.airborne {
                -animation.fall_stretch
            } else {
                self.landing * animation.landing_squash
            };
        }
        visual
    }
}

#[derive(Default)]
pub(crate) struct ActorAnimator {
    tracks: HashMap<u64, Track>,
    moving: HashMap<u64, moving::Track>,
}

impl ActorAnimator {
    pub(crate) fn present(&mut self, avatars: &mut [VisualAvatar], now: Instant) {
        self.present_with_local(avatars, now, None);
    }

    pub(crate) fn present_with_local(
        &mut self,
        avatars: &mut [VisualAvatar],
        now: Instant,
        local: Option<u64>,
    ) {
        let ids: HashSet<_> = avatars.iter().take(MAX_AVATARS).map(|a| a.id).collect();
        self.tracks.retain(|id, _| ids.contains(id));
        self.moving.retain(|id, _| ids.contains(id));
        for avatar in avatars.iter_mut().take(MAX_AVATARS) {
            if avatar.motion.is_some() {
                self.tracks.remove(&avatar.id);
                let track = self
                    .moving
                    .entry(avatar.id)
                    .or_insert_with(|| moving::Track::new(*avatar, now));
                *avatar = track.update(*avatar, now);
                continue;
            }
            self.moving.remove(&avatar.id);
            let track = self
                .tracks
                .entry(avatar.id)
                .or_insert_with(|| Track::new(*avatar, now));
            *avatar = if Some(avatar.id) == local {
                track.update_mode(*avatar, now, true)
            } else {
                track.update(*avatar, now)
            };
        }
    }
}

#[cfg(test)]
mod tests;
