//! Local-only body visibility and arm framing; world pose and interaction eye stay authoritative.
use glam::{Mat4, Quat, Vec3};

#[derive(Clone, Copy)]
pub(crate) struct View {
    pub id: u64,
    pub eye_height: f32,
    pub pitch: f32,
}

impl View {
    pub(super) fn prepare_pose(self, pose: &mut [Mat4; 7]) {
        // Keep the chest behind the eye so it cannot fill the view when looking down.
        // This affects only the rendered owner; camera, authoritative position, and collision stay put.
        for joint in pose.iter_mut() {
            *joint = Mat4::from_translation(Vec3::new(0.0, 0.0, -0.18)) * *joint;
        }
        // Ease back to attached world arms when looking down at the body.
        let down = ((-self.pitch - 0.35) / 0.75).clamp(0.0, 1.0);
        let weight = 1.0 - down * down * (3.0 - 2.0 * down);
        let pitch = Quat::from_rotation_x(-self.pitch.clamp(-1.55, 1.55));
        for (index, side) in [(3, 1.0), (4, -1.0)] {
            let (scale, rotation, translation) = pose[index].to_scale_rotation_translation();
            // Keep the shoulder caps below the viewport; the authored swing
            // brings the hand into view without a resting pair of shoulders.
            let shoulder = Vec3::Y * self.eye_height + pitch * Vec3::new(side * 0.27, -0.65, 0.28);
            let framed = pitch * Quat::from_rotation_x(-0.7) * rotation;
            pose[index] = Mat4::from_scale_rotation_translation(
                scale,
                rotation.slerp(framed, weight),
                translation.lerp(shoulder, weight),
            );
        }
    }
}

#[cfg(test)]
mod tests;
