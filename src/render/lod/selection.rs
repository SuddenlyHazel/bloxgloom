use crate::lod::TileKey;
use glam::Vec3;
/// Stable nested rings, coarsest first. Tile membership only changes at tile
/// boundaries; retain uploaded ancestors until replacement coverage is ready.
pub(crate) fn desired_tiles(position: Vec3, horizon: u16, quality: u8) -> Vec<TileKey> {
    if horizon == 0 {
        return vec![];
    }
    let mut out = Vec::new();
    for level in (2 - quality.min(2)..=4).rev() {
        let width = 32i32 << level;
        let cx = (position.x.floor() as i32).div_euclid(width);
        let cz = (position.z.floor() as i32).div_euclid(width);
        let radius = if level == 4 {
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
    out
}
