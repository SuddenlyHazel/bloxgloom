use super::SUN_DIRECTION;

pub(super) fn with_world_sun(source: &str) -> String {
    source.replace(
        "WORLD_SUN_DIRECTION",
        &format!(
            "vec3<f32>({}, {}, {})",
            SUN_DIRECTION.x, SUN_DIRECTION.y, SUN_DIRECTION.z
        ),
    )
}
