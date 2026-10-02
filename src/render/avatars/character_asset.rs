//! Native GLB player asset. Static cube transforms are folded into rigid vertices;
//! authored tracks retain the thirty named actor joints used by first-person grips.
use crate::render::model_asset::{Model, Transform};
use glam::{Mat4, Vec3};
use std::sync::OnceLock;
mod gameplay;
mod load;
pub(crate) use gameplay::tool_duration;
pub(super) mod rig;
pub(super) const JOINT_COUNT: usize = 30;
pub(super) const MATERIAL_COUNT: usize = 15;

pub(super) struct CharacterVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub joint: usize,
    /// Body/hair draw group, independent of the embedded texture index.
    pub material: u32,
    pub surface: u32,
    pub texture: u32,
}
#[cfg(test)]
pub(super) struct Joint {
    pub name: String,
    pub parent: Option<usize>,
}
pub(super) struct CharacterAsset {
    #[cfg(test)]
    pub joints: Vec<Joint>,
    pub vertices: Vec<CharacterVertex>,
    pub indices: Vec<u32>,
    pub animation: Model,
    pub images: Vec<crate::render::model_asset::Image>,
    soles: Vec<(usize, Vec3)>,
}
type LocalPose = Transform;
impl CharacterAsset {
    pub fn builtin() -> &'static Self {
        static ASSET: OnceLock<CharacterAsset> = OnceLock::new();
        ASSET.get_or_init(|| load::load().expect("checked-in player GLB must validate"))
    }
    fn local_pose(&self, clip: &str, seconds: f32) -> [LocalPose; JOINT_COUNT] {
        let seconds = if seconds.is_finite() {
            seconds.max(0.0)
        } else {
            0.0
        };
        let clip = (clip != "rest").then_some(clip);
        let mut pose = std::array::from_fn(|i| self.animation.nodes[i].rest);
        self.animation
            .local_pose_into(clip, seconds, &mut pose)
            .expect("validated player clip");
        pose
    }
    fn matrices(&self, pose: [LocalPose; JOINT_COUNT]) -> [Mat4; JOINT_COUNT] {
        let basis = Mat4::from_rotation_y(std::f32::consts::PI);
        let mut matrices = [Mat4::IDENTITY; JOINT_COUNT];
        for (i, local) in pose.into_iter().enumerate() {
            let parent = self.animation.nodes[i]
                .parent
                .map_or(basis, |p| matrices[p]);
            matrices[i] = parent
                * Mat4::from_scale_rotation_translation(
                    local.scale,
                    local.rotation,
                    local.translation,
                );
        }
        matrices
    }

    pub fn sample(&self, clip: &str, seconds: f32) -> [Mat4; JOINT_COUNT] {
        self.matrices(self.local_pose(clip, seconds))
    }
    fn ground(&self, pose: &mut [Mat4; JOINT_COUNT]) {
        let lowest = self
            .soles
            .iter()
            .map(|(joint, point)| pose[*joint].transform_point3(*point).y)
            .fold(f32::INFINITY, f32::min);
        for m in pose {
            m.w_axis.y -= lowest;
        }
    }
    #[cfg(test)]
    pub fn sample_blended(&self, idle: f32, walk: f32, weight: f32) -> [Mat4; JOINT_COUNT] {
        let a = self.local_pose("idle", idle);
        let b = self.local_pose("walk", walk);
        self.matrices(std::array::from_fn(|i| gameplay::blend(a[i], b[i], weight)))
    }
}
#[cfg(test)]
mod tests;
