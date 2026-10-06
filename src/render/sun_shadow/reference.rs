//! Local supplied BSL default shadow comparison. No source assets are bundled.
use super::Settings;
pub(super) fn shader() -> String {
    format!(
        "{}\n{}",
        include_str!("reference/equations.wgsl"),
        include_str!("reference.wgsl")
    )
}
pub(super) fn configure(mut settings: Settings, enabled: bool, maximum: u32) -> Settings {
    if enabled && settings.distance > 0.0 {
        settings.reference = true;
        settings.resolution = 2048.min(maximum);
        settings.distance = if settings.resolution >= 512 {
            256.0
        } else {
            0.0
        };
        settings.filter = 1.0;
    }
    settings
}
pub(super) fn data(settings: Settings) -> [f32; 4] {
    [f32::from(settings.reference), 256.0, 0.9, 0.0]
}
#[cfg(test)]
mod tests;
