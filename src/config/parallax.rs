//! Local material relief settings and their bounded shader representation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Parallax {
    pub enabled: bool,
    pub depth: f32,
    /// Fade begins halfway to this distance and finishes at it.
    pub distance: f32,
    /// Maximum samples at an oblique angle; head-on surfaces use fewer.
    pub steps: u32,
}

impl Default for Parallax {
    fn default() -> Self {
        Self {
            enabled: true,
            // Imported alpha encodes depth relative to labPBR's quarter block.
            // Half of that relief is a conservative block-edge-safe default.
            depth: 0.125,
            distance: 32.0,
            steps: 32,
        }
    }
}

impl Parallax {
    pub(super) fn sanitized(self) -> Self {
        Self {
            enabled: self.enabled,
            depth: super::clamp_finite(self.depth, 0.0, 0.15, 0.125),
            distance: super::clamp_finite(self.distance, 8.0, 96.0, 32.0),
            steps: self.steps.clamp(12, 64),
        }
    }

    pub(crate) fn uniform(self) -> [f32; 4] {
        let settings = self.sanitized();
        [
            if settings.enabled {
                settings.depth
            } else {
                0.0
            },
            settings.distance,
            settings.steps as f32,
            0.0,
        ]
    }

    pub(super) fn parse(&mut self, key: &str, value: &str) -> bool {
        match key {
            "parallax_enabled" => {
                if let Ok(enabled) = value.parse() {
                    self.enabled = enabled;
                }
            }
            "parallax_depth" => self.depth = super::parse_clamped_float(value, 0.0, 0.15, 0.125),
            "parallax_distance" => {
                self.distance = super::parse_clamped_float(value, 8.0, 96.0, 32.0)
            }
            "parallax_steps" => {
                if let Ok(steps) = value.parse::<u32>() {
                    self.steps = steps.clamp(12, 64);
                }
            }
            _ => return false,
        }
        true
    }

    pub(super) fn serialize(self) -> String {
        format!(
            "parallax_enabled={}\nparallax_depth={}\nparallax_distance={}\nparallax_steps={}\n",
            self.enabled, self.depth, self.distance, self.steps
        )
    }
}
