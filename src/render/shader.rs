use super::SUN_DIRECTION;

pub(super) fn with_world_sun(source: &str) -> String {
    format!(
        "const WORLD_SUN_DIRECTION: vec3<f32> = vec3<f32>({}, {}, {});\n{source}",
        SUN_DIRECTION.x, SUN_DIRECTION.y, SUN_DIRECTION.z
    )
}
