//! Shared voxel coordinates, deterministic terrain, and authoritative edits.

use std::collections::{BTreeMap, HashMap};
use std::io;
use std::path::PathBuf;

use crate::storage::{SavedEdits, Storage};

pub const CHUNK_SIZE: usize = 16;
pub const CHUNK_VOLUME: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;
pub const MAX_TERRAIN_HEIGHT: i32 = 64;
pub const BEDROCK_Y: i32 = -64;
pub const TERRAIN_GENERATOR_VERSION: u16 = 3;
pub type BlockId = u8;
pub const AIR: BlockId = 0;
pub const GRASS: BlockId = 1;
pub const DIRT: BlockId = 2;
pub const STONE: BlockId = 3;
pub const SAND: BlockId = 4;
pub const SNOW: BlockId = 5;
pub const MOSS: BlockId = 6;
pub const GRAVEL: BlockId = 7;
pub const MAX_BLOCK: BlockId = GRAVEL;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChunkKey {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    pub key: ChunkKey,
    pub version: u64,
    pub blocks: Vec<BlockId>,
}

impl Chunk {
    /// Local x changes fastest, followed by z, then y.
    pub fn index(local: [usize; 3]) -> Option<usize> {
        let [x, y, z] = local;
        if x < CHUNK_SIZE && y < CHUNK_SIZE && z < CHUNK_SIZE {
            Some(x + CHUNK_SIZE * (z + CHUNK_SIZE * y))
        } else {
            None
        }
    }

    pub fn block(&self, local: [usize; 3]) -> Option<BlockId> {
        Self::index(local).map(|index| self.blocks[index])
    }

    /// Returns the new version; an unchanged value does not advance it.
    pub fn set_block(&mut self, local: [usize; 3], block: BlockId) -> Option<u64> {
        if block > MAX_BLOCK {
            return None;
        }
        let index = Self::index(local)?;
        if self.blocks[index] != block {
            let version = self.version.checked_add(1)?;
            self.blocks[index] = block;
            self.version = version;
        }
        Some(self.version)
    }
}

pub fn world_to_chunk(x: i32, y: i32, z: i32) -> (ChunkKey, [usize; 3]) {
    let size = CHUNK_SIZE as i32;
    (
        ChunkKey {
            x: x.div_euclid(size),
            y: y.div_euclid(size),
            z: z.div_euclid(size),
        },
        [
            x.rem_euclid(size) as usize,
            y.rem_euclid(size) as usize,
            z.rem_euclid(size) as usize,
        ],
    )
}

pub fn generate_chunk(key: ChunkKey, seed: u64) -> Chunk {
    let mut blocks = vec![AIR; CHUNK_VOLUME];
    let bottom = i64::from(key.y) * CHUNK_SIZE as i64;
    if bottom + CHUNK_SIZE as i64 <= i64::from(BEDROCK_Y) {
        blocks.fill(STONE);
        return Chunk {
            key,
            version: 0,
            blocks,
        };
    }
    if bottom > i64::from(MAX_TERRAIN_HEIGHT) {
        return Chunk {
            key,
            version: 0,
            blocks,
        };
    }
    let mut patterns = HashMap::new();
    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            let world_x = i64::from(key.x) * CHUNK_SIZE as i64 + x as i64;
            let world_z = i64::from(key.z) * CHUNK_SIZE as i64 + z as i64;
            let column = terrain_column(world_x, world_z, seed);
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
    Chunk {
        key,
        version: 0,
        blocks,
    }
}

fn generated_block(x: i64, y: i64, z: i64, seed: u64) -> BlockId {
    generated_block_in_column(x, y, z, terrain_column(x, z, seed), seed)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Biome {
    Plains,
    Forest,
    Desert,
    Tundra,
    Highland,
}

#[derive(Clone, Copy)]
struct Column {
    height: i64,
    rocky: bool,
    biome: Biome,
}

fn terrain_column(x: i64, z: i64, seed: u64) -> Column {
    let continent = noise2(x, z, 256, seed ^ 0x42ab_51a4);
    let temperature = noise2(x, z, 384, seed ^ 0x8179_e6f2);
    let moisture = noise2(x, z, 320, seed ^ 0x6a03_d2e1);
    let uplift = noise2(x, z, 512, seed ^ 0x9b57_2a13);
    let hills = noise2(x, z, 64, seed ^ 0xd88a_4f9b);
    let detail = noise2(x, z, 24, seed ^ 0x7c14_1583);
    let ridge = 1.0 - noise2(x, z, 96, seed ^ 0xe7d2_391f).abs();
    let mountain = smooth(((uplift - 0.05) / 0.72).clamp(0.0, 1.0));
    let aridity =
        ((temperature + 0.05) * 1.5).clamp(0.0, 1.0) * ((-moisture + 0.05) * 1.5).clamp(0.0, 1.0);
    let forested = (moisture * 1.5).clamp(0.0, 1.0);
    let polar = ((-temperature - 0.1) * 1.6).clamp(0.0, 1.0);
    let dunes = noise2(x + z / 3, z, 42, seed ^ 0xb62d_7a35) * 3.4
        + noise2(x, z, 14, seed ^ 0x7b43_719a) * 0.8;
    let height = (21.0
        + continent * 6.0
        + hills * (3.5 + mountain * 3.0)
        + detail * (1.5 - polar * 0.8)
        + aridity * dunes
        + forested * noise2(x, z, 48, seed ^ 0x2740_83be) * 2.0
        + mountain * mountain * ridge * ridge * 24.0)
        .round() as i64;
    let biome = if mountain > 0.72 && height > 34 {
        Biome::Highland
    } else if temperature < -0.23 {
        Biome::Tundra
    } else if temperature > 0.12 && moisture < -0.12 {
        Biome::Desert
    } else if moisture > 0.08 {
        Biome::Forest
    } else {
        Biome::Plains
    };
    Column {
        height,
        rocky: biome == Biome::Highland && height > 41,
        biome,
    }
}

fn generated_block_in_column(x: i64, y: i64, z: i64, column: Column, seed: u64) -> BlockId {
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
        AIR
    } else {
        // Coarse caverns and finer breaks share absolute world coordinates, so
        // both horizontal and vertical chunk faces sample the same field.
        if y >= i64::from(BEDROCK_Y) + 5 {
            let caverns = noise3(x, y, z, 22, seed ^ 0x9907_ae41);
            let breaks = noise3(x, y, z, 9, seed ^ 0x287a_13dc);
            let near_spawn = x.abs() <= 12 && z.abs() <= 12;
            let threshold = if y >= column.height - 5 { 0.66 } else { 0.35 };
            if !(near_spawn && y >= column.height - 7) && caverns + breaks * 0.4 > threshold {
                return AIR;
            }
        }
        if y < column.height - 4 {
            return STONE;
        }
        let top = match column.biome {
            Biome::Plains => [GRASS, GRASS, GRAVEL, STONE][pattern as usize],
            Biome::Forest => [GRASS, MOSS, MOSS, STONE][pattern as usize],
            Biome::Desert => [SAND, SAND, GRAVEL, STONE][pattern as usize],
            Biome::Tundra => [SNOW, SNOW, GRAVEL, STONE][pattern as usize],
            Biome::Highland => [STONE, GRAVEL, GRAVEL, SNOW][pattern as usize],
        };
        if y == column.height {
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

const SURFACE_NEIGHBORS: [u8; 4] = [0b0011, 0b0111, 0b1110, 0b1100];

fn collapse_surface(region: (i64, i64), seed: u64) -> [u8; 64] {
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
    let hash = mix(seed
        ^ (x as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (y as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ (z as u64).wrapping_mul(0x94d0_49bb_1331_11eb));
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

struct CacheEntry {
    chunk: Chunk,
    edits: BTreeMap<u16, BlockId>,
    last_used: u64,
}

pub struct World {
    seed: u64,
    storage: Storage,
    cache: HashMap<ChunkKey, CacheEntry>,
    max_cached_chunks: usize,
    clock: u64,
}

impl World {
    pub fn new(seed: u64, path: PathBuf) -> io::Result<Self> {
        Self::with_capacity(seed, path, 512)
    }

    pub fn with_capacity(seed: u64, path: PathBuf, max_cached_chunks: usize) -> io::Result<Self> {
        if max_cached_chunks == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cache capacity must be positive",
            ));
        }
        Ok(Self {
            seed,
            storage: Storage::new(path, seed)?,
            cache: HashMap::new(),
            max_cached_chunks,
            clock: 0,
        })
    }

    /// Returns an owned, stable snapshot suitable for sending or meshing.
    pub fn get_chunk(&mut self, key: ChunkKey) -> io::Result<Chunk> {
        self.ensure_loaded(key)?;
        Ok(self.cache[&key].chunk.clone())
    }

    pub fn get_block(&mut self, x: i32, y: i32, z: i32) -> io::Result<BlockId> {
        let (key, local) = world_to_chunk(x, y, z);
        self.ensure_loaded(key)?;
        Ok(self.cache[&key].chunk.block(local).unwrap())
    }

    /// A successful return means the changed block and version have been saved.
    pub fn edit(&mut self, x: i32, y: i32, z: i32, block: BlockId) -> io::Result<(ChunkKey, u64)> {
        if block > MAX_BLOCK {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unknown block identifier",
            ));
        }
        if y <= BEDROCK_Y {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "world bottom is immutable",
            ));
        }
        let (key, local) = world_to_chunk(x, y, z);
        self.ensure_loaded(key)?;
        let index = Chunk::index(local).unwrap();
        let entry = &self.cache[&key];
        if entry.chunk.blocks[index] == block {
            return Ok((key, entry.chunk.version));
        }
        let version = entry
            .chunk
            .version
            .checked_add(1)
            .ok_or_else(|| io::Error::other("chunk version exhausted"))?;
        let mut edits = entry.edits.clone();
        let baseline = generated_block(i64::from(x), i64::from(y), i64::from(z), self.seed);
        if block == baseline {
            edits.remove(&(index as u16));
        } else {
            edits.insert(index as u16, block);
        }
        if let Err(error) = self.storage.save(
            key,
            &SavedEdits {
                version,
                blocks: edits.clone(),
            },
        ) {
            // A rename can succeed before a later directory sync fails. Reload on
            // the next access so memory cannot diverge from whichever file won.
            self.cache.remove(&key);
            return Err(error);
        }
        let entry = self.cache.get_mut(&key).unwrap();
        let committed_version = entry
            .chunk
            .set_block(local, block)
            .expect("validated edit must update the chunk");
        assert_eq!(committed_version, version);
        entry.edits = edits;
        Ok((key, version))
    }

    fn ensure_loaded(&mut self, key: ChunkKey) -> io::Result<()> {
        self.clock = self.clock.wrapping_add(1);
        if let Some(entry) = self.cache.get_mut(&key) {
            entry.last_used = self.clock;
            return Ok(());
        }
        let saved = self.storage.load(key)?;
        let mut chunk = generate_chunk(key, self.seed);
        for (&index, &block) in &saved.blocks {
            chunk.blocks[index as usize] = block;
        }
        chunk.version = saved.version;
        if self.cache.len() >= self.max_cached_chunks
            && let Some((&oldest, _)) = self.cache.iter().min_by_key(|(_, entry)| entry.last_used)
        {
            self.cache.remove(&oldest);
        }
        self.cache.insert(
            key,
            CacheEntry {
                chunk,
                edits: saved.blocks,
                last_used: self.clock,
            },
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn test_dir() -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "bloxgloom-world-{}-{stamp}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn negative_and_boundary_coordinates() {
        assert_eq!(
            world_to_chunk(-1, -16, -17),
            (
                ChunkKey {
                    x: -1,
                    y: -1,
                    z: -2
                },
                [15, 0, 15]
            )
        );
        assert_eq!(
            world_to_chunk(16, 15, 0),
            (ChunkKey { x: 1, y: 0, z: 0 }, [0, 15, 0])
        );
        assert_eq!(world_to_chunk(i32::MIN, 0, i32::MAX).1, [0, 0, 15]);
    }

    #[test]
    fn edits_survive_restart_and_cache_eviction() {
        let path = test_dir();
        let mut world = World::with_capacity(42, path.clone(), 1).unwrap();
        let original = world.get_block(-1, 100, 16).unwrap();
        let replacement = if original == STONE { AIR } else { STONE };
        let (key, version) = world.edit(-1, 100, 16, replacement).unwrap();
        assert_eq!(key, ChunkKey { x: -1, y: 6, z: 1 });
        assert_eq!(version, 1);
        let (adjacent_key, adjacent_version) = world.edit(0, 100, 16, replacement).unwrap();
        assert_eq!(adjacent_key, ChunkKey { x: 0, y: 6, z: 1 });
        assert_eq!(adjacent_version, 1);
        world.get_chunk(ChunkKey { x: 100, y: 0, z: 0 }).unwrap();
        assert_eq!(world.get_block(-1, 100, 16).unwrap(), replacement);
        drop(world);
        let mut reopened = World::new(42, path.clone()).unwrap();
        assert_eq!(reopened.get_block(-1, 100, 16).unwrap(), replacement);
        assert_eq!(reopened.get_block(0, 100, 16).unwrap(), replacement);
        assert_eq!(reopened.get_chunk(key).unwrap().version, version);
        assert_eq!(reopened.edit(-1, 100, 16, original).unwrap().1, 2);
        drop(reopened);
        let mut reopened = World::new(42, path.clone()).unwrap();
        assert_eq!(reopened.get_block(-1, 100, 16).unwrap(), original);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn corrupt_save_is_not_silently_discarded() {
        let path = test_dir();
        let key = ChunkKey { x: 0, y: 0, z: 0 };
        let mut world = World::new(1, path.clone()).unwrap();
        world.edit(0, 15, 0, AIR).unwrap();
        drop(world);
        let save_path = path.join("0_0_0.bged");
        fs::write(save_path, b"corrupt").unwrap();
        assert!(World::new(1, path.clone()).unwrap().get_chunk(key).is_err());
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn interrupted_temporary_save_does_not_replace_committed_edit() {
        let path = test_dir();
        let mut world = World::new(7, path.clone()).unwrap();
        let (key, version) = world.edit(1, 30, 1, STONE).unwrap();
        drop(world);
        fs::write(path.join(".0_1_0.999.999.tmp"), b"partial write").unwrap();
        let mut reopened = World::new(7, path.clone()).unwrap();
        assert_eq!(reopened.get_block(1, 30, 1).unwrap(), STONE);
        assert_eq!(reopened.get_chunk(key).unwrap().version, version);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn terrain_is_deterministic_and_continuous_across_chunk_faces() {
        let seed = 0xB10C_6100;
        let origin = ChunkKey { x: -1, y: 0, z: 0 };
        let east = ChunkKey { x: 0, y: 0, z: 0 };
        let above = ChunkKey { x: -1, y: 1, z: 0 };
        let chunk = generate_chunk(origin, seed);
        assert_eq!(chunk, generate_chunk(origin, seed));
        assert_ne!(chunk, generate_chunk(origin, seed + 1));
        let east_chunk = generate_chunk(east, seed);
        let above_chunk = generate_chunk(above, seed);
        for z in 0..CHUNK_SIZE {
            for y in 0..CHUNK_SIZE {
                assert_eq!(
                    chunk.block([15, y, z]),
                    Some(generated_block(-1, y as i64, z as i64, seed))
                );
                assert_eq!(
                    east_chunk.block([0, y, z]),
                    Some(generated_block(0, y as i64, z as i64, seed))
                );
            }
            for x in 0..CHUNK_SIZE {
                assert_eq!(
                    chunk.block([x, 15, z]),
                    Some(generated_block(x as i64 - 16, 15, z as i64, seed))
                );
                assert_eq!(
                    above_chunk.block([x, 0, z]),
                    Some(generated_block(x as i64 - 16, 16, z as i64, seed))
                );
            }
        }
        for z in -64..=64 {
            let left = terrain_column(-1, z, seed).height;
            let right = terrain_column(0, z, seed).height;
            assert!((left - right).abs() <= 3, "height jumps at x chunk seam");
            let north = terrain_column(z, 15, seed).height;
            let south = terrain_column(z, 16, seed).height;
            assert!((north - south).abs() <= 3, "height jumps at z chunk seam");
        }
    }

    #[test]
    fn terrain_has_broad_variation_and_caves() {
        let seed = 0xB10C_6100;
        let mut min_height = i64::MAX;
        let mut max_height = i64::MIN;
        let mut rocky_columns = 0;
        for z in (-512..=512).step_by(16) {
            for x in (-512..=512).step_by(16) {
                let column = terrain_column(x, z, seed);
                min_height = min_height.min(column.height);
                max_height = max_height.max(column.height);
                rocky_columns += usize::from(column.rocky);
                assert!(column.height <= i64::from(MAX_TERRAIN_HEIGHT));
            }
        }
        assert!(
            max_height - min_height >= 15,
            "terrain should have hills and valleys"
        );
        assert!(rocky_columns > 0, "rocky uplands should occur");

        let mut cave_air = 0;
        let mut cave_entrances = 0;
        for z in (-64..=64).step_by(4) {
            for x in (-64..=64).step_by(4) {
                let column = terrain_column(x, z, seed);
                cave_entrances += usize::from(
                    generated_block_in_column(x, column.height, z, column, seed) == AIR,
                );
                for y in 0..column.height - 3 {
                    cave_air +=
                        usize::from(generated_block_in_column(x, y, z, column, seed) == AIR);
                }
            }
        }
        assert!(cave_air > 0, "underground caves should occur");
        assert!(cave_entrances > 0, "some caves should open at the surface");
    }

    #[test]
    fn biomes_cover_distinct_surfaces_across_an_endless_world() {
        let seed = 0xB10C_6100;
        let mut seen = [false; 5];
        let mut samples = [None; 5];
        for z in (-1024..=1024).step_by(32) {
            for x in (-1024..=1024).step_by(32) {
                let column = terrain_column(x, z, seed);
                let index = match column.biome {
                    Biome::Plains => 0,
                    Biome::Forest => 1,
                    Biome::Desert => 2,
                    Biome::Tundra => 3,
                    Biome::Highland => 4,
                };
                seen[index] = true;
                samples[index].get_or_insert((x, z));
                assert!(column.height <= i64::from(MAX_TERRAIN_HEIGHT));
            }
        }
        assert!(seen.into_iter().all(|present| present));
        eprintln!("biome preview coordinates: {samples:?}");
        let distant = ChunkKey {
            x: 100_000,
            y: 1,
            z: -100_000,
        };
        assert_eq!(generate_chunk(distant, seed), generate_chunk(distant, seed));
    }

    #[test]
    fn collapsed_surface_obeys_constraints_and_matches_region_edges() {
        let seed = 0xB10C_6100;
        let left = collapse_surface((0, 0), seed);
        let right = collapse_surface((1, 0), seed);
        assert_eq!(left, collapse_surface((0, 0), seed));
        assert!(
            left.iter().any(|tile| *tile >= 2),
            "collapse should make patches"
        );
        for field in [left, right] {
            for z in 0..8 {
                for x in 0..8 {
                    let tile = field[x + z * 8];
                    assert!(tile < 4);
                    if x < 7 {
                        assert_ne!(
                            SURFACE_NEIGHBORS[tile as usize] & (1 << field[x + 1 + z * 8]),
                            0
                        );
                    }
                    if z < 7 {
                        assert_ne!(
                            SURFACE_NEIGHBORS[tile as usize] & (1 << field[x + (z + 1) * 8]),
                            0
                        );
                    }
                }
            }
        }
        for z in 0..8 {
            assert_eq!(left[7 + z * 8], 0);
            assert_eq!(right[z * 8], 0);
        }
        for region_z in -4..4 {
            for region_x in -4..4 {
                let field = collapse_surface((region_x, region_z), seed);
                assert!(
                    field.iter().any(|tile| *tile != 0),
                    "collapse fell back to empty at region ({region_x}, {region_z})"
                );
            }
        }
    }

    #[test]
    fn world_bottom_is_solid_and_cannot_be_edited() {
        let seed = 17;
        for (x, z) in [(0, 0), (123_456, -654_321)] {
            assert_eq!(generated_block(x, i64::from(BEDROCK_Y), z, seed), STONE);
            assert_eq!(
                generated_block(x, i64::from(BEDROCK_Y) - 1000, z, seed),
                STONE
            );
        }
        let path = test_dir();
        let mut world = World::new(seed, path.clone()).unwrap();
        assert_eq!(world.get_block(0, BEDROCK_Y, 0).unwrap(), STONE);
        assert_eq!(
            world.edit(0, BEDROCK_Y, 0, AIR).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn old_generator_world_is_rejected_without_changing_its_metadata() {
        let path = test_dir();
        let mut metadata = Vec::from(*b"BGWD");
        metadata.extend_from_slice(&2u16.to_le_bytes());
        metadata.extend_from_slice(&17u64.to_le_bytes());
        let metadata_path = path.join("world.meta");
        std::fs::write(&metadata_path, &metadata).unwrap();
        assert_eq!(
            World::new(17, path.clone()).err().unwrap().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(std::fs::read(metadata_path).unwrap(), metadata);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn legacy_edits_are_rejected_before_world_opens() {
        let path = test_dir();
        fs::write(path.join("0_0_0.bged"), b"legacy").unwrap();
        assert!(World::new(1, path.clone()).is_err());
        fs::remove_dir_all(path).unwrap();
    }
}
