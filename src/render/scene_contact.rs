//! Bounded dynamic scene contacts. The existing resident-floor validator supplies
//! footprints; only sky diffuse is reduced, never the already baked voxel field.
//! This is analytic actor grounding, not a screen-space/full-scene AO estimator.
use super::contact_shadow::Patch;

pub(super) const MAX_PATCHES: usize = 32;
pub(super) const UNIFORM_BYTES: u64 = (16 + MAX_PATCHES * 48) as u64;

pub(super) fn configured_strength() -> f32 {
    strength(std::env::var("BLOXGLOOM_CONTACT_OCCLUSION").ok().as_deref())
}

fn strength(value: Option<&str>) -> f32 {
    value
        .and_then(|v| v.parse::<f32>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(1.0)
        .clamp(0.0, 1.0)
}

pub(super) fn data(patches: &[Patch], strength: f32) -> [f32; 4 + MAX_PATCHES * 12] {
    let mut data = [0.0; 4 + MAX_PATCHES * 12];
    data[1] = strength;
    // The caller orders actors by distance then ID. Retain whole footprints in
    // that deterministic order; truncating a footprint would expose cell seams.
    // Budget membership can change when actors exchange distance order.
    let mut count = 0;
    for footprint in patches.chunk_by(|a, b| a.center == b.center) {
        if count + footprint.len() > MAX_PATCHES {
            continue;
        }
        for patch in footprint {
            let out = &mut data[4 + count * 12..4 + (count + 1) * 12];
            out[..4].copy_from_slice(&patch.bounds);
            out[4..8].copy_from_slice(&patch.center);
            out[8..12].copy_from_slice(&patch.light);
            count += 1;
        }
    }
    data[0] = count as f32;
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strength_rejects_nonfinite_and_bounds_configuration() {
        for value in [None, Some("NaN"), Some("inf"), Some("bad")] {
            assert_eq!(strength(value), 1.0);
        }
        assert_eq!(strength(Some("-1")), 0.0);
        assert_eq!(strength(Some("2")), 1.0);
        assert_eq!(strength(Some("0.4")), 0.4);
        assert_eq!(strength(Some("0")), 0.0);
    }

    #[test]
    fn upload_is_bounded_and_clears_stale_contacts() {
        let patch = Patch {
            bounds: [1.0, 2.0, 3.0, 4.0],
            center: [5.0, 6.0, 7.0, 0.6],
            light: [0.26, 1.0, 0.0, 0.0],
        };
        let patches: Vec<_> = (0..MAX_PATCHES + 1)
            .map(|i| Patch {
                center: [i as f32, 6.0, 7.0, 0.6],
                ..patch
            })
            .collect();
        let packed = data(&patches, 0.5);
        assert_eq!(packed.len() * 4, UNIFORM_BYTES as usize);
        assert_eq!(packed[..2], [MAX_PATCHES as f32, 0.5]);
        assert_eq!(packed[4..8], patch.bounds);
        assert_eq!(data(&[], 0.0), [0.0; 4 + MAX_PATCHES * 12]);
        let mut grouped = patches[..MAX_PATCHES - 1].to_vec();
        grouped.extend([patch; 3]);
        assert_eq!(
            data(&grouped, 1.0)[0],
            (MAX_PATCHES - 1) as f32,
            "the last actor must be omitted whole, never clipped to budget"
        );
    }
}

#[cfg(test)]
#[path = "scene_contact/tests.rs"]
mod gpu_tests;
