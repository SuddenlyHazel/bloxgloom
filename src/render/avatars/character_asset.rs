//! Validated, offline-converted authored character data. No runtime glTF parser.
//! Rigid vertices are stored in joint-local space, including inverse binds.
use glam::{Mat4, Quat, Vec3};
use serde::Deserialize;
mod gameplay;
pub(crate) use gameplay::tool_duration;
mod mesh;
pub(super) mod occlusion;

pub(super) const JOINT_COUNT: usize = 30;
pub(super) mod rig;
pub(super) const MATERIAL_COUNT: usize = 15;
pub(super) const BODY_PNG: &[u8] =
    include_bytes!("../../../assets/models/player/articulated/flat_chest.png");
pub(super) const BODY_DEFINED_PNG: &[u8] =
    include_bytes!("../../../assets/models/player/articulated/defined_chest_sports_bra.png");
pub(super) const HAIR_PNGS: [&[u8]; 26] = [
    include_bytes!("../../../assets/models/player/articulated/tousled_crop_neutral.png"),
    include_bytes!("../../../assets/models/player/articulated/tousled_crop_accessory.png"),
    include_bytes!("../../../assets/models/player/articulated/side_swept_undercut_neutral.png"),
    include_bytes!("../../../assets/models/player/articulated/side_swept_undercut_accessory.png"),
    include_bytes!("../../../assets/models/player/articulated/space_buns_neutral.png"),
    include_bytes!("../../../assets/models/player/articulated/space_buns_accessory.png"),
    include_bytes!("../../../assets/models/player/articulated/curly_bob_neutral.png"),
    include_bytes!("../../../assets/models/player/articulated/curly_bob_accessory.png"),
    include_bytes!("../../../assets/models/player/articulated/curly_pigtails_neutral.png"),
    include_bytes!("../../../assets/models/player/articulated/curly_pigtails_accessory.png"),
    include_bytes!("../../../assets/models/player/articulated/sidepart_bob_neutral.png"),
    include_bytes!("../../../assets/models/player/articulated/sidepart_bob_accessory.png"),
    include_bytes!("../../../assets/models/player/articulated/compact_braid_neutral.png"),
    include_bytes!("../../../assets/models/player/articulated/compact_braid_accessory.png"),
    include_bytes!("../../../assets/models/player/articulated/long_loose_curls_neutral.png"),
    include_bytes!("../../../assets/models/player/articulated/long_loose_curls_accessory.png"),
    include_bytes!("../../../assets/models/player/articulated/long_curly_ponytail_neutral.png"),
    include_bytes!("../../../assets/models/player/articulated/long_curly_ponytail_accessory.png"),
    include_bytes!("../../../assets/models/player/articulated/half_up_curly_cascade_neutral.png"),
    include_bytes!("../../../assets/models/player/articulated/half_up_curly_cascade_accessory.png"),
    include_bytes!("../../../assets/models/player/articulated/rounded_afro_neutral.png"),
    include_bytes!("../../../assets/models/player/articulated/rounded_afro_accessory.png"),
    include_bytes!("../../../assets/models/player/articulated/twin_braids_neutral.png"),
    include_bytes!("../../../assets/models/player/articulated/twin_braids_accessory.png"),
    include_bytes!("../../../assets/models/player/articulated/curly_mohawk_neutral.png"),
    include_bytes!("../../../assets/models/player/articulated/curly_mohawk_accessory.png"),
];
pub(super) const CLEAN_FACE_PNG: &[u8] =
    include_bytes!("../../../assets/models/player/face/clean.png");
pub(super) const EYE_PNGS: [&[u8]; 8] = [
    include_bytes!("../../../assets/models/player/face/eyes/0.png"),
    include_bytes!("../../../assets/models/player/face/eyes/1.png"),
    include_bytes!("../../../assets/models/player/face/eyes/2.png"),
    include_bytes!("../../../assets/models/player/face/eyes/3.png"),
    include_bytes!("../../../assets/models/player/face/eyes/4.png"),
    include_bytes!("../../../assets/models/player/face/eyes/5.png"),
    include_bytes!("../../../assets/models/player/face/eyes/6.png"),
    include_bytes!("../../../assets/models/player/face/eyes/7.png"),
];
pub(super) const MOUTH_PNGS: [&[u8]; 6] = [
    include_bytes!("../../../assets/models/player/face/mouths/0.png"),
    include_bytes!("../../../assets/models/player/face/mouths/1.png"),
    include_bytes!("../../../assets/models/player/face/mouths/2.png"),
    include_bytes!("../../../assets/models/player/face/mouths/3.png"),
    include_bytes!("../../../assets/models/player/face/mouths/4.png"),
    include_bytes!("../../../assets/models/player/face/mouths/5.png"),
];
pub(super) const IRIS_MASK_PNGS: [&[u8]; 8] = [
    include_bytes!("../../../assets/models/player/face/masks/0.png"),
    include_bytes!("../../../assets/models/player/face/masks/1.png"),
    include_bytes!("../../../assets/models/player/face/masks/2.png"),
    include_bytes!("../../../assets/models/player/face/masks/3.png"),
    include_bytes!("../../../assets/models/player/face/masks/4.png"),
    include_bytes!("../../../assets/models/player/face/masks/5.png"),
    include_bytes!("../../../assets/models/player/face/masks/6.png"),
    include_bytes!("../../../assets/models/player/face/masks/7.png"),
];
pub(super) const EYE_NAMES: [&str; 8] = [
    "classic",
    "cute_glint",
    "kawaii_star",
    "playful_wink",
    "happy_crescent",
    "neon_focus",
    "neon_curious",
    "soft_sleepy",
];
pub(super) const MOUTH_NAMES: [&str; 6] = [
    "classic",
    "soft_smile",
    "cat_smile",
    "tiny_open",
    "playful",
    "smirk",
];

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CharacterVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub joint: usize,
    pub material: u32,
    pub surface: u32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Joint {
    pub name: String,
    pub parent: Option<usize>,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Clip {
    pub name: String,
    pub duration: f32,
    pub looping: bool,
    channels: Vec<Channel>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ChannelPath {
    Translation,
    Rotation,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Channel {
    joint: usize,
    path: ChannelPath,
    times: Vec<f32>,
    /// Translation uses xyz; w is padding. Rotation is glTF xyzw.
    values: Vec<[f32; 4]>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CharacterAsset {
    version: u32,
    pub joints: Vec<Joint>,
    pub vertices: Vec<CharacterVertex>,
    pub indices: Vec<u32>,
    pub clips: Vec<Clip>,
}

#[derive(Clone, Copy)]
struct LocalPose {
    translation: Vec3,
    rotation: Quat,
}

impl CharacterAsset {
    pub fn builtin() -> Self {
        Self::from_builtin_parts().expect("checked-in authored character asset must validate")
    }

    #[cfg(test)]
    pub fn parse(json: &str) -> Result<Self, String> {
        if json.len() > 4 * 1024 * 1024 {
            return Err("character asset exceeds byte limit".into());
        }
        let asset: Self = serde_json::from_str(json).map_err(|error| error.to_string())?;
        asset.validate()?;
        Ok(asset)
    }

    fn validate(&self) -> Result<(), String> {
        let finite = |values: &[f32]| values.iter().all(|value| value.is_finite());
        let rotation = |values: [f32; 4]| {
            finite(&values) && (Quat::from_array(values).length_squared() - 1.0).abs() < 0.001
        };
        if self.version != 2
            || self.joints.len() != JOINT_COUNT
            || self.vertices.is_empty()
            || self.vertices.len() > 65536
            || self.indices.is_empty()
            || self.indices.len() > 196608
            || !self.indices.len().is_multiple_of(3)
            || self.clips.len() != 4
        {
            return Err("invalid character shape or version".into());
        }
        for (index, joint) in self.joints.iter().enumerate() {
            if joint.name.is_empty()
                || joint.name.len() > 32
                || joint.parent.is_some_and(|parent| parent >= index)
                || !finite(&joint.translation)
                || !rotation(joint.rotation)
            {
                return Err("invalid character joint".into());
            }
        }
        if self.joints[0].parent.is_some()
            || self.joints[1..].iter().any(|joint| joint.parent.is_none())
        {
            return Err("character must have one connected joint hierarchy".into());
        }
        for vertex in &self.vertices {
            if vertex.joint >= JOINT_COUNT
                || vertex.material as usize >= MATERIAL_COUNT
                || vertex.surface > 9
                || ((1..14).contains(&vertex.material) && vertex.joint != 5)
                || !finite(&vertex.position)
                || !finite(&vertex.normal)
                || !finite(&vertex.uv)
                || vertex.position.iter().any(|v| v.abs() > 8.0)
                || (Vec3::from_array(vertex.normal).length_squared() - 1.0).abs() > 0.001
                || vertex.uv.iter().any(|v| !(0.0..=1.0).contains(v))
            {
                return Err("invalid character vertex".into());
            }
        }
        if self
            .indices
            .iter()
            .any(|&index| index as usize >= self.vertices.len())
        {
            return Err("character triangle index out of bounds".into());
        }
        let mut counts = [0; MATERIAL_COUNT];
        let mut material = 0;
        for triangle in self.indices.chunks_exact(3) {
            let next = self.vertices[triangle[0] as usize].material as usize;
            if triangle
                .iter()
                .any(|&i| self.vertices[i as usize].material as usize != next)
                || next < material
                || next > material + 1
            {
                return Err("character material ranges must be contiguous and ordered".into());
            }
            material = next;
            counts[next] += 3;
        }
        if counts.iter().any(|&count| count == 0 || count > 49152) {
            return Err("character material index budget exceeded".into());
        }
        for name in [
            "crouch_test",
            "grip_test",
            "weight_shift",
            "wrist_ankle_test",
        ] {
            if self.clips.iter().filter(|clip| clip.name == name).count() != 1 {
                return Err("missing or duplicate source clip".into());
            }
        }
        for clip in &self.clips {
            if !clip.duration.is_finite()
                || !(0.01..=30.0).contains(&clip.duration)
                || clip.channels.len() > JOINT_COUNT * 2
            {
                return Err("invalid animation duration or channels".into());
            }
            let mut seen = [[false; 2]; JOINT_COUNT];
            for channel in &clip.channels {
                let path = match channel.path {
                    ChannelPath::Translation => 0,
                    ChannelPath::Rotation => 1,
                };
                if channel.joint >= JOINT_COUNT
                    || channel.times.is_empty()
                    || channel.times.len() > 2048
                    || channel.times.len() != channel.values.len()
                    || !finite(&channel.times)
                    || channel.times[0] < 0.0
                    || channel
                        .times
                        .last()
                        .is_some_and(|&time| time > clip.duration)
                    || channel.times.windows(2).any(|pair| pair[0] >= pair[1])
                    || channel.values.iter().any(|value| !finite(value))
                    || (path == 0
                        && channel
                            .values
                            .iter()
                            .flatten()
                            .any(|value| value.abs() > 8.0))
                    || (path == 1 && channel.values.iter().any(|&value| !rotation(value)))
                    || seen[channel.joint][path]
                {
                    return Err("invalid animation channel".into());
                }
                seen[channel.joint][path] = true;
            }
        }
        Ok(())
    }

    fn local_pose(&self, name: &str, seconds: f32) -> [LocalPose; JOINT_COUNT] {
        let mut pose = std::array::from_fn(|index| LocalPose {
            translation: Vec3::from_array(self.joints[index].translation),
            rotation: Quat::from_array(self.joints[index].rotation),
        });
        if gameplay::animate(name, seconds, &mut pose) {
            return pose;
        }
        let Some(clip) = self.clips.iter().find(|clip| clip.name == name) else {
            return pose;
        };
        let seconds = if seconds.is_finite() {
            seconds.max(0.0)
        } else {
            0.0
        };
        let time = if clip.looping {
            seconds.rem_euclid(clip.duration)
        } else {
            seconds.min(clip.duration)
        };
        for channel in &clip.channels {
            let upper = channel
                .times
                .partition_point(|&key| key <= time)
                .min(channel.times.len() - 1);
            let lower = upper.saturating_sub(1);
            let span = channel.times[upper] - channel.times[lower];
            let blend = if span > 0.0 {
                ((time - channel.times[lower]) / span).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let a = channel.values[lower];
            let b = channel.values[upper];
            match channel.path {
                ChannelPath::Translation => {
                    pose[channel.joint].translation =
                        Vec3::new(a[0], a[1], a[2]).lerp(Vec3::new(b[0], b[1], b[2]), blend);
                }
                ChannelPath::Rotation => {
                    pose[channel.joint].rotation = Quat::from_array(a)
                        .slerp(Quat::from_array(b), blend)
                        .normalize();
                }
            }
        }
        pose
    }

    fn matrices(&self, pose: [LocalPose; JOINT_COUNT]) -> [Mat4; JOINT_COUNT] {
        // Source: 1.8 m tall, feet-origin, Y up, -Z forward. Engine: 1.8 blocks,
        // feet-origin, Y up, +Z forward. The proper rotation preserves winding.
        let basis = Mat4::from_scale_rotation_translation(
            Vec3::ONE,
            Quat::from_rotation_y(std::f32::consts::PI),
            Vec3::ZERO,
        );
        let mut matrices = [Mat4::IDENTITY; JOINT_COUNT];
        for (index, local) in pose.into_iter().enumerate() {
            let parent = self.joints[index]
                .parent
                .map_or(basis, |parent| matrices[parent]);
            matrices[index] =
                parent * Mat4::from_rotation_translation(local.rotation, local.translation);
        }
        matrices
    }

    /// Unknown clip names return the bind/rest pose. One-shots clamp, never wrap.
    pub fn sample(&self, clip: &str, seconds: f32) -> [Mat4; JOINT_COUNT] {
        let mut matrices = self.matrices(self.local_pose(clip, seconds));
        if matches!(
            clip,
            "idle" | "walk" | "run" | "crouch" | "tool_use_left" | "tool_use_right"
        ) {
            gameplay::ground(&mut matrices);
        }
        matrices
    }

    /// Blend local transforms before hierarchy evaluation, avoiding matrix lerp.
    #[cfg(test)]
    pub fn sample_blended(
        &self,
        idle_time: f32,
        walk_time: f32,
        walk_weight: f32,
    ) -> [Mat4; JOINT_COUNT] {
        let idle = self.local_pose("idle", idle_time);
        let walk = self.local_pose("walk", walk_time);
        let weight = if walk_weight.is_finite() {
            walk_weight.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let mut matrices = self.matrices(std::array::from_fn(|index| LocalPose {
            translation: idle[index]
                .translation
                .lerp(walk[index].translation, weight),
            rotation: idle[index]
                .rotation
                .slerp(walk[index].rotation, weight)
                .normalize(),
        }));
        gameplay::ground(&mut matrices);
        matrices
    }
}

#[cfg(test)]
mod tests;
