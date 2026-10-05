//! Detached terrain work. No chunks are installed into simulation or its cache.
use super::*;
use crate::world::{BEDROCK_Y, CHUNK_SIZE, MAX_GENERATED_HEIGHT};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
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
    pub cache_hit: bool,
    pub sources: Duration,
    pub cache_read: Duration,
    pub generation: Duration,
    pub cache_write: Duration,
    pub finished: Instant,
}

pub(super) fn run(
    world: World,
    root: PathBuf,
    jobs: Arc<Mutex<Receiver<Job>>>,
    results: SyncSender<Completion>,
    disk: Arc<Mutex<()>>,
) {
    let cache = root.join("lod-cache");
    let _ = fs::create_dir_all(&cache);
    loop {
        // Release the receiver before computation: each lane owns one job.
        let received = jobs.lock().unwrap().recv();
        let Ok(job) = received else {
            break;
        };
        let started = Instant::now();
        let path = super::disk::path(&cache, job.key);
        let sources = (!job.cancelled.load(Ordering::Relaxed))
            .then(|| super::sources::Sources::capture(&world, &root, &job))
            .and_then(Result::ok);
        let source_time = started.elapsed();
        let read_at = Instant::now();
        let cached = sources
            .as_ref()
            .and_then(|s| super::disk::load(&path, s.stamp, &world, &job));
        let cache_read = read_at.elapsed();
        let cache_hit = cached.is_some();
        let build_at = Instant::now();
        let tile = cached.or_else(|| sources.as_ref().and_then(|s| build(&world, &job, s).ok()));
        let generation = if cache_hit {
            Duration::ZERO
        } else {
            build_at.elapsed()
        };
        let tile = if job.cancelled.load(Ordering::Relaxed) {
            None
        } else {
            tile
        };
        let write_at = Instant::now();
        if let Some(tile) = &tile {
            // Atomic replacements may run concurrently, but cap enforcement
            // and temporary-file lifecycle share one short filesystem lane.
            let _guard = disk.lock().unwrap();
            if !job.cancelled.load(Ordering::Relaxed) {
                super::disk::store(&path, sources.as_ref().unwrap().stamp, &world, tile);
            }
        }
        let cache_write = write_at.elapsed();
        if results
            .send(Completion {
                key: job.key,
                revision: job.revision,
                tile,
                elapsed: started.elapsed(),
                queue_age: started.saturating_duration_since(job.requested_at),
                cache_hit,
                sources: source_time,
                cache_read,
                generation,
                cache_write,
                finished: Instant::now(),
            })
            .is_err()
        {
            break;
        }
    }
}
fn build(world: &World, job: &Job, sources: &super::sources::Sources) -> io::Result<LodTile> {
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
    let keys: BTreeSet<_> = sources
        .saved
        .keys()
        .chain(sources.resident.keys())
        .copied()
        .collect();
    if keys.len() > 4096 {
        return Err(io::Error::other("LOD source chunk budget exceeded"));
    }
    let mut chunks = Vec::with_capacity(keys.len());
    for key in keys {
        if job.cancelled.load(Ordering::Relaxed) {
            return Err(io::Error::other("LOD cancelled"));
        }
        if let Some(chunk) = sources.resident.get(&key) {
            chunks.push((**chunk).clone());
            continue;
        }
        let bytes = sources
            .saved
            .get(&key)
            .and_then(Option::as_deref)
            .unwrap_or_default();
        let chunk = world.load_chunk_snapshot_uncached(key, bytes)?;
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
