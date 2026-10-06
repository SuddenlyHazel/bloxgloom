//! Known static builtin geometry needs only numerical allowance. Unclassified
//! fixture/package triangles and real botanical wind keep conservative bounds.
use super::Triangle;
use crate::content::TextureDef;
pub(super) fn stationary(texture: Option<&TextureDef>) -> bool {
    texture.is_some_and(|t| {
        t.key.starts_with("bloxgloom:")
            && !(t.alpha_cutout && (t.foliage.wrap > 0.0 || t.foliage.transmission > 0.0))
    })
}
pub(super) fn padding(triangle: &Triangle, tight: bool) -> f32 {
    if tight && triangle.normal[3] == -1.0 {
        0.0001
    } else {
        0.12
    }
}

#[cfg(test)]
mod tests;
