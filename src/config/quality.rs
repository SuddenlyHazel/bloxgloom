//! Saved renderer budgets. Presets leave color grading, controls, and identity alone.
use super::{Config, SunShadowQuality};

/// Independent world-resolution floor; menus and HUD retain native pixels.
pub(crate) const MIN_RENDER_SCALE: f32 = 0.35;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QualityPreset {
    #[default]
    Custom,
    Performance,
    Balanced,
    Quality,
}

impl QualityPreset {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "custom" => Some(Self::Custom),
            "performance" => Some(Self::Performance),
            "balanced" => Some(Self::Balanced),
            "quality" => Some(Self::Quality),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Custom => "custom",
            Self::Performance => "performance",
            Self::Balanced => "balanced",
            Self::Quality => "quality",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Custom => "Custom",
            Self::Performance => "MacBook",
            Self::Balanced => "Balanced",
            Self::Quality => "Quality",
        }
    }

    /// Custom describes manual settings; cycling chooses an authored preset.
    pub const fn cycle(self, increase: bool) -> Self {
        match (self, increase) {
            (Self::Custom | Self::Quality, true) | (Self::Balanced, false) => Self::Performance,
            (Self::Performance, true) | (Self::Quality, false) => Self::Balanced,
            (Self::Balanced, true) | (Self::Custom | Self::Performance, false) => Self::Quality,
        }
    }
}

impl Config {
    pub fn apply_quality_preset(&mut self, preset: QualityPreset) {
        self.quality_preset = preset;
        let Some(budget) = preset.budget() else {
            return;
        };
        self.render_scale = budget.render_scale;
        self.view_distance = budget.view_distance;
        self.lod_horizon = budget.lod_horizon;
        self.lod_quality = budget.lod_quality;
        self.sun_shadow_quality = budget.sun_shadow_quality;
        self.parallax.enabled = budget.parallax_enabled;
        self.parallax.steps = budget.parallax_steps;
        self.parallax.distance = budget.parallax_distance;
        self.reflections_enabled = budget.reflections_enabled;
        self.local_shadows = budget.local_shadows;
        self.bloom_enabled = budget.bloom_enabled;
        // Bounced voxel lighting is separate from opt-in path-traced GI. Neither
        // needs to be enabled to exercise the raster quality presets.
        self.bounced_gi = false;
    }

    /// Never display an authored name for an edited or contradictory bundle.
    pub fn effective_quality_preset(&self) -> QualityPreset {
        if self.quality_preset.budget() == Some(Budget::of(self)) && !self.bounced_gi {
            self.quality_preset
        } else {
            QualityPreset::Custom
        }
    }
}

// Comparing these small values avoids cloning the entire config/binding map
// every frame just to show the preset name in the UI.
#[derive(Clone, Copy, PartialEq)]
struct Budget {
    render_scale: f32,
    view_distance: u8,
    lod_horizon: u16,
    lod_quality: u8,
    sun_shadow_quality: SunShadowQuality,
    parallax_enabled: bool,
    parallax_steps: u32,
    parallax_distance: f32,
    reflections_enabled: bool,
    local_shadows: crate::render::local_shadow::Settings,
    bloom_enabled: bool,
}

impl Budget {
    fn of(config: &Config) -> Self {
        Self {
            render_scale: config.render_scale,
            view_distance: config.view_distance,
            lod_horizon: config.lod_horizon,
            lod_quality: config.lod_quality,
            sun_shadow_quality: config.sun_shadow_quality,
            parallax_enabled: config.parallax.enabled,
            parallax_steps: config.parallax.steps,
            parallax_distance: config.parallax.distance,
            reflections_enabled: config.reflections_enabled,
            local_shadows: config.local_shadows,
            bloom_enabled: config.bloom_enabled,
        }
    }
}

impl QualityPreset {
    fn budget(self) -> Option<Budget> {
        let (scale, view, detail, sun, parallax, steps, distance, reflections, local, resolution) =
            match self {
                Self::Custom => return None,
                Self::Performance => (
                    MIN_RENDER_SCALE,
                    3,
                    0,
                    SunShadowQuality::Low,
                    false,
                    12,
                    16.0,
                    false,
                    0,
                    128,
                ),
                Self::Balanced => (
                    0.5,
                    4,
                    1,
                    SunShadowQuality::Medium,
                    true,
                    16,
                    20.0,
                    true,
                    1,
                    256,
                ),
                Self::Quality => (
                    1.0,
                    6,
                    2,
                    SunShadowQuality::High,
                    true,
                    32,
                    32.0,
                    true,
                    2,
                    512,
                ),
            };
        Some(Budget {
            render_scale: scale,
            view_distance: view,
            lod_horizon: 512,
            lod_quality: detail,
            sun_shadow_quality: sun,
            parallax_enabled: parallax,
            parallax_steps: steps,
            parallax_distance: distance,
            reflections_enabled: reflections,
            local_shadows: crate::render::local_shadow::Settings {
                count: local,
                resolution,
                range: 16.0,
                updates: local.max(1),
            },
            bloom_enabled: self != Self::Performance,
        })
    }
}

#[cfg(test)]
mod tests;
