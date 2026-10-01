//! Local-only arm framing; world pose and interaction eye stay authoritative.
use super::super::character_asset::{
    JOINT_COUNT,
    rig::{LEFT_ARM, RIGHT_ARM, grip_anchor},
    tool_duration,
};
use glam::{Mat4, Quat, Vec3};

#[derive(Clone, Copy)]
pub(crate) struct View {
    pub id: u64,
    pub eye_height: f32,
    pub pitch: f32,
}

impl View {
    pub(super) fn prepare_pose(self, pose: &mut [Mat4; JOINT_COUNT], tool: Option<(bool, f32)>) {
        // Shift only the owner's render body, never camera/collision/interaction.
        for joint in pose.iter_mut() {
            *joint = Mat4::from_translation(Vec3::new(0.0, 0.0, -0.35)) * *joint;
        }
        // Ease back to attached world arms when looking down at the body.
        let view_pitch = if self.pitch.is_finite() {
            self.pitch.clamp(-1.55, 1.55)
        } else {
            0.0
        };
        let down = ((-view_pitch - 0.35) / 0.75).clamp(0.0, 1.0);
        let weight = 1.0 - down * down * (3.0 - 2.0 * down);
        if weight == 0.0 {
            return;
        }
        let pitch = Quat::from_rotation_x(-view_pitch);
        for (arm, right) in [(RIGHT_ARM, true), (LEFT_ARM, false)] {
            let shoulder_matrix = pose[arm[1]];
            let (scale, rotation, translation) = shoulder_matrix.to_scale_rotation_translation();
            // Anatomical right is -X after the native-to-engine basis rotation.
            let side = grip_anchor(pose, right).w_axis.x.signum();
            // Resting shoulders remain below the viewport; the active arm rises
            // smoothly into view for the shorter native forearm/hand proportions.
            let lift = tool
                .filter(|(hand, time)| *hand == right && time.is_finite())
                .map_or(0.0, |(_, time)| {
                    let phase = (time / tool_duration(right)).clamp(0.0, 1.0);
                    (phase * std::f32::consts::PI).sin().powi(2) * 0.48
                });
            let shoulder =
                Vec3::Y * self.eye_height + pitch * Vec3::new(side * 0.27, -0.72 + lift, 0.24);
            let framed_rotation = pitch * Quat::from_rotation_x(-0.7) * rotation;
            let framed = Mat4::from_scale_rotation_translation(
                scale,
                rotation.slerp(framed_rotation, weight),
                translation.lerp(shoulder, weight),
            );
            let delta = framed * shoulder_matrix.inverse();
            // Apply the same rigid delta to clavicle, upper arm, forearm, wrist
            // and grip. Updating only the shoulder tears apart articulated arms.
            for joint in arm {
                pose[joint] = delta * pose[joint];
            }
        }
    }
}

#[cfg(test)]
mod tests;
