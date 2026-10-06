//! Explicit resolution/preset controls keep MacBook and historical runs comparable.
use crate::config::{Config, quality::QualityPreset};

pub(super) struct Options {
    pub native: (u32, u32),
    pub dimensions: (u32, u32),
    pub config: Option<Config>,
}
impl Options {
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        let mut config = std::env::var_os("BLOXGLOOM_PERF_CONFIG").map(Config::load);
        if let Ok(value) = std::env::var("BLOXGLOOM_QUALITY_PRESET") {
            let preset = QualityPreset::parse(&value)
                .filter(|preset| *preset != QualityPreset::Custom)
                .ok_or("BLOXGLOOM_QUALITY_PRESET must be performance, balanced or quality")?;
            let mut settings = config.unwrap_or_default();
            settings.apply_quality_preset(preset);
            config = Some(settings);
        }
        let dimension = |key: &str, fallback| -> Result<u32, Box<dyn std::error::Error>> {
            let value = match std::env::var(key) {
                Ok(value) => value.parse::<u32>()?,
                Err(std::env::VarError::NotPresent) => fallback,
                Err(error) => return Err(error.into()),
            };
            if !(1..=8192).contains(&value) {
                return Err(format!("{key} must be 1..8192").into());
            }
            Ok(value)
        };
        let native = (
            dimension("BLOXGLOOM_PERF_WIDTH", super::super::PERF_WIDTH)?,
            dimension("BLOXGLOOM_PERF_HEIGHT", super::super::PERF_HEIGHT)?,
        );
        let scale = config.as_ref().map_or(1.0, |c| c.render_scale);
        let dimensions = (
            (native.0 as f32 * scale).round().max(1.0) as u32,
            (native.1 as f32 * scale).round().max(1.0) as u32,
        );
        eprintln!(
            "quality: {}, requested {}x{}, world {}x{}; radius/horizon remain explicit CLI arguments",
            config
                .as_ref()
                .map_or("historical", |c| c.effective_quality_preset().as_str()),
            native.0,
            native.1,
            dimensions.0,
            dimensions.1
        );
        Ok(Self {
            native,
            dimensions,
            config,
        })
    }
}
