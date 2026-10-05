//! Deterministic tree anchors and species-specific crowns across chunk seams.
use super::materials::palette;
use super::{Biome, generated_block_in_column, lattice_hash, terrain_column};
use crate::world::{BlockId, LEAVES, WOOD, supports_plant};

pub(in crate::world) const TREE_CELL: i64 = 16;
pub(in crate::world) const TREE_RADIUS: i64 = 3;

#[derive(Clone, Copy)]
pub(in crate::world) struct Tree {
    pub(in crate::world) x: i64,
    pub(in crate::world) z: i64,
    pub(in crate::world) ground_y: i64,
    pub(in crate::world) trunk_top: i64,
    pub(in crate::world) log: BlockId,
    pub(in crate::world) leaves: BlockId,
    species: usize,
}

pub(in crate::world) fn tree_anchor(cell_x: i64, cell_z: i64, seed: u64) -> Option<Tree> {
    let hash = lattice_hash(seed ^ 0x906f_89ad, cell_x, 0, cell_z);
    let x = cell_x * TREE_CELL + 2 + ((hash >> 8) % 13) as i64;
    let z = cell_z * TREE_CELL + 2 + ((hash >> 16) % 13) as i64;
    if x.abs() <= 14 && z.abs() <= 14 {
        return None;
    }
    let column = terrain_column(x, z, seed);
    if column.water_level.is_some() {
        return None;
    }
    let frequency = match column.biome {
        Biome::Forest => 380,
        Biome::Plains => 45,
        Biome::Tundra => 55,
        Biome::Highland => 30,
        Biome::Desert => 0,
    };
    if hash % 1000 >= frequency
        || !supports_plant(generated_block_in_column(x, column.height, z, column, seed))
    {
        return None;
    }
    let species = match column.biome {
        Biome::Tundra | Biome::Highland => 1,
        Biome::Plains => [0, 2, 4, 7][((hash >> 40) % 4) as usize],
        _ => ((hash >> 40) % 10) as usize,
    };
    let (log, leaves) = if species == 9 {
        (WOOD, LEAVES)
    } else {
        palette().trees[species]
    };
    let base_height = match species {
        1 => 9,
        3 => 11,
        5 => 8,
        _ => 5,
    };
    Some(Tree {
        x,
        z,
        ground_y: column.height,
        trunk_top: column.height + base_height + ((hash >> 28) % 3) as i64,
        log,
        leaves,
        species,
    })
}

pub(in crate::world) fn tree_piece(tree: Tree, x: i64, y: i64, z: i64) -> Option<BlockId> {
    let dx = (x - tree.x).abs();
    let dz = (z - tree.z).abs();
    if dx == 0 && dz == 0 && y > tree.ground_y && y <= tree.trunk_top {
        return Some(tree.log);
    }
    let layer = y - tree.trunk_top;
    let radius = if tree.species == 1 {
        // Layered spruce crown leaves its lower trunk open.
        match layer {
            -5 | -3 | -1 => 2,
            -4 | -2 | 0 => 1,
            1 => 1,
            2 => 0,
            _ => return None,
        }
    } else if tree.species == 4 {
        match layer {
            -1 | 0 => 3,
            1 => 2,
            _ => return None,
        }
    } else {
        match layer.abs() {
            0 => 3,
            1 => 2,
            2 => 1,
            _ => return None,
        }
    };
    (y > tree.ground_y && dx <= radius && dz <= radius && dx + dz <= radius + 1)
        .then_some(tree.leaves)
}

#[cfg(test)]
pub(in crate::world) fn tree_block_at(x: i64, y: i64, z: i64, seed: u64) -> Option<BlockId> {
    let mut leaf = None;
    for cz in (z - TREE_RADIUS).div_euclid(TREE_CELL)..=(z + TREE_RADIUS).div_euclid(TREE_CELL) {
        for cx in (x - TREE_RADIUS).div_euclid(TREE_CELL)..=(x + TREE_RADIUS).div_euclid(TREE_CELL)
        {
            if let Some(tree) = tree_anchor(cx, cz, seed)
                && let Some(piece) = tree_piece(tree, x, y, z)
            {
                if piece == tree.log {
                    return Some(piece);
                }
                leaf.get_or_insert(piece);
            }
        }
    }
    leaf
}
