//! Checked-in gbuffers_skytextured.glsl default Sun/Moon radiance.
//! Minecraft's vertex tint/blend state is external to the shader; our unchanged
//! RGB art supplies opaque white vertices and enters the existing linear HDR sky.
pub(super) const SHADER: &str = include_str!("reference_celestial.wgsl");

#[cfg(test)]
#[path = "reference_celestial/tests.rs"]
mod tests;
