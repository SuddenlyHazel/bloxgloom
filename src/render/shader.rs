use super::SUN_DIRECTION;
use super::material::GLOWSTONE_LAYER;

pub(super) fn with_world_sun(source: &str) -> String {
    format!(
        "const WORLD_SUN_DIRECTION: vec3<f32> = vec3<f32>({}, {}, {});\n{source}",
        SUN_DIRECTION.x, SUN_DIRECTION.y, SUN_DIRECTION.z
    )
}

pub(super) fn with_voxel_constants(source: &str) -> String {
    format!(
        "const GLOWSTONE_LAYER: i32 = {GLOWSTONE_LAYER};\n{}",
        with_world_sun(source)
    )
}
