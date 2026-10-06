//! Deterministic terrain generation, biome sampling, and chunk decoration.
mod forest;
mod hydrology;
mod landforms;
mod materials;
mod snow;
#[cfg(test)]
mod tests;
mod trees;
mod vegetation;
pub(super) use vegetation::ground_plant;

#[cfg(test)]
use trees::tree_block_at;
use trees::{TREE_CELL, tree_piece};
pub(super) use trees::{TREE_RADIUS, Tree, tree_anchor};

use std::collections::HashMap;

use super::{
    AIR, BEDROCK_Y, BlockId, CHUNK_SIZE, CHUNK_VOLUME, Chunk, ChunkKey, DIRT, GRASS, GRAVEL,
    MAX_GENERATED_HEIGHT, MOSS, SAND, SNOW, STONE, WATER,
};

pub(super) fn generate_blocks(key: ChunkKey, seed: u64) -> Vec<BlockId> {
    let mut blocks = vec![AIR; CHUNK_VOLUME];
    let bottom = i64::from(key.y) * CHUNK_SIZE as i64;
    if bottom + CHUNK_SIZE as i64 <= i64::from(BEDROCK_Y) {
        blocks.fill(STONE);
        return blocks;
    }
    if bottom > i64::from(MAX_GENERATED_HEIGHT) {
        return blocks;
    }
    let mut patterns = HashMap::new();
    let mut columns = Vec::with_capacity(CHUNK_SIZE * CHUNK_SIZE);
    let mut hydrology = hydrology::Sampler::new(seed);
    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            let world_x = i64::from(key.x) * CHUNK_SIZE as i64 + x as i64;
            let world_z = i64::from(key.z) * CHUNK_SIZE as i64 + z as i64;
            let column = hydrology.column(world_x, world_z);
            columns.push(column);
            let pattern =
                if bottom <= column.height && bottom + CHUNK_SIZE as i64 > column.height - 4 {
                    surface_pattern(world_x, world_z, seed, &mut patterns)
                } else {
                    0
                };
            for y in 0..CHUNK_SIZE {
                let world_y = bottom + y as i64;
                let block =
                    generated_block_with_pattern(world_x, world_y, world_z, column, pattern, seed);
                blocks[Chunk::index([x, y, z]).unwrap()] = block;
            }
        }
    }
    decorate_chunk(key, seed, &mut blocks, &mut patterns, &columns);
    blocks
}

#[cfg(test)]
pub(super) fn generated_block(x: i64, y: i64, z: i64, seed: u64) -> BlockId {
    let column = terrain_column(x, z, seed);
    let ground = generated_block_in_column(x, y, z, column, seed);
    if ground != AIR {
        return ground;
    }
    if let Some(tree) = tree_block_at(x, y, z, seed) {
        return tree;
    }
    if y == column.height + 1 {
        let soil = generated_block_in_column(x, column.height, z, column, seed);
        return ground_plant(x, z, seed, column.biome, soil);
    }
    AIR
}

fn decorate_chunk(
    key: ChunkKey,
    seed: u64,
    blocks: &mut [BlockId],
    patterns: &mut HashMap<(i64, i64), [u8; 64]>,
    columns: &[Column],
) {
    let first_x = i64::from(key.x) * CHUNK_SIZE as i64;
    let first_y = i64::from(key.y) * CHUNK_SIZE as i64;
    let first_z = i64::from(key.z) * CHUNK_SIZE as i64;
    let last_x = first_x + CHUNK_SIZE as i64 - 1;
    let last_z = first_z + CHUNK_SIZE as i64 - 1;
    for cz in
        (first_z - TREE_RADIUS).div_euclid(TREE_CELL)..=(last_z + TREE_RADIUS).div_euclid(TREE_CELL)
    {
        for cx in (first_x - TREE_RADIUS).div_euclid(TREE_CELL)
            ..=(last_x + TREE_RADIUS).div_euclid(TREE_CELL)
        {
            let Some(tree) = tree_anchor(cx, cz, seed) else {
                continue;
            };
            for z in (tree.z - TREE_RADIUS).max(first_z)..=(tree.z + TREE_RADIUS).min(last_z) {
                for x in (tree.x - TREE_RADIUS).max(first_x)..=(tree.x + TREE_RADIUS).min(last_x) {
                    for y in (tree.ground_y + 1).max(first_y)
                        ..=(tree.trunk_top + 2).min(first_y + CHUNK_SIZE as i64 - 1)
                    {
                        if let Some(piece) = tree_piece(tree, x, y, z) {
                            let local = [
                                (x - first_x) as usize,
                                (y - first_y) as usize,
                                (z - first_z) as usize,
                            ];
                            let at = Chunk::index(local).unwrap();
                            if blocks[at] == AIR
                                || (tree.is_log(piece)
                                    && crate::content::jg_rtx::is_leaf(
                                        crate::content::catalog(),
                                        blocks[at],
                                    ))
                            {
                                blocks[at] = piece;
                            }
                        }
                    }
                }
            }
        }
    }
    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            let world_x = first_x + x as i64;
            let world_z = first_z + z as i64;
            let column = columns[x + z * CHUNK_SIZE];
            let world_y = column.height + 1;
            if !(first_y..first_y + CHUNK_SIZE as i64).contains(&world_y) {
                continue;
            }
            let at = Chunk::index([x, (world_y - first_y) as usize, z]).unwrap();
            if blocks[at] == AIR {
                let pattern = surface_pattern(world_x, world_z, seed, patterns);
                let soil = generated_block_with_pattern(
                    world_x,
                    column.height,
                    world_z,
                    column,
                    pattern,
                    seed,
                );
                blocks[at] = ground_plant(world_x, world_z, seed, column.biome, soil);
            }
        }
    }
}

fn lattice_hash(seed: u64, x: i64, y: i64, z: i64) -> u64 {
    mix(seed
        ^ (x as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (y as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ (z as u64).wrapping_mul(0x94d0_49bb_1331_11eb))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Biome {
    Plains,
    Forest,
    Desert,
    Tundra,
    Highland,
}

#[derive(Clone, Copy)]
pub(super) struct Column {
    pub(super) height: i64,
    pub(super) rocky: bool,
    pub(super) biome: Biome,
    pub(super) water_level: Option<i64>,
    water_kind: Option<hydrology::Kind>,
    pub(super) shore: bool,
    rock_region: i64,
    strata_offset: i64,
    temperature: f64,
    moisture: f64,
    slope: f64,
}

pub(super) fn terrain_column(x: i64, z: i64, seed: u64) -> Column {
    hydrology::Sampler::new(seed).column(x, z)
}
fn base_column(x: i64, z: i64, seed: u64) -> Column {
    landforms::column(x, z, seed)
}

pub(crate) fn terrain_height(x: i64, z: i64, seed: u64) -> i64 {
    terrain_column(x, z, seed).height
}

#[cfg(test)]
pub(crate) fn coast_column_diagnostic(x: i64, z: i64, seed: u64) -> String {
    let c = terrain_column(x, z, seed);
    format!(
        "sample=({x},{z}) height={} biome={:?} shore={} rocky={} temp={:.4} slope={:.4} exact_top={:?}",
        c.height,
        c.biome,
        c.shore,
        c.rocky,
        c.temperature,
        c.slope,
        generated_block_in_column(x, c.height, z, c, seed)
    )
}

pub(super) fn generated_block_in_column(
    x: i64,
    y: i64,
    z: i64,
    column: Column,
    seed: u64,
) -> BlockId {
    let pattern = if y >= column.height - 4 && y <= column.height {
        surface_pattern(x, z, seed, &mut HashMap::new())
    } else {
        0
    };
    generated_block_with_pattern(x, y, z, column, pattern, seed)
}

fn generated_block_with_pattern(
    x: i64,
    y: i64,
    z: i64,
    column: Column,
    pattern: u8,
    seed: u64,
) -> BlockId {
    if y <= i64::from(BEDROCK_Y) {
        return STONE;
    }
    if y > column.height {
        if column.water_level.is_some_and(|level| y <= level) {
            WATER
        } else {
            AIR
        }
    } else {
        // Coarse caverns and finer breaks share absolute world coordinates, so
        // both horizontal and vertical chunk faces sample the same field.
        if y >= i64::from(BEDROCK_Y) + 5
            && !(column.water_level.is_some() && y >= column.height - 3)
        {
            let caverns = noise3(x, y, z, 22, seed ^ 0x9907_ae41);
            let breaks = noise3(x, y, z, 9, seed ^ 0x287a_13dc);
            let near_spawn = x.abs() <= 12 && z.abs() <= 12;
            let threshold = if y >= column.height - 5 { 0.66 } else { 0.35 };
            if !(near_spawn && y >= column.height - 7) && caverns + breaks * 0.4 > threshold {
                return AIR;
            }
        }
        if y < column.height - 4 {
            return materials::rock(x, y, z, column, seed);
        }
        if y == column.height {
            let top = if column.biome == Biome::Tundra && !column.shore {
                snow::surface(x, z, column, seed)
            } else if column.rocky && !column.shore {
                if column.height > 74 || column.temperature < -0.55 {
                    SNOW
                } else {
                    materials::rock(x, y, z, column, seed)
                }
            } else if column.shore {
                if matches!(column.biome, Biome::Desert) {
                    SAND
                } else if column.biome == Biome::Forest {
                    materials::palette().soils[3]
                } else {
                    GRAVEL
                }
            } else {
                match column.biome {
                    Biome::Plains => {
                        [GRASS, GRASS, materials::palette().soils[0], STONE][pattern as usize]
                    }
                    Biome::Forest => [
                        GRASS,
                        materials::palette().soils[1],
                        MOSS,
                        materials::palette().soils[2],
                    ][pattern as usize],
                    Biome::Desert => [SAND, SAND, GRAVEL, STONE][pattern as usize],
                    Biome::Tundra => {
                        unreachable!("cold non-shore surfaces use continuous snow cover")
                    }
                    Biome::Highland => [STONE, GRAVEL, GRAVEL, SNOW][pattern as usize],
                }
            };
            return top;
        }
        match column.biome {
            Biome::Desert if y >= column.height - 3 => SAND,
            Biome::Tundra if y >= column.height - 2 => DIRT,
            Biome::Highland if column.rocky || y < column.height - 2 => STONE,
            _ if y >= column.height - 3 => DIRT,
            _ => STONE,
        }
    }
}

/// Collapse a small Wang-like ground-cover field. Border cells are the common
/// substrate, so independently generated regions always meet legally. The
/// solver propagates neighbor constraints after each minimum-entropy choice;
/// it is only evaluated once per region while building a chunk.
fn surface_pattern(x: i64, z: i64, seed: u64, cache: &mut HashMap<(i64, i64), [u8; 64]>) -> u8 {
    const CELL_SIZE: i64 = 4;
    const REGION_SIZE: i64 = 8 * CELL_SIZE;
    let region = (x.div_euclid(REGION_SIZE), z.div_euclid(REGION_SIZE));
    let tiles = cache
        .entry(region)
        .or_insert_with(|| collapse_surface(region, seed));
    let cx = x.rem_euclid(REGION_SIZE) / CELL_SIZE;
    let cz = z.rem_euclid(REGION_SIZE) / CELL_SIZE;
    let tile = tiles[(cx + cz * 8) as usize];
    if tile == 0 {
        return 0;
    }
    let edge = noise2(x, z, 7, seed ^ 0x56d8_2f4a) + noise2(x, z, 3, seed ^ 0x83a2_1c59) * 0.35;
    match tile {
        1 if edge > 0.37 => 2,
        2 if edge < -0.28 => 1,
        3 if edge < -0.32 => 2,
        _ => tile,
    }
}

pub(super) const SURFACE_NEIGHBORS: [u8; 4] = [0b0011, 0b0111, 0b1110, 0b1100];

pub(super) fn collapse_surface(region: (i64, i64), seed: u64) -> [u8; 64] {
    let mut possible = [0b1111u8; 64];
    for z in 0..8 {
        for x in 0..8 {
            if x == 0 || x == 7 || z == 0 || z == 7 {
                possible[x + z * 8] = 1;
            }
        }
    }
    let region_seed = mix(seed
        ^ (region.0 as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (region.1 as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9));
    for step in 0..64u64 {
        // Arc-consistency propagation: each candidate must have support in
        // every neighboring cell. This prevents isolated patch/dense tiles.
        loop {
            let mut changed = false;
            for z in 0..8 {
                for x in 0..8 {
                    let index = x + z * 8;
                    let mut mask = possible[index];
                    for neighbor in [
                        (x > 0).then_some(index.wrapping_sub(1)),
                        (x < 7).then_some(index + 1),
                        (z > 0).then_some(index.wrapping_sub(8)),
                        (z < 7).then_some(index + 8),
                    ]
                    .into_iter()
                    .flatten()
                    {
                        let mut supported = 0;
                        for (tile, allowed) in SURFACE_NEIGHBORS.iter().enumerate() {
                            if possible[neighbor] & allowed != 0 {
                                supported |= 1 << tile;
                            }
                        }
                        mask &= supported;
                    }
                    if mask == 0 {
                        return [0; 64];
                    }
                    if mask != possible[index] {
                        possible[index] = mask;
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        let next = possible
            .iter()
            .enumerate()
            .filter(|(_, mask)| mask.count_ones() > 1)
            .min_by_key(|(index, mask)| {
                (mask.count_ones(), mix(region_seed ^ step ^ *index as u64))
            });
        let Some((index, mask)) = next else {
            break;
        };
        let weights = [1u32, 4, 8, 3];
        let total: u32 = weights
            .iter()
            .enumerate()
            .filter(|(tile, _)| mask & (1 << tile) != 0)
            .map(|(_, weight)| weight)
            .sum();
        let mut choice =
            (mix(region_seed ^ step.wrapping_mul(0x94d0_49bb_1331_11eb)) % u64::from(total)) as u32;
        for (tile, weight) in weights.into_iter().enumerate() {
            if mask & (1 << tile) == 0 {
                continue;
            }
            if choice < weight {
                possible[index] = 1 << tile;
                break;
            }
            choice -= weight;
        }
    }
    std::array::from_fn(|index| possible[index].trailing_zeros() as u8)
}

fn noise2(x: i64, z: i64, scale: i64, seed: u64) -> f64 {
    let x0 = x.div_euclid(scale);
    let z0 = z.div_euclid(scale);
    let tx = smooth(x.rem_euclid(scale) as f64 / scale as f64);
    let tz = smooth(z.rem_euclid(scale) as f64 / scale as f64);
    let a = lerp(lattice(seed, x0, 0, z0), lattice(seed, x0 + 1, 0, z0), tx);
    let b = lerp(
        lattice(seed, x0, 0, z0 + 1),
        lattice(seed, x0 + 1, 0, z0 + 1),
        tx,
    );
    lerp(a, b, tz)
}

fn noise3(x: i64, y: i64, z: i64, scale: i64, seed: u64) -> f64 {
    let x0 = x.div_euclid(scale);
    let y0 = y.div_euclid(scale);
    let z0 = z.div_euclid(scale);
    let tx = smooth(x.rem_euclid(scale) as f64 / scale as f64);
    let ty = smooth(y.rem_euclid(scale) as f64 / scale as f64);
    let tz = smooth(z.rem_euclid(scale) as f64 / scale as f64);
    let layer = |z_lattice| {
        let a = lerp(
            lattice(seed, x0, y0, z_lattice),
            lattice(seed, x0 + 1, y0, z_lattice),
            tx,
        );
        let b = lerp(
            lattice(seed, x0, y0 + 1, z_lattice),
            lattice(seed, x0 + 1, y0 + 1, z_lattice),
            tx,
        );
        lerp(a, b, ty)
    };
    lerp(layer(z0), layer(z0 + 1), tz)
}

fn lattice(seed: u64, x: i64, y: i64, z: i64) -> f64 {
    let hash = lattice_hash(seed, x, y, z);
    ((hash >> 40) as f64 / 8_388_607.5) - 1.0
}

fn smooth(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn mix(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

/// Shared exact builtin sampler for distant summaries. Reuses surface and tree
/// caches across columns, avoiding per-voxel terrain and decoration discovery.
pub(super) struct LodSampler {
    seed: u64,
    patterns: HashMap<(i64, i64), [u8; 64]>,
    trees: HashMap<(i64, i64), Option<Tree>>,
    hydrology: hydrology::Sampler,
}
impl LodSampler {
    pub(super) fn new(seed: u64) -> Self {
        Self {
            seed,
            patterns: HashMap::new(),
            trees: HashMap::new(),
            hydrology: hydrology::Sampler::new(seed),
        }
    }
    pub(super) fn tree_feature(&mut self, cx: i64, cz: i64) -> Option<crate::lod::TreeFeature> {
        let tree = (*self
            .trees
            .entry((cx, cz))
            .or_insert_with(|| tree_anchor(cx, cz, self.seed)))?;
        forest::feature(tree)
    }
    pub(super) fn ground_column(&mut self, x: i64, z: i64, bottom: i32, top: i32) -> Vec<BlockId> {
        let column = self.hydrology.column(x, z);
        let pattern = surface_pattern(x, z, self.seed, &mut self.patterns);
        (bottom..top)
            .map(|y| {
                let y = i64::from(y);
                let ground = generated_block_with_pattern(x, y, z, column, pattern, self.seed);
                if ground == AIR && y == column.height + 1 {
                    let soil = generated_block_with_pattern(
                        x,
                        column.height,
                        z,
                        column,
                        pattern,
                        self.seed,
                    );
                    ground_plant(x, z, self.seed, column.biome, soil)
                } else {
                    ground
                }
            })
            .collect()
    }
    pub(super) fn column(&mut self, x: i64, z: i64, bottom: i32, top: i32) -> Vec<BlockId> {
        let column = self.hydrology.column(x, z);
        let pattern = surface_pattern(x, z, self.seed, &mut self.patterns);
        let mut trees = Vec::new();
        for cz in (z - TREE_RADIUS).div_euclid(TREE_CELL)..=(z + TREE_RADIUS).div_euclid(TREE_CELL)
        {
            for cx in
                (x - TREE_RADIUS).div_euclid(TREE_CELL)..=(x + TREE_RADIUS).div_euclid(TREE_CELL)
            {
                if let Some(tree) = *self
                    .trees
                    .entry((cx, cz))
                    .or_insert_with(|| tree_anchor(cx, cz, self.seed))
                {
                    trees.push(tree);
                }
            }
        }
        (bottom..top)
            .map(|y| {
                let y = i64::from(y);
                let ground = generated_block_with_pattern(x, y, z, column, pattern, self.seed);
                if ground != AIR {
                    return ground;
                }
                let mut leaf = None;
                for tree in &trees {
                    if let Some(piece) = tree_piece(*tree, x, y, z) {
                        if tree.is_log(piece) {
                            return piece;
                        }
                        leaf.get_or_insert(piece);
                    }
                }
                if let Some(leaf) = leaf {
                    return leaf;
                }
                if y == column.height + 1 {
                    let soil = generated_block_with_pattern(
                        x,
                        column.height,
                        z,
                        column,
                        pattern,
                        self.seed,
                    );
                    return ground_plant(x, z, self.seed, column.biome, soil);
                }
                AIR
            })
            .collect()
    }
}

/// Procedural diagnostics/previews only; gameplay reads authoritative chunks.
pub(crate) fn water_feature(x: i64, z: i64, seed: u64) -> Option<(i64, i64, &'static str)> {
    let column = terrain_column(x, z, seed);
    Some((
        column.water_level?,
        column.height,
        match column.water_kind? {
            hydrology::Kind::River => "river",
            hydrology::Kind::Lake => "lake",
            hydrology::Kind::Pond => "pond",
            hydrology::Kind::Ocean => "ocean",
        },
    ))
}
