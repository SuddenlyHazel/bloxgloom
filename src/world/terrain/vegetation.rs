//! Ground-cover selection consumes the same frozen palette as terrain and LOD.
use super::materials::palette;
use super::{Biome, lattice_hash, noise2};
use crate::world::{
    AIR, BLUE_FLOWER, BlockId, FERN, RED_FLOWER, TALL_GRASS, YELLOW_FLOWER, supports_plant,
};

pub(in crate::world) fn ground_plant(
    x: i64,
    z: i64,
    seed: u64,
    biome: Biome,
    soil: BlockId,
) -> BlockId {
    if !supports_plant(soil) || (x.abs() <= 12 && z.abs() <= 12) {
        return AIR;
    }
    let cluster = noise2(x, z, 19, seed ^ 0x36f9_91cb);
    let hash = lattice_hash(seed ^ 0xc483_f4a2, x, 0, z);
    let roll = hash % 1000;
    match biome {
        Biome::Forest if cluster > -0.3 => match roll {
            0..=129 => FERN,
            130..=199 => TALL_GRASS,
            200..=214 => BLUE_FLOWER,
            215..=235 => palette().flowers[((hash >> 16) % 8) as usize],
            _ => AIR,
        },
        Biome::Plains if cluster > -0.15 => match roll {
            0..=199 => TALL_GRASS,
            200..=224 => RED_FLOWER,
            225..=249 => YELLOW_FLOWER,
            250..=269 => BLUE_FLOWER,
            270..=309 => palette().flowers[((hash >> 16) % 8) as usize],
            _ => AIR,
        },
        _ => AIR,
    }
}
