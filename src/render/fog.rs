//! Shared distance fog for terrain and actors. Storm scattering is independent
//! of surface lighting, so distant shadows cannot remain visible through fog.
pub(super) const SHADER: &str = include_str!("fog.wgsl");
const REFERENCE_SHADER: &str = include_str!("fog/reference.wgsl");

pub(super) fn shader(source: &str) -> String {
    format!(
        "{}\nconst BG_FOG_BSL_STYLE: bool = {};\nconst BG_FOG_REFERENCE: bool = {};\nconst BG_FOG_NOON_HEIGHT: f32 = {};\n{REFERENCE_SHADER}\n{SHADER}\n{source}",
        super::sky::STYLE_SHADER,
        super::sky::style_enabled(),
        super::bsl_reference::enabled(),
        super::SUN_DIRECTION.normalize().y,
    )
}

pub(super) fn density(strength: f32, exposure: f32) -> f32 {
    0.065 * strength.clamp(0.0, 1.0).sqrt() * exposure.clamp(0.0, 1.0)
}

#[cfg(test)]
#[path = "fog/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "fog/reference_tests.rs"]
mod reference_tests;
