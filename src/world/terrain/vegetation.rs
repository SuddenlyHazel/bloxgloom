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
    let cluster = noise2(x, z, 27, seed ^ 0x36f9_91cb);
    let meadow = noise2(x, z, 96, seed ^ 0x198a_b57c);
    let species = noise2(x, z, 64, seed ^ 0x647e_c13a);
    let flower = palette().flowers[(((species + 1.0) * 4.0) as usize).min(7)];
    let hash = lattice_hash(seed ^ 0xc483_f4a2, x, 0, z);
    let roll = hash % 1000;
    match biome {
        Biome::Forest if cluster > -0.3 => match roll {
            0..=129 => FERN,
            130..=199 => TALL_GRASS,
            200..=214 => BLUE_FLOWER,
            215..=255 => flower,
            _ => AIR,
        },
        Biome::Plains if cluster > -0.35 => match roll {
            0..=199 => TALL_GRASS,
            200..=224 => RED_FLOWER,
            225..=249 => YELLOW_FLOWER,
            250..=269 => BLUE_FLOWER,
            270..=449 if meadow > 0.1 => flower,
            270..=359 => TALL_GRASS,
            _ => AIR,
        },
        _ => AIR,
    }
}
