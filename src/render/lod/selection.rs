use crate::lod::TileKey;
use glam::Vec3;
/// Nested rings, coarsest and nearest first. Refinements always request complete
/// sibling families so an uploaded parent can actually retire near the camera.
pub(crate) fn desired_tiles(
    position: Vec3,
    horizon: u16,
    quality: u8,
    max_level: u8,
) -> Vec<TileKey> {
    if horizon == 0 {
        return vec![];
    }
    let max_level = max_level.clamp(3, 4);
    // Contributor generation advertises the smaller independent work horizon.
    let horizon = horizon.min(if max_level == 3 { 512 } else { 1024 });
    let mut out = Vec::new();
    for level in (2 - quality.min(2)..=max_level).rev() {
        let width = 32i32 << level;
        let Some(center) =
            TileKey::containing(level, position.x.floor() as i32, position.z.floor() as i32)
        else {
            continue;
        };
        let (minx, minz, count) = if level == max_level {
            let radius = (i32::from(horizon) + width - 1) / width;
            (center.x - radius, center.z - radius, 2 * radius + 1)
        } else {
            // Keep complete sibling families, including at negative coordinates.
            // Extend the finest ring beyond the normal near-chunk band so trees
            // and cliffs do not immediately jump to broad coarse cells.
            // The 1,024-block skyline uses the spare residency budget itself.
            let count = if horizon <= 512 && quality == 1 && level == 1 {
                6
            } else {
                4
            };
            let offset = count / 2 - 1;
            (
                (center.x - offset).div_euclid(2) * 2,
                (center.z - offset).div_euclid(2) * 2,
                count,
            )
        };
        for z in minz..minz + count {
            for x in minx..minx + count {
                let key = TileKey { level, x, z };
                if key.bounds().is_some() {
                    out.push(key);
                }
            }
        }
    }
    out.sort_by_key(|k| {
        let width = 32i64 << k.level;
        let dx = (i64::from(k.x) * width + width / 2 - position.x.floor() as i64).abs();
        let dz = (i64::from(k.z) * width + width / 2 - position.z.floor() as i64).abs();
        (std::cmp::Reverse(k.level), dx + dz, k.x, k.z)
    });
    out
}
