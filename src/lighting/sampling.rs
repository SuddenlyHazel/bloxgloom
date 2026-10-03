//! Spatial light for thin crossed geometry. Samples the same voxel fields as
//! terrain corners; no additional ambient-occlusion multiplier is applied.
use super::{CHUNK_SIZE, LightField, SIDE, index};

impl LightField {
    /// Trilinear, cell-centered light at a position relative to the target chunk.
    /// The existing halo makes samples continuous at chunk edges. This is a
    /// meshing-time operation, with no additional vertex bytes or frame work.
    #[cfg(test)]
    pub(crate) fn spatial(&self, local: [f32; 3]) -> [f32; 8] {
        self.spatial_with_visibility(local).0
    }
    pub(crate) fn spatial_with_visibility(&self, local: [f32; 3]) -> ([f32; 8], f32) {
        let position = local.map(|p| (p + CHUNK_SIZE as f32 - 0.5).clamp(0.0, (SIDE - 2) as f32));
        let base = position.map(|p| p.floor() as usize);
        let fraction = std::array::from_fn::<_, 3, _>(|axis| position[axis] - base[axis] as f32);
        let mut result = [0.0; 8];
        let mut maxima = [0.0f32; 8];
        for y in 0..2 {
            for z in 0..2 {
                for x in 0..2 {
                    let offsets = [x, y, z];
                    let weight = (0..3)
                        .map(|axis| {
                            if offsets[axis] == 0 {
                                1.0 - fraction[axis]
                            } else {
                                fraction[axis]
                            }
                        })
                        .product::<f32>();
                    let at = index(base[0] + x, base[1] + y, base[2] + z);
                    if weight > 0.0 {
                        maxima[0] = maxima[0].max(f32::from(self.sky[at]) / 15.0);
                    }
                    result[0] += weight * f32::from(self.sky[at]) / 15.0;
                    result[1] += weight * f32::from(self.glow[at]) / 15.0;
                    for channel in 0..3 {
                        if let Some(field) = &self.bounce {
                            result[2 + channel] += weight * f32::from(field[at][channel]) / 255.0;
                            if weight > 0.0 {
                                maxima[2 + channel] =
                                    maxima[2 + channel].max(f32::from(field[at][channel]) / 255.0);
                            }
                        }
                        if let Some(field) = &self.glow_bounce {
                            result[5 + channel] += weight * f32::from(field[at][channel]) / 255.0;
                            if weight > 0.0 {
                                maxima[5 + channel] =
                                    maxima[5 + channel].max(f32::from(field[at][channel]) / 255.0);
                            }
                        }
                    }
                }
            }
        }
        let visibility = result
            .into_iter()
            .zip(maxima)
            .filter(|(_, max)| *max > 0.0)
            .map(|(sample, max)| sample / max)
            .fold(1.0f32, f32::min);
        (result, visibility)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lighting::VOLUME;

    #[test]
    fn spatial_light_preserves_constant_fields_and_separates_day_and_emission() {
        let field = LightField {
            sky: vec![9; VOLUME],
            glow: vec![3; VOLUME],
            local: None,
            bounce: Some(vec![[51, 102, 153]; VOLUME]),
            glow_bounce: Some(vec![[0, 51, 102]; VOLUME]),
        };
        for point in [[0.02, 0.02, 0.08], [31.92, 31.98, 31.08], [5.5, 6.5, 7.5]] {
            let sample = field.spatial(point);
            for (actual, expected) in sample
                .into_iter()
                .zip([0.6, 0.2, 0.2, 0.4, 0.6, 0.0, 0.2, 0.4])
            {
                assert!((actual - expected).abs() < 0.00001);
            }
        }
    }

    #[test]
    fn spatial_light_resolves_root_tip_and_horizontal_variation_once() {
        let mut field = LightField {
            sky: vec![0; VOLUME],
            glow: vec![0; VOLUME],
            local: None,
            bounce: None,
            glow_bounce: None,
        };
        // A single lit voxel: exact cell center retains it; halfway to an
        // occluded neighbor averages once, without an extra AO factor.
        field.sky[index(CHUNK_SIZE, CHUNK_SIZE, CHUNK_SIZE)] = 15;
        assert_eq!(field.spatial([0.5, 0.5, 0.5])[0], 1.0);
        assert_eq!(field.spatial([0.5, 0.0, 0.5])[0], 0.5);
        assert_eq!(field.spatial_with_visibility([0.5, 0.0, 0.5]).1, 0.5);
        assert_eq!(field.spatial_with_visibility([0.5, 0.5, 0.5]).1, 1.0);
        assert_eq!(field.spatial([0.0, 0.5, 0.5])[0], 0.5);
        assert_eq!(field.spatial([0.5, 1.5, 0.5])[0], 0.0);
    }
    #[test]
    fn corner_visibility_preserves_uniform_transport_and_protects_averaged_occlusion() {
        let mut field = LightField {
            sky: vec![6; crate::lighting::VOLUME],
            glow: vec![0; crate::lighting::VOLUME],
            local: None,
            bounce: None,
            glow_bounce: None,
        };
        let (samples, visibility) = field.corner_with_visibility([1, 0, 2], 1, 0, [1, 1]);
        assert_eq!(samples[0], 0.4);
        assert_eq!(
            visibility, 1.0,
            "uniform cave/portal transport is not additional local AO"
        );
        field.sky[index(CHUNK_SIZE, CHUNK_SIZE + 1, CHUNK_SIZE)] = 0;
        field.sky[index(CHUNK_SIZE + 1, CHUNK_SIZE + 1, CHUNK_SIZE)] = 0;
        let (samples, visibility) = field.corner_with_visibility([1, 0, 2], 1, 0, [1, 1]);
        assert_eq!(
            samples,
            field.corner([1, 0, 2], 1, 0, [1, 1]),
            "original irradiance unchanged"
        );
        assert_eq!(samples[0], 0.2);
        assert_eq!(visibility, 0.5);
        field.sky.fill(0);
        assert_eq!(
            field.corner_with_visibility([1, 0, 2], 1, 0, [1, 1]).1,
            1.0,
            "sealed black adds no energy or artificial visibility floor"
        );
    }
}
