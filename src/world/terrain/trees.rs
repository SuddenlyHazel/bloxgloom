//! Regional species and bounded asymmetric crowns, sampled in world space.
use super::materials::palette;
use super::{Biome, generated_block_in_column, lattice_hash, noise2, terrain_column};
use crate::world::{BlockId, LEAVES, WOOD, supports_plant};

pub(in crate::world) const TREE_CELL: i64 = 12;
pub(in crate::world) const TREE_RADIUS: i64 = 6;

#[derive(Clone, Copy)]
pub(in crate::world) struct Tree {
    pub(in crate::world) x: i64,
    pub(in crate::world) z: i64,
    pub(in crate::world) ground_y: i64,
    pub(in crate::world) trunk_top: i64,
    pub(in crate::world) log: BlockId,
    pub(in crate::world) leaves: BlockId,
    log_x: BlockId,
    log_z: BlockId,
    species: usize,
    shape: u64,
}

impl Tree {
    pub(in crate::world) fn presentation_shape(self) -> (u8, u8) {
        let bits = ((self.shape >> 48) & 3)
            | ((self.shape & 1) << 2)
            | (((self.shape >> 3) & 1) << 3)
            | (((self.shape >> 6) & 1) << 4);
        (self.species as u8, bits as u8)
    }
    pub(in crate::world) fn presentation_bark(self) -> (BlockId, BlockId) {
        (self.log_x, self.log_z)
    }
    pub(in crate::world) fn is_log(self, block: BlockId) -> bool {
        block == self.log || block == self.log_x || block == self.log_z
    }
}

pub(in crate::world) fn tree_anchor(cell_x: i64, cell_z: i64, seed: u64) -> Option<Tree> {
    let hash = lattice_hash(seed ^ 0x906f_89ad, cell_x, 0, cell_z);
    let x = cell_x * TREE_CELL + 2 + ((hash >> 8) % 9) as i64;
    let z = cell_z * TREE_CELL + 2 + ((hash >> 16) % 9) as i64;
    if x.abs() <= 14 && z.abs() <= 14 {
        return None;
    }
    let column = terrain_column(x, z, seed);
    if column.water_level.is_some() || column.slope > 0.65 || column.height > 72 {
        return None;
    }
    let cluster = noise2(x, z, 96, seed ^ 0x391a_e723);
    let frequency = match column.biome {
        Biome::Forest => (500.0 + cluster * 300.0) as u64,
        Biome::Plains => (100.0 + cluster * 95.0) as u64,
        Biome::Tundra => 115,
        Biome::Highland => 150,
        Biome::Desert => {
            if column.moisture > -0.32 {
                45
            } else {
                0
            }
        }
    };
    if hash % 1000 >= frequency
        || !supports_plant(generated_block_in_column(x, column.height, z, column, seed))
    {
        return None;
    }
    let region = noise2(x, z, 192, seed ^ 0x837a_59e1);
    let species = match column.biome {
        Biome::Tundra | Biome::Highland => 1,
        Biome::Desert => 4,
        Biome::Forest if column.temperature > 0.36 && column.moisture > 0.32 => 3,
        Biome::Forest if column.shore && column.temperature > 0.10 => 6,
        Biome::Forest if column.temperature < -0.05 => {
            if region > 0.12 {
                2
            } else {
                1
            }
        }
        Biome::Forest if region > 0.38 => 7,
        Biome::Forest if region < -0.40 => 5,
        Biome::Forest if region < -0.22 => 8,
        Biome::Plains if column.temperature > 0.2 && column.moisture < 0.05 => 4,
        _ if region > 0.44 => 7,
        _ if region < -0.24 => 2,
        // Keep the builtin oak as one ecotype, never change its saved identity.
        _ if hash.is_multiple_of(7) => 9,
        _ => 0,
    };
    let (log, leaves) = if species == 9 {
        (WOOD, LEAVES)
    } else {
        palette().trees[species]
    };
    // Living boughs carry bark through every bend and terminal face. Cut log
    // endgrain remains available for placed timber and the vertical trunk.
    let (log_x, log_z) = palette().branch_wood[if species == 9 { 0 } else { species }];
    let base_height = match species {
        1 => 11,
        2 => 8,
        3 => 15,
        4 => 6,
        5 => 10,
        6 => 8,
        _ => 7,
    };
    Some(Tree {
        x,
        z,
        ground_y: column.height,
        trunk_top: column.height + base_height + ((hash >> 28) % 4) as i64,
        log,
        leaves,
        log_x,
        log_z,
        species,
        shape: hash,
    })
}

pub(in crate::world) fn tree_piece(tree: Tree, x: i64, y: i64, z: i64) -> Option<BlockId> {
    let dx = x - tree.x;
    let dz = z - tree.z;
    if y <= tree.ground_y || y > tree.trunk_top + 2 {
        return None;
    }
    if dx == 0 && dz == 0 && y <= tree.trunk_top {
        return Some(tree.log);
    }
    let layer = y - tree.trunk_top;
    if tree.species == 1 {
        // Open trunk, three progressively narrower tiers and a pointed leader.
        let radius = match layer {
            -8 | -6 => 3,
            -7 | -5 | -4 => 2,
            -3 | -1 => 2,
            -2 | 0 | 1 => 1,
            2 => 0,
            _ => return None,
        };
        return (dx.abs() <= radius && dz.abs() <= radius && dx.abs() + dz.abs() <= radius + 1)
            .then_some(tree.leaves);
    }
    // Several connected branch lobes replace identical centered leaf diamonds.
    // The tallest central lobe retains a closed leader for seam/edit baselines.
    let central = dx * dx + dz * dz + layer * layer * 2 <= 10;
    let direction = (tree.shape >> 48) as i64;
    let mut leaf = central;
    for branch in 0..3 {
        let angle = (direction + branch) & 3;
        let (bx, bz) = match angle {
            0 => (1, 0),
            1 => (0, 1),
            2 => (-1, 0),
            _ => (0, -1),
        };
        let reach = if tree.species == 4 || tree.species == 7 {
            3
        } else {
            2
        };
        let by = if tree.species == 4 {
            -1
        } else {
            -2 + ((tree.shape >> (branch * 3)) & 1) as i64
        };
        // Rising boughs have bounded one-voxel steps; every segment joins the
        // trunk or the preceding segment. Logs take priority over all leaves.
        for step in 1..=reach {
            if dx == bx * step
                && dz == bz * step
                && (layer == by - reach + step || layer == by - reach + step - 1)
            {
                return Some(if bx != 0 { tree.log_x } else { tree.log_z });
            }
        }
        let rx = dx - bx * reach;
        let rz = dz - bz * reach;
        let ly = layer - by;
        let vertical = if tree.species == 4 { 4 } else { 2 };
        leaf |= rx * rx + rz * rz + ly * ly * vertical <= 10;
    }
    leaf.then_some(tree.leaves)
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
                if tree.is_log(piece) {
                    return Some(piece);
                }
                leaf.get_or_insert(piece);
            }
        }
    }
    leaf
}

#[cfg(test)]
mod tests;
