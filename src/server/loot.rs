//! Deterministic, server-owned harvest rules. Item IDs need not be placeable blocks.
use crate::content::Catalog;
use crate::items::{ItemId, SAPLING, SEEDS, STICK};
use crate::world::{self, BlockId};

pub(super) fn harvest(
    block: BlockId,
    position: [i32; 3],
    edit_version: u64,
    seed: u64,
) -> [Option<(ItemId, u16)>; 3] {
    harvest_with_catalog(
        crate::content::catalog(),
        block,
        position,
        edit_version,
        seed,
    )
}

pub(super) fn harvest_with_catalog(
    catalog: &Catalog,
    block: BlockId,
    position: [i32; 3],
    edit_version: u64,
    seed: u64,
) -> [Option<(ItemId, u16)>; 3] {
    let [x, y, z] = position;
    let roll = mix(seed
        ^ (x as i64 as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (y as i64 as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ (z as i64 as u64).wrapping_mul(0x94d0_49bb_1331_11eb)
        ^ edit_version);
    match block {
        world::AIR => [None; 3],
        world::TALL_GRASS => [roll.is_multiple_of(3).then_some((SEEDS, 1)), None, None],
        world::LEAVES => [
            ((roll & 15) == 0).then_some((ItemId::new(world::LEAVES.get()), 1)),
            (roll >> 8).is_multiple_of(5).then_some((STICK, 1)),
            (roll >> 16).is_multiple_of(20).then_some((SAPLING, 1)),
        ],
        _ => [
            catalog.primary_block_item(block).map(|item| (item, 1)),
            None,
            None,
        ],
    }
}

#[inline]
fn mix(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flower_harvests_itself_and_grass_and_leaves_have_distinct_loot() {
        assert_eq!(
            harvest(world::RED_FLOWER, [1, 2, 3], 1, 7),
            [Some((ItemId::new(world::RED_FLOWER.get()), 1)), None, None]
        );
        let grass: Vec<_> = (0..240)
            .flat_map(|x| harvest(world::TALL_GRASS, [x, 20, 0], 1, 7))
            .flatten()
            .collect();
        assert!(grass.len() > 40 && grass.len() < 120);
        assert!(grass.iter().all(|drop| *drop == (SEEDS, 1)));
        let leaves: Vec<_> = (0..400)
            .flat_map(|x| harvest(world::LEAVES, [x, 20, 0], 1, 7))
            .flatten()
            .collect();
        assert!(leaves.iter().any(|drop| drop.0 == SAPLING));
        assert!(leaves.iter().any(|drop| drop.0 == STICK));
        let first: Vec<_> = (0..128)
            .map(|x| harvest(world::LEAVES, [x, 20, 0], 7, 3))
            .collect();
        let repeated: Vec<_> = (0..128)
            .map(|x| harvest(world::LEAVES, [x, 20, 0], 7, 3))
            .collect();
        let next_revision: Vec<_> = (0..128)
            .map(|x| harvest(world::LEAVES, [x, 20, 0], 8, 3))
            .collect();
        assert_eq!(first, repeated);
        assert_ne!(first, next_revision);
    }
}
