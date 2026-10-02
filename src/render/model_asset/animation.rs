//! Authored TRS sampling and pose blending, including baked cubic spline tracks.
use super::*;
use gltf::animation::{Interpolation, Property};

pub(crate) struct Channel {
    pub node: usize,
    pub property: Property,
    pub interpolation: Interpolation,
    pub times: Vec<f32>,
    pub values: Vec<[f32; 4]>,
}
pub(crate) struct Clip {
    pub name: String,
    pub duration: f32,
    pub channels: Vec<Channel>,
}
impl Channel {
    fn sample(&self, time: f32) -> [f32; 4] {
        let next = self.times.partition_point(|&t| t <= time);
        let a = next.saturating_sub(1);
        let b = next.min(self.times.len() - 1);
        let key = |i| {
            self.values[if self.interpolation == Interpolation::CubicSpline {
                i * 3 + 1
            } else {
                i
            }]
        };
        if a == b || self.interpolation == Interpolation::Step {
            return key(a);
        }
        let dt = self.times[b] - self.times[a];
        let t = ((time - self.times[a]) / dt).clamp(0.0, 1.0);
        let (av, bv) = (key(a), key(b));
        if self.interpolation == Interpolation::Linear && self.property == Property::Rotation {
            return Quat::from_array(av)
                .slerp(Quat::from_array(bv), t)
                .normalize()
                .to_array();
        }
        let mut value = std::array::from_fn(|i| av[i] + (bv[i] - av[i]) * t);
        if self.interpolation == Interpolation::CubicSpline {
            let (t2, t3) = (t * t, t * t * t);
            value = std::array::from_fn(|i| {
                (2.0 * t3 - 3.0 * t2 + 1.0) * av[i]
                    + (t3 - 2.0 * t2 + t) * dt * self.values[a * 3 + 2][i]
                    + (-2.0 * t3 + 3.0 * t2) * bv[i]
                    + (t3 - t2) * dt * self.values[b * 3][i]
            });
            if self.property == Property::Rotation {
                let q = Quat::from_array(value);
                if q.length_squared() > 1e-12 {
                    value = q.normalize().to_array();
                }
            }
        }
        value
    }
}
impl Model {
    pub(crate) fn local_pose_into(
        &self,
        name: Option<&str>,
        time: f32,
        pose: &mut [Transform],
    ) -> Result<()> {
        ensure(
            pose.len() == self.nodes.len(),
            "local pose has the wrong node count",
        )?;
        ensure(
            time.is_finite() && time >= 0.0,
            "animation time must be finite and nonnegative",
        )?;
        for (p, n) in pose.iter_mut().zip(&self.nodes) {
            *p = n.rest;
        }
        if let Some(name) = name {
            let clip = self
                .clips
                .iter()
                .find(|c| c.name == name)
                .ok_or_else(|| format!("unknown model clip: {name}"))?;
            let looping = self.controls.loops.get(name).copied().unwrap_or(false);
            let time = if looping && clip.duration > 0.0 {
                time.rem_euclid(clip.duration)
            } else {
                time.min(clip.duration)
            };
            for channel in &clip.channels {
                let v = channel.sample(time);
                match channel.property {
                    Property::Translation => pose[channel.node].translation = Vec3::from_slice(&v),
                    Property::Rotation => pose[channel.node].rotation = Quat::from_array(v),
                    Property::Scale => pose[channel.node].scale = Vec3::from_slice(&v),
                    Property::MorphTargetWeights => unreachable!("loader rejects morph channels"),
                }
            }
        }
        Ok(())
    }
    pub(crate) fn local_pose(&self, name: Option<&str>, time: f32) -> Result<Vec<Transform>> {
        let mut pose: Vec<_> = self.nodes.iter().map(|n| n.rest).collect();
        self.local_pose_into(name, time, &mut pose)?;
        Ok(pose)
    }
    pub(crate) fn sample(&self, name: Option<&str>, time: f32) -> Result<Vec<Mat4>> {
        self.matrices(self.local_pose(name, time)?)
    }
    pub(crate) fn sample_blended(
        &self,
        a: Option<&str>,
        a_time: f32,
        b: Option<&str>,
        b_time: f32,
        weight: f32,
    ) -> Result<Vec<Mat4>> {
        ensure(
            weight.is_finite() && (0.0..=1.0).contains(&weight),
            "animation blend weight must be 0..1",
        )?;
        let a = self.local_pose(a, a_time)?;
        let b = self.local_pose(b, b_time)?;
        self.matrices(
            a.into_iter()
                .zip(b)
                .map(|(a, b)| Transform {
                    translation: a.translation.lerp(b.translation, weight),
                    rotation: a.rotation.slerp(b.rotation, weight).normalize(),
                    scale: a.scale.lerp(b.scale, weight),
                })
                .collect(),
        )
    }
    pub(crate) fn matrices(&self, pose: Vec<Transform>) -> Result<Vec<Mat4>> {
        let mut worlds = vec![Mat4::IDENTITY; pose.len()];
        for (i, local) in pose.into_iter().enumerate() {
            ensure(
                local.translation.is_finite()
                    && local.scale.is_finite()
                    && local.scale.min_element() > 1e-6
                    && local.rotation.is_finite()
                    && (local.rotation.length_squared() - 1.0).abs() < 0.001,
                "sampled model pose is nonfinite or singular",
            )?;
            let parent = self.nodes[i].parent.map_or(Mat4::IDENTITY, |p| worlds[p]);
            worlds[i] = parent * local.matrix();
            ensure(
                worlds[i].is_finite() && worlds[i].determinant().abs() > 1e-12,
                "sampled model hierarchy is singular",
            )?;
        }
        self.bindings
            .iter()
            .map(|binding| {
                let matrix = worlds[binding.node] * binding.inverse_bind;
                ensure(
                    matrix.is_finite() && matrix.determinant().abs() > 1e-12,
                    "sampled skin matrix is singular",
                )?;
                Ok(matrix)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
