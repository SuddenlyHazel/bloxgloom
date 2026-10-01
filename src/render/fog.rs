//! Shared distance fog for terrain and actors. Storm scattering is independent
//! of surface lighting, so distant shadows cannot remain visible through fog.
pub(super) const SHADER: &str = include_str!("fog.wgsl");

pub(super) fn shader(source: &str) -> String {
    format!("{SHADER}\n{source}")
}

pub(super) fn density(strength: f32, exposure: f32) -> f32 {
    0.065 * strength.clamp(0.0, 1.0).sqrt() * exposure.clamp(0.0, 1.0)
}

#[cfg(test)]
#[path = "fog/tests.rs"]
mod tests;
