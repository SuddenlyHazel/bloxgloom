//! Frozen source inputs shared by persistent-cache validation and generation.
use super::{worker::Job, *};
use crate::world::{CHUNK_SIZE, Chunk};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

pub(super) struct Sources {
    pub saved: BTreeMap<ChunkKey, Option<Vec<u8>>>,
    pub resident: BTreeMap<ChunkKey, Arc<Chunk>>,
    pub stamp: [u8; 32],
}
impl Sources {
    pub fn capture(world: &World, root: &Path, job: &Job) -> io::Result<Self> {
        let bounds = job
            .key
            .bounds()
            .ok_or_else(|| io::Error::other("invalid tile"))?;
        let inside = |key: ChunkKey| {
            let x = i64::from(key.x) * CHUNK_SIZE as i64;
            let z = i64::from(key.z) * CHUNK_SIZE as i64;
            x >= i64::from(bounds[0])
                && x < i64::from(bounds[2])
                && z >= i64::from(bounds[1])
                && z < i64::from(bounds[3])
        };
        let mut saved = BTreeMap::new();
        let mut bytes = 0;
        for (visited, entry) in fs::read_dir(root)?.enumerate() {
            if visited > 100_000 {
                return Err(io::Error::other("LOD save index budget exceeded"));
            }
            let entry = entry?;
            let name = entry.file_name();
            let Some(stem) = name.to_str().and_then(|n| n.strip_suffix(".bged")) else {
                continue;
            };
            let coords: Vec<_> = stem.split('_').map(str::parse::<i32>).collect();
            if let [Ok(x), Ok(y), Ok(z)] = coords.as_slice() {
                let key = ChunkKey {
                    x: *x,
                    y: *y,
                    z: *z,
                };
                if inside(key) {
                    if job.cancelled.load(Ordering::Relaxed) {
                        return Err(io::Error::other("LOD cancelled"));
                    }
                    let snapshot = world.storage_handle().read_snapshot(key)?;
                    bytes += snapshot.as_ref().map_or(0, Vec::len);
                    if bytes > 16 * 1024 * 1024 || saved.len() >= 4096 {
                        return Err(io::Error::other("LOD source snapshot budget exceeded"));
                    }
                    saved.insert(key, snapshot);
                }
            }
        }
        // The coordinator's committed snapshots win over checkpoint files,
        // including explicit deletion of an override. Keep those frozen bytes
        // for construction as well as hashing; never reread after validation.
        for (key, snapshot) in &job.overlays {
            if !inside(*key) {
                return Err(io::Error::other("LOD overlay outside tile"));
            }
            saved.insert(*key, (!snapshot.is_empty()).then(|| snapshot.clone()));
        }
        let resident: BTreeMap<_, _> = job
            .resident
            .iter()
            .filter(|c| world.lod_max_level() != 4 || c.version != 0)
            .map(|c| (c.key, c.clone()))
            .collect();
        if resident.keys().any(|key| !inside(*key)) || saved.len() + resident.len() > 8192 {
            return Err(io::Error::other("LOD resident source budget exceeded"));
        }
        let total: usize = saved.values().flatten().map(Vec::len).sum();
        if total > 16 * 1024 * 1024 {
            return Err(io::Error::other("LOD source snapshot budget exceeded"));
        }
        let mut hash = Sha256::new();
        hash.update(world.lod_cache_identity());
        hash.update([job.key.level]);
        hash.update(job.key.x.to_le_bytes());
        hash.update(job.key.z.to_le_bytes());
        for (key, snapshot) in &saved {
            hash.update([0]);
            key_hash(&mut hash, *key);
            let snapshot = snapshot.as_deref().unwrap_or_default();
            hash.update((snapshot.len() as u64).to_le_bytes());
            hash.update(snapshot);
        }
        for (key, chunk) in &resident {
            hash.update([1]);
            key_hash(&mut hash, *key);
            hash.update(chunk.version.to_le_bytes());
            for state in chunk.blocks.iter() {
                hash.update(state.get().to_le_bytes());
            }
        }
        // Reduced parents also depend on children's explicit examined heights.
        // Revision numbers are session-local and deliberately excluded.
        if let Some(children) = &job.children {
            for child in children {
                hash.update([2]);
                for column in &child.columns {
                    hash.update((column.coverage.len() as u32).to_le_bytes());
                    for v in &column.coverage {
                        hash.update(v.bottom.to_le_bytes());
                        hash.update(v.top.to_le_bytes());
                    }
                    hash.update((column.spans.len() as u32).to_le_bytes());
                    for v in &column.spans {
                        hash.update(v.bottom.to_le_bytes());
                        hash.update(v.top.to_le_bytes());
                        hash.update(v.state.get().to_le_bytes());
                        hash.update([v.sky, v.glow]);
                    }
                }
            }
        }
        Ok(Self {
            saved,
            resident,
            stamp: hash.finalize().into(),
        })
    }
}
fn key_hash(hash: &mut Sha256, key: ChunkKey) {
    hash.update(key.x.to_le_bytes());
    hash.update(key.y.to_le_bytes());
    hash.update(key.z.to_le_bytes());
}
