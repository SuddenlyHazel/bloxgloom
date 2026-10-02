use crate::lod::TileKey;
use glam::Vec3;
/// Stable nested rings, coarsest first. Tile membership only changes at tile
/// boundaries; retain uploaded ancestors until replacement coverage is ready.
pub(crate) fn desired_tiles(
    position: Vec3,
    horizon: u16,
    quality: u8,
    max_level: u8,
) -> Vec<TileKey> {
    if horizon == 0 {
        return vec![];
    }
    let horizon = horizon.min(1024);
    let max_level = max_level.clamp(3, 4);
    let mut out = Vec::new();
    for level in (2 - quality.min(2)..=max_level).rev() {
        let width = 32i32 << level;
        let Some(center) =
            TileKey::containing(level, position.x.floor() as i32, position.z.floor() as i32)
        else {
            continue;
        };
        let cx = center.x;
        let cz = center.z;
        let radius = if level == max_level {
            (i32::from(horizon) + width - 1) / width
        } else {
            1
        };
        for z in cz - radius..=cz + radius {
            for x in cx - radius..=cx + radius {
                let k = TileKey { level, x, z };
                if k.bounds().is_some() {
                    out.push(k);
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
