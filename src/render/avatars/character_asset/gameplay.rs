//! Local-transform overlays keep locomotion active beneath crouch and tool use.
use super::{CharacterAsset, JOINT_COUNT, LocalPose};
use glam::{Mat4, Quat};

/// Both checked-in anatomical tool clips last 0.8 seconds and end at rest.
pub(crate) fn tool_duration(_right: bool) -> f32 {
    0.8
}

fn weight(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn blend(a: LocalPose, b: LocalPose, weight: f32) -> LocalPose {
    LocalPose {
        translation: a.translation.lerp(b.translation, weight),
        rotation: a.rotation.slerp(b.rotation, weight).normalize(),
    }
}

fn overlay(base: LocalPose, rest: LocalPose, layer: LocalPose, weight: f32) -> LocalPose {
    let rotation_delta = rest.rotation.inverse() * layer.rotation;
    LocalPose {
        translation: base.translation + (layer.translation - rest.translation) * weight,
        rotation: (base.rotation * Quat::IDENTITY.slerp(rotation_delta, weight)).normalize(),
    }
}

impl CharacterAsset {
    /// Crouch uses its held pose; tools clamp rather than loop. Tool overlays
    /// affect only upper-body joints, leaving the lowered root and walking legs.
    pub fn sample_gameplay(
        &self,
        idle_time: f32,
        walk_time: f32,
        walk_weight: f32,
        crouch_weight: f32,
        tool: Option<(bool, f32)>,
    ) -> [Mat4; JOINT_COUNT] {
        let idle = self.local_pose("idle", idle_time);
        let walk = self.local_pose("walk", walk_time);
        let walk_weight = weight(walk_weight);
        let crouch_weight = weight(crouch_weight);
        let mut pose = std::array::from_fn(|index| blend(idle[index], walk[index], walk_weight));
        if crouch_weight == 0.0 && tool.is_none() {
            return self.matrices(pose);
        }
        let rest = self.local_pose("rest", 0.0);
        if crouch_weight > 0.0 {
            let crouch = self.local_pose("crouch", f32::MAX);
            for index in 0..JOINT_COUNT {
                pose[index] = overlay(pose[index], rest[index], crouch[index], crouch_weight);
            }
        }
        if let Some((right, seconds)) = tool {
            let tool = self.local_pose(
                if right {
                    "tool_use_right"
                } else {
                    "tool_use_left"
                },
                seconds,
            );
            for (index, joint) in self.joints.iter().enumerate() {
                if matches!(
                    joint.name.as_str(),
                    "head" | "torso" | "right_arm" | "left_arm"
                ) {
                    pose[index] = overlay(pose[index], rest[index], tool[index], 1.0);
                }
            }
        }
        self.matrices(pose)
    }
}
