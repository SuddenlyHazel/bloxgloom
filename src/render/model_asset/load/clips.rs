//! Decode every baked named clip, preserving linear, stepped and cubic tracks.
use super::*;
use crate::render::model_asset::animation::{Channel, Clip};
use gltf::animation::{Interpolation, Property, util::ReadOutputs};

pub(super) fn load(g: &gltf::Gltf, blob: &[u8], order: &[Option<usize>]) -> Result<Vec<Clip>> {
    ensure(g.animations().len() <= 64, "more than 64 model clips")?;
    let mut clips = Vec::new();
    let mut names = HashSet::new();
    let mut total = 0;
    for animation in g.animations() {
        let name = named(animation.name(), "clip", animation.index())?;
        ensure(names.insert(name.clone()), "duplicate model clip name")?;
        let mut seen = HashSet::new();
        let mut channels = Vec::new();
        let mut duration = 0.0_f32;
        for channel in animation.channels() {
            let target = channel.target();
            let node =
                order[target.node().index()].ok_or("animation target outside active scene")?;
            let property = target.property();
            ensure(
                property != Property::MorphTargetWeights
                    && channels.len() < 768
                    && seen.insert((node, property as u8)),
                "unsupported, duplicate or oversized animation channel",
            )?;
            ensure(
                !matches!(
                    target.node().transform(),
                    gltf::scene::Transform::Matrix { .. }
                ),
                "animated nodes must export TRS, not matrices",
            )?;
            let reader = channel.reader(|_| Some(blob));
            let times: Vec<_> = reader
                .read_inputs()
                .ok_or("animation has no time keys")?
                .collect();
            let interpolation = channel.sampler().interpolation();
            let values: Vec<[f32; 4]> =
                match reader.read_outputs().ok_or("animation has no values")? {
                    ReadOutputs::Translations(v) | ReadOutputs::Scales(v) => {
                        v.map(|v| [v[0], v[1], v[2], 0.0]).collect()
                    }
                    ReadOutputs::Rotations(v) => v.into_f32().collect(),
                    ReadOutputs::MorphTargetWeights(_) => {
                        return Err("morph animations unsupported".into());
                    }
                };
            ensure(
                !times.is_empty()
                    && times.len() <= 4096
                    && times
                        .iter()
                        .all(|v| v.is_finite() && (0.0..=120.0).contains(v))
                    && times.windows(2).all(|v| v[0] < v[1]),
                "invalid animation key times",
            )?;
            let stride = if interpolation == Interpolation::CubicSpline {
                3
            } else {
                1
            };
            ensure(
                values.len() == times.len() * stride
                    && values
                        .iter()
                        .flatten()
                        .all(|v| v.is_finite() && v.abs() <= 10000.0),
                "invalid animation key values",
            )?;
            for value in values
                .iter()
                .skip(if stride == 3 { 1 } else { 0 })
                .step_by(stride)
            {
                if property == Property::Rotation {
                    ensure(
                        (Quat::from_array(*value).length_squared() - 1.0).abs() < 0.001,
                        "animation rotation must be unit length",
                    )?;
                }
                if property == Property::Scale {
                    ensure(
                        value[..3].iter().all(|v| *v > 0.0 && *v <= 256.0),
                        "animation scale must be positive",
                    )?;
                }
            }
            total += values.len();
            ensure(total <= 262144, "model animation key budget exceeded")?;
            duration = duration.max(*times.last().unwrap());
            channels.push(Channel {
                node,
                property,
                interpolation,
                times,
                values,
            });
        }
        ensure(!channels.is_empty(), "model clip is empty")?;
        clips.push(Clip {
            name,
            duration,
            channels,
        });
    }
    Ok(clips)
}
