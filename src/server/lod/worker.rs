//! Detached terrain work. No chunks are installed into simulation or its cache.
use super::*;
use crate::world::{BEDROCK_Y, CHUNK_SIZE, MAX_GENERATED_HEIGHT};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender};

pub(super) struct Job {
    pub key: TileKey,
    pub revision: u64,
    pub overlays: Vec<(ChunkKey, Vec<u8>)>,
    pub resident: Vec<Arc<crate::world::Chunk>>,
    pub children: Option<[LodTile; 4]>,
    pub requested_at: Instant,
    pub cancelled: Arc<AtomicBool>,
}
pub(super) struct Completion {
    pub key: TileKey,
    pub revision: u64,
    pub tile: Option<LodTile>,
    pub elapsed: Duration,
    pub queue_age: Duration,
}

pub(super) fn run(
    world: World,
    root: PathBuf,
    jobs: Receiver<Job>,
    results: SyncSender<Completion>,
) {
    // All persisted summaries are disposable. A fresh service namespace makes a
    // crash before invalidation harmless: old sessions are never read.
    let cache = root.join("lod-cache");
    let _ = fs::remove_dir_all(&cache);
    let _ = fs::create_dir_all(&cache);
    while let Ok(job) = jobs.recv() {
        let started = Instant::now();
        let path = cache.join(format!(
            "{}_{}_{}_{}.tile",
            job.revision, job.key.level, job.key.x, job.key.z
        ));
        let tile = if job.cancelled.load(Ordering::Relaxed) {
            None
        } else {
            load_cached(&path, &world)
                .filter(|tile| tile.key == job.key && tile.revision == job.revision)
                .or_else(|| build(&world, &root, &job).ok())
        };
        let tile = if job.cancelled.load(Ordering::Relaxed) {
            None
        } else {
            tile
        };
        if let Some(tile) = &tile {
            let mut bytes = Vec::new();
            if crate::protocol::write_server_with_catalog(
                &mut bytes,
                &ServerMessage::LodTile {
                    session: 1,
                    request: 1,
                    tile: tile.clone(),
                },
                world.catalog(),
            )
            .is_ok()
            {
                let checksum = cache_checksum(&bytes);
                bytes.extend(checksum.to_le_bytes());
                let temp = path.with_extension("tmp");
                if fs::write(&temp, &bytes).is_ok() {
                    let _ = fs::rename(&temp, &path);
                }
                trim_cache(&cache);
            }
        }
        if results
            .send(Completion {
                key: job.key,
                revision: job.revision,
                tile,
                elapsed: started.elapsed(),
                queue_age: started.saturating_duration_since(job.requested_at),
            })
            .is_err()
        {
            break;
        }
    }
}
fn load_cached(path: &std::path::Path, world: &World) -> Option<LodTile> {
    use std::io::Read;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .ok()?
        .take((crate::protocol::MAX_FRAME + 13) as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    let checksum_at = bytes.len().checked_sub(8)?;
    let checksum = u64::from_le_bytes(bytes[checksum_at..].try_into().ok()?);
    if cache_checksum(&bytes[..checksum_at]) != checksum {
        return None;
    }
    match crate::protocol::read_server_with_catalog(&bytes[..checksum_at], world.catalog()).ok()? {
        ServerMessage::LodTile { tile, .. } => Some(tile),
        _ => None,
    }
}
fn cache_checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}
fn trim_cache(path: &std::path::Path) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    let mut entries: Vec<_> = entries
        .filter_map(Result::ok)
        .filter_map(|v| Some((v.metadata().ok()?.modified().ok()?, v.path())))
        .collect();
    entries.sort_by_key(|v| v.0);
    let excess = entries.len().saturating_sub(128);
    for (_, path) in entries.into_iter().take(excess) {
        let _ = fs::remove_file(path);
    }
}
fn build(world: &World, root: &std::path::Path, job: &Job) -> io::Result<LodTile> {
    if let Some(children) = &job.children
        && let Ok(tile) = crate::lod::reduce_parent(
            job.key,
            job.revision,
            [&children[0], &children[1], &children[2], &children[3]],
            world.catalog(),
        )
    {
        return Ok(tile);
    }
    let bounds = job
        .key
        .bounds()
        .ok_or_else(|| io::Error::other("invalid tile"))?;
    let minx = bounds[0].div_euclid(CHUNK_SIZE as i32);
    let maxx = (bounds[2] - 1).div_euclid(CHUNK_SIZE as i32);
    let minz = bounds[1].div_euclid(CHUNK_SIZE as i32);
    let maxz = (bounds[3] - 1).div_euclid(CHUNK_SIZE as i32);
    let mut keys = BTreeSet::new();
    // Saved high structures are included as disjoint known coverage. Extensions
    // outside this inspected range remain explicitly unknown, never known air.
    for (visited, entry) in fs::read_dir(root)?.enumerate() {
        if visited > 100_000 {
            return Err(io::Error::other("LOD save index budget exceeded"));
        }
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(stem) = name.strip_suffix(".bged") else {
            continue;
        };
        let coords: Vec<_> = stem.split('_').map(str::parse::<i32>).collect();
        if let [Ok(x), Ok(y), Ok(z)] = coords.as_slice()
            && (minx..=maxx).contains(x)
            && (minz..=maxz).contains(z)
        {
            keys.insert(ChunkKey {
                x: *x,
                y: *y,
                z: *z,
            });
        }
    }
    let resident: HashMap<_, _> = job
        .resident
        .iter()
        // Frozen builtin sampling already handles untouched chunks.
        .filter(|c| world.lod_max_level() != 4 || c.version != 0)
        .map(|c| (c.key, c))
        .collect();
    keys.extend(resident.keys().copied());
    for (key, _) in &job.overlays {
        keys.insert(*key);
    }
    if keys.len() > 4096 {
        return Err(io::Error::other("LOD source chunk budget exceeded"));
    }
    let overlays: HashMap<_, _> = job
        .overlays
        .iter()
        .map(|(key, data)| (*key, data))
        .collect();
    let mut chunks = Vec::with_capacity(keys.len());
    for key in keys {
        if job.cancelled.load(Ordering::Relaxed) {
            return Err(io::Error::other("LOD cancelled"));
        }
        if let Some(chunk) = resident.get(&key) {
            chunks.push((***chunk).clone());
            continue;
        }
        let chunk = match overlays.get(&key) {
            Some(bytes) => world.load_chunk_snapshot_uncached(key, bytes)?,
            None => world.load_chunk_uncached(key)?,
        };
        chunks.push(chunk.chunk);
    }
    // Builtin composition has a bounded direct coarse contract. Saved edits
    // still come from complete authoritative snapshots before reduction.
    if let Some(tile) = world.lod_builtin_summary(job.key, job.revision, &chunks) {
        return tile.map_err(io::Error::other);
    }
    // Registered contributors have no coarse coverage contract yet. Preserve
    // their exact composition through detached chunk generation, or explicitly
    // return unavailable when the independent source budget cannot fit.
    let existing: BTreeSet<_> = chunks.iter().map(|c| c.key).collect();
    let source_count = (i64::from(maxx) - i64::from(minx) + 1)
        * (i64::from(maxz) - i64::from(minz) + 1)
        * i64::from(
            MAX_GENERATED_HEIGHT.div_euclid(CHUNK_SIZE as i32)
                - BEDROCK_Y.div_euclid(CHUNK_SIZE as i32)
                + 1,
        );
    let extra_sources = existing
        .iter()
        .filter(|k| {
            k.y < BEDROCK_Y.div_euclid(CHUNK_SIZE as i32)
                || k.y > MAX_GENERATED_HEIGHT.div_euclid(CHUNK_SIZE as i32)
        })
        .count();
    if source_count + extra_sources as i64 > 4096 {
        return Err(io::Error::other("LOD contributor source budget exceeded"));
    }
    for z in minz..=maxz {
        for x in minx..=maxx {
            for y in BEDROCK_Y.div_euclid(CHUNK_SIZE as i32)
                ..=MAX_GENERATED_HEIGHT.div_euclid(CHUNK_SIZE as i32)
            {
                if job.cancelled.load(Ordering::Relaxed) {
                    return Err(io::Error::other("LOD cancelled"));
                }
                let key = ChunkKey { x, y, z };
                if !existing.contains(&key) {
                    chunks.push(world.load_chunk_uncached(key)?.chunk);
                }
            }
        }
    }
    crate::lod::extract(job.key, job.revision, &chunks, world.catalog()).map_err(io::Error::other)
}
