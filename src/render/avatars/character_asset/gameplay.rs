//! State blending of the GLB's baked clips. No procedural animation generators.
use super::{CharacterAsset, JOINT_COUNT, LocalPose, rig::*};
use glam::Mat4;
#[cfg(test)]
use glam::Vec3;

pub(crate) fn tool_duration(right: bool) -> f32 {
    CharacterAsset::builtin().duration(if right {
        "tool_use_right"
    } else {
        "tool_use_left"
    })
}
fn weight(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}
pub(super) fn blend(a: LocalPose, b: LocalPose, w: f32) -> LocalPose {
    let w = weight(w);
    LocalPose {
        translation: a.translation.lerp(b.translation, w),
        rotation: a.rotation.slerp(b.rotation, w).normalize(),
        scale: a.scale.lerp(b.scale, w),
    }
}
fn overlay(base: LocalPose, rest: LocalPose, layer: LocalPose) -> LocalPose {
    LocalPose {
        translation: base.translation + layer.translation - rest.translation,
        rotation: (base.rotation * rest.rotation.inverse() * layer.rotation).normalize(),
        scale: base.scale * layer.scale / rest.scale,
    }
}
impl CharacterAsset {
    pub(super) fn duration(&self, name: &str) -> f32 {
        self.animation
            .clips
            .iter()
            .find(|c| c.name == name)
            .expect("required player clip")
            .duration
    }
    #[cfg(test)]
    pub fn sample_gameplay(
        &self,
        idle: f32,
        walk: f32,
        w: f32,
        crouch: f32,
        tool: Option<(bool, f32)>,
    ) -> [Mat4; JOINT_COUNT] {
        self.sample_gameplay_look(idle, walk, w, 0.0, crouch, tool, [0.0; 2])
    }
    #[allow(clippy::too_many_arguments)]
    pub fn sample_gameplay_look(
        &self,
        idle: f32,
        walk: f32,
        w: f32,
        run: f32,
        crouch: f32,
        tool: Option<(bool, f32)>,
        look: [f32; 2],
    ) -> [Mat4; JOINT_COUNT] {
        let idle = self.local_pose("idle", idle);
        // The client presents a normalized 0.8-second stride clock. Different
        // authored cycle durations must stay phase-aligned during run blending.
        let walking = self.local_pose("walk", walk * self.duration("walk") / 0.8);
        let running = self.local_pose("run", walk * self.duration("run") / 0.8);
        let crouching = self.local_pose("crouch", self.duration("crouch"));
        let mut pose = std::array::from_fn(|i| {
            blend(
                blend(idle[i], blend(walking[i], running[i], run), w),
                crouching[i],
                crouch,
            )
        });
        if let Some((right, time)) = tool.filter(|(_, t)| t.is_finite() && *t >= 0.0) {
            let rest = self.local_pose("rest", 0.0);
            let tool = self.local_pose(
                if right {
                    "tool_use_right"
                } else {
                    "tool_use_left"
                },
                time,
            );
            for i in [SPINE, CHEST, NECK, HEAD]
                .into_iter()
                .chain(RIGHT_ARM)
                .chain(LEFT_ARM)
            {
                pose[i] = overlay(pose[i], rest[i], tool[i]);
            }
        }
        apply_look(&mut pose, look);
        let mut matrices = self.matrices(pose);
        self.ground(&mut matrices);
        matrices
    }
}
#[cfg(test)]
mod tests;
