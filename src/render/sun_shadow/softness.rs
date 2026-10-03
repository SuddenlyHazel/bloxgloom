//! Bounded art direction for the sun's apparent angular size. Startup-only
//! environment overrides leave the saved shadow-quality configuration unchanged.
#[derive(Clone, Copy, Debug)]
pub(super) struct Softness {
    angular_radius: f32,
    max_radius_texels: f32,
}

impl Default for Softness {
    fn default() -> Self {
        Self::from_values(None, None)
    }
}

impl Softness {
    pub(super) fn configured() -> Self {
        Self::from_values(
            std::env::var("BLOXGLOOM_SUN_SOFTNESS").ok().as_deref(),
            std::env::var("BLOXGLOOM_SUN_PENUMBRA_TEXELS")
                .ok()
                .as_deref(),
        )
    }

    pub(super) fn from_values(softness: Option<&str>, radius: Option<&str>) -> Self {
        let bounded = |value: Option<&str>, default: f32, min: f32, max: f32| {
            value
                .and_then(|value| value.parse::<f32>().ok())
                .filter(|value| value.is_finite())
                .unwrap_or(default)
                .clamp(min, max)
        };
        Self {
            // tan(half-angle), about 1.15 degrees at the restrained default.
            angular_radius: 0.02 * bounded(softness, 1.0, 0.0, 4.0),
            max_radius_texels: bounded(radius, 6.0, 2.0, 12.0),
        }
    }

    pub(super) fn data(self) -> [f32; 4] {
        // The depth search is bounded to 24 world units; distant blockers cannot
        // request unbounded work or enlarge the filter beyond the texel cap.
        [self.angular_radius, self.max_radius_texels, 24.0, 0.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soft_sun_configuration_is_finite_bounded_and_can_disable_search() {
        for bad in [None, Some("bad"), Some("NaN"), Some("inf")] {
            assert_eq!(
                Softness::from_values(bad, bad).data(),
                [0.02, 6.0, 24.0, 0.0]
            );
        }
        assert_eq!(
            Softness::from_values(Some("-1"), Some("-1")).data(),
            [0.0, 2.0, 24.0, 0.0]
        );
        assert_eq!(
            Softness::from_values(Some("10"), Some("100")).data(),
            [0.08, 12.0, 24.0, 0.0]
        );
        assert_eq!(
            Softness::from_values(Some("0.5"), Some("4")).data(),
            [0.01, 4.0, 24.0, 0.0]
        );
    }
}
