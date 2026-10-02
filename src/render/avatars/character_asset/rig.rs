//! Named GLB actor joints. Cube groups are folded into this bounded render rig.
use super::{JOINT_COUNT, LocalPose};
use glam::{EulerRot, Mat4, Quat};

#[cfg(test)]
pub(in crate::render::avatars) const ROOT: usize = 0;
#[cfg(test)]
pub(in crate::render::avatars) const PELVIS: usize = 1;
pub(in crate::render::avatars) const SPINE: usize = 2;
pub(in crate::render::avatars) const CHEST: usize = 3;
pub(in crate::render::avatars) const NECK: usize = 4;
pub(in crate::render::avatars) const HEAD: usize = 5;
pub(in crate::render::avatars) const RIGHT_ARM: [usize; 5] = [6, 7, 8, 9, 10];
pub(in crate::render::avatars) const LEFT_ARM: [usize; 5] = [18, 19, 20, 21, 22];
pub(in crate::render::avatars) const RIGHT_LEG: [usize; 4] = [11, 12, 13, 14];
pub(in crate::render::avatars) const LEFT_LEG: [usize; 4] = [23, 24, 25, 26];

/// Native grip matrices include the wrist and every ancestor, in engine space.
/// Apply the actor transform and the item's local grip offset after this helper.
/// For the owner, pass the first-person framed pose, not the world pose.
pub(in crate::render::avatars) fn grip_anchor(pose: &[Mat4; JOINT_COUNT], right: bool) -> Mat4 {
    pose[if right { RIGHT_ARM[4] } else { LEFT_ARM[4] }]
}

pub(in crate::render::avatars) fn clamp_look(look: [f32; 2]) -> [f32; 2] {
    std::array::from_fn(|axis| {
        let limit = [20.0_f32, 5.0][axis].to_radians();
        if look[axis].is_finite() {
            look[axis].clamp(-limit, limit)
        } else {
            0.0
        }
    })
}

/// Clamp the final local head rotation, including animation, to the hair-tested
/// envelope. Positive input pitch looks up in engine space (+Z forward).
pub(super) fn apply_look(pose: &mut [LocalPose; JOINT_COUNT], look: [f32; 2]) {
    let (yaw, pitch, roll) = pose[HEAD].rotation.to_euler(EulerRot::YXZ);
    let look = clamp_look([yaw + look[0], pitch + look[1]]);
    pose[HEAD].rotation = Quat::from_euler(EulerRot::YXZ, look[0], look[1], roll);
}

pub(super) const NAMES: [&str; JOINT_COUNT] = [
    "root",
    "pelvis",
    "spine",
    "chest",
    "neck",
    "head",
    "clavicle_R",
    "upper_arm_R",
    "forearm_R",
    "hand_R",
    "grip_R",
    "thigh_R",
    "shin_R",
    "foot_R",
    "toe_R",
    "eye_R",
    "pupil_R",
    "brow_R",
    "clavicle_L",
    "upper_arm_L",
    "forearm_L",
    "hand_L",
    "grip_L",
    "thigh_L",
    "shin_L",
    "foot_L",
    "toe_L",
    "eye_L",
    "pupil_L",
    "brow_L",
];
