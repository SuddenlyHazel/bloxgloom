//! Shared voxel coordinates, deterministic terrain, and authoritative edits.

use std::collections::{BTreeMap, HashMap};
use std::io;
use std::path::PathBuf;

use crate::storage::{SavedEdits, Storage};

pub const CHUNK_SIZE: usize = 16;
pub const CHUNK_VOLUME: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;
pub const MAX_TERRAIN_HEIGHT: i32 = 64;
pub const TERRAIN_GENERATOR_VERSION: u16 = 2;
pub type BlockId = u8;
pub const AIR: BlockId = 0;
pub const GRASS: BlockId = 1;
pub const DIRT: BlockId = 2;
pub const STONE: BlockId = 3;

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
        if block > STONE {
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
    if bottom > i64::from(MAX_TERRAIN_HEIGHT) {
        return Chunk {
            key,
            version: 0,
            blocks,
        };
    }
    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            let world_x = i64::from(key.x) * CHUNK_SIZE as i64 + x as i64;
            let world_z = i64::from(key.z) * CHUNK_SIZE as i64 + z as i64;
            let column = terrain_column(world_x, world_z, seed);
            for y in 0..CHUNK_SIZE {
                let world_y = bottom + y as i64;
                let block = generated_block_in_column(world_x, world_y, world_z, column, seed);
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

#[derive(Clone, Copy)]
struct Column {
    height: i64,
    rocky: bool,
}

fn terrain_column(x: i64, z: i64, seed: u64) -> Column {
    let continent = noise2(x, z, 256, seed ^ 0x42ab_51a4);
    let climate = noise2(x, z, 384, seed ^ 0x8179_e6f2);
    let hills = noise2(x, z, 64, seed ^ 0xd88a_4f9b);
    let detail = noise2(x, z, 20, seed ^ 0x7c14_1583);
    let ridge = 1.0 - noise2(x, z, 96, seed ^ 0xe7d2_391f).abs();
    let mountain = ((climate + 0.4) * 1.25).clamp(0.0, 1.0);
    let height = (20.0
        + continent * 8.0
        + hills * 6.0
        + detail * 2.0
        + mountain * mountain * ridge * ridge * 24.0)
        .round() as i64;
    Column {
        height,
        rocky: mountain > 0.55 && height > 29,
    }
}

fn generated_block_in_column(x: i64, y: i64, z: i64, column: Column, seed: u64) -> BlockId {
    if y > column.height {
        AIR
    } else {
        // Coarse caverns and finer breaks share absolute world coordinates, so
        // both horizontal and vertical chunk faces sample the same field.
        if y >= -64 {
            let caverns = noise3(x, y, z, 22, seed ^ 0x9907_ae41);
            let breaks = noise3(x, y, z, 9, seed ^ 0x287a_13dc);
            let threshold = if y >= column.height - 2 { 0.52 } else { 0.35 };
            if caverns + breaks * 0.4 > threshold {
                return AIR;
            }
        }
        if column.rocky {
            STONE
        } else if y == column.height {
            GRASS
        } else if y >= column.height - 3 {
            DIRT
        } else {
            STONE
        }
    }
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
        if block > STONE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unknown block identifier",
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
    fn legacy_edits_are_rejected_before_world_opens() {
        let path = test_dir();
        fs::write(path.join("0_0_0.bged"), b"legacy").unwrap();
        assert!(World::new(1, path.clone()).is_err());
        fs::remove_dir_all(path).unwrap();
    }
}
