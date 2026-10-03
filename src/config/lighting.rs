//! Bounded scene lighting scales, independent of camera exposure.
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Lighting {
    pub sun_intensity: f32,
    pub ambient_intensity: f32,
    pub environment_intensity: f32,
    /// Multiplier for the default 65% directional local-light mixture.
    pub local_directionality: f32,
}

impl Default for Lighting {
    fn default() -> Self {
        Self {
            sun_intensity: 1.0,
            ambient_intensity: 1.0,
            environment_intensity: 1.0,
            local_directionality: 1.0,
        }
    }
}

impl Lighting {
    /// Authored assets reject invalid scales instead of silently repairing them.
    pub(crate) fn validate(self) -> Result<(), String> {
        for (name, value) in [
            ("sun_intensity", self.sun_intensity),
            ("ambient_intensity", self.ambient_intensity),
            ("environment_intensity", self.environment_intensity),
            ("local_directionality", self.local_directionality),
        ] {
            if !value.is_finite() || !(0.0..=4.0).contains(&value) {
                return Err(format!(
                    "environment_lighting.{name} must be finite and in 0..4"
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn sanitized(self) -> Self {
        Self {
            sun_intensity: super::clamp_finite(self.sun_intensity, 0.0, 4.0, 1.0),
            ambient_intensity: super::clamp_finite(self.ambient_intensity, 0.0, 4.0, 1.0),
            environment_intensity: super::clamp_finite(self.environment_intensity, 0.0, 4.0, 1.0),
            local_directionality: super::clamp_finite(self.local_directionality, 0.0, 4.0, 1.0),
        }
    }

    pub(super) fn parse(&mut self, key: &str, value: &str) -> bool {
        let field = match key {
            "lighting_sun_intensity" => &mut self.sun_intensity,
            "lighting_ambient_intensity" => &mut self.ambient_intensity,
            "lighting_environment_intensity" => &mut self.environment_intensity,
            "lighting_local_directionality" => &mut self.local_directionality,
            _ => return false,
        };
        *field = super::parse_clamped_float(value, 0.0, 4.0, 1.0);
        true
    }

    pub(super) fn serialize(self) -> String {
        format!(
            "lighting_sun_intensity={}\nlighting_ambient_intensity={}\nlighting_environment_intensity={}\nlighting_local_directionality={}\n",
            self.sun_intensity,
            self.ambient_intensity,
            self.environment_intensity,
            self.local_directionality
        )
    }
}
