//! Validated, offline-converted authored character data. No runtime glTF parser.
//! Rigid vertices are stored in joint-local space, including inverse binds.
use glam::{Mat4, Quat, Vec3};
use serde::Deserialize;

pub(super) const JOINT_COUNT: usize = 7;
pub(super) const BODY_PNG: &[u8] = include_bytes!("../../../assets/models/player/body.png");
pub(super) const HAIR_PNG: &[u8] = include_bytes!("../../../assets/models/player/hair.png");

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CharacterVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub joint: usize,
    pub material: u32,
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
        Self::parse(include_str!("../../../assets/models/player/character.json"))
            .expect("checked-in authored character asset must validate")
    }

    pub fn parse(json: &str) -> Result<Self, String> {
        if json.len() > 2 * 1024 * 1024 {
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
        if self.version != 1
            || self.joints.len() != JOINT_COUNT
            || self.vertices.is_empty()
            || self.vertices.len() > 8192
            || self.indices.is_empty()
            || self.indices.len() > 24576
            || !self.indices.len().is_multiple_of(3)
            || self.clips.len() != 5
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
                || vertex.material > 1
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
        let expected = ["walk", "idle", "crouch", "tool_use_left", "tool_use_right"];
        for name in expected {
            if self.clips.iter().filter(|clip| clip.name == name).count() != 1 {
                return Err("missing or duplicate authored clip".into());
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
        // Source: 2 m tall, feet-origin, Y up, -Z forward. Engine: 1.8 blocks,
        // feet-origin, Y up, +Z forward. The proper rotation preserves winding.
        let basis = Mat4::from_scale_rotation_translation(
            Vec3::splat(0.9),
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
        self.matrices(self.local_pose(clip, seconds))
    }

    /// Blend local transforms before hierarchy evaluation, avoiding matrix lerp.
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
        self.matrices(std::array::from_fn(|index| LocalPose {
            translation: idle[index]
                .translation
                .lerp(walk[index].translation, weight),
            rotation: idle[index]
                .rotation
                .slerp(walk[index].rotation, weight)
                .normalize(),
        }))
    }
}

#[cfg(test)]
mod tests;
