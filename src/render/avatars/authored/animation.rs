//! A bounded presentation clock, with interrupted fades starting from the
//! currently visible pose rather than jumping back to an older clip.
use crate::render::model_asset::{Model, Transform};
use glam::Mat4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Playback {
    pub clip: Option<usize>,
    pub serial: Option<(u64, u32)>,
    pub start_s: f32,
    pub speed: f32,
    pub looping: bool,
    pub fade_s: f32,
}
pub(super) struct Animator {
    current: Option<Playback>,
    time: f32,
    fade: f32,
    pub pose: Vec<Transform>,
    from: Vec<Transform>,
    worlds: Vec<Mat4>,
    pub matrices: Vec<Mat4>,
}
impl Animator {
    pub fn new(model: &Model) -> Self {
        let pose: Vec<_> = model.nodes.iter().map(|node| node.rest).collect();
        Self {
            current: None,
            time: 0.0,
            fade: 0.0,
            from: pose.clone(),
            pose,
            worlds: vec![Mat4::IDENTITY; model.nodes.len()],
            matrices: vec![Mat4::IDENTITY; model.bindings.len()],
        }
    }
    pub fn finished(&self, model: &Model) -> bool {
        self.current.is_some_and(|p| {
            !p.looping
                && p.clip
                    .and_then(|index| model.clips.get(index))
                    .is_some_and(|clip| self.time >= clip.duration)
        })
    }
    pub fn step(&mut self, model: &Model, target: Playback, dt: f32) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.1)
        } else {
            0.0
        };
        let changed = self
            .current
            .is_none_or(|old| old.clip != target.clip || old.serial != target.serial);
        if changed {
            self.from.copy_from_slice(&self.pose);
            self.time = target.start_s;
            self.fade = 0.0;
        } else {
            if target.serial.is_some() {
                self.time = target.start_s;
            } else {
                self.time += dt * target.speed;
            }
            self.fade += dt;
        }
        self.current = Some(target);
        if let Some(clip) = target.clip.and_then(|index| model.clips.get(index)) {
            if target.looping && clip.duration > 0.0 {
                self.time = self.time.rem_euclid(clip.duration);
            } else {
                self.time = self.time.min(clip.duration);
            }
        }
        // Registration and public-state validation establish these bounds. Keep
        // a rest-pose fallback if a bad caller ever violates that contract.
        if model
            .local_pose_index_into(target.clip, self.time, target.looping, &mut self.pose)
            .is_err()
        {
            for (pose, node) in self.pose.iter_mut().zip(&model.nodes) {
                *pose = node.rest;
            }
        }
        let weight = if target.fade_s > 0.0 {
            (self.fade / target.fade_s).clamp(0.0, 1.0)
        } else {
            1.0
        };
        // Smoothstep keeps both endpoints free of a visible velocity jump.
        let weight = weight * weight * (3.0 - 2.0 * weight);
        for (pose, from) in self.pose.iter_mut().zip(&self.from) {
            pose.translation = from.translation.lerp(pose.translation, weight);
            pose.scale = from.scale.lerp(pose.scale, weight);
            pose.rotation = from.rotation.slerp(pose.rotation, weight).normalize();
        }
        if model
            .matrices_into(&self.pose, &mut self.worlds, &mut self.matrices)
            .is_err()
        {
            self.matrices.fill(Mat4::IDENTITY);
        }
    }
}
