//! Stable preview placement and audible-distance gating; world anchors remain intact.
use super::{super::dsp::Rng, VOICES};
use crate::audio::rain_tuning::Placement;
pub(super) fn preview(seed: u32, profile: Placement, eye: [f32; 3]) -> [Option<[f32; 3]>; VOICES] {
    let mut rng = Rng::new(seed, 0x67af_e185);
    std::array::from_fn(|_| {
        let angle = std::f32::consts::PI * profile.stereo_width * rng.between(-1.0, 1.0);
        let distance = rng.between(profile.min_distance_m, profile.max_distance_m);
        Some([
            eye[0] + distance * angle.cos(),
            eye[1],
            eye[2] + distance * angle.sin(),
        ])
    })
}
pub(super) fn audible(
    sources: &[Option<[f32; 3]>; VOICES],
    eye: [f32; 3],
    profile: Placement,
) -> [bool; VOICES] {
    sources.map(|s| {
        s.is_some_and(|s| {
            let d = (0..3).map(|i| (s[i] - eye[i]).powi(2)).sum::<f32>().sqrt();
            (profile.min_distance_m..=profile.max_distance_m).contains(&d)
        })
    })
}
