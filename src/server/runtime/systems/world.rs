//! Bounded, authoritative owner-neighborhood capture for public worker jobs.
//! Every observed chunk is fenced until the owner's WAL receipt. Missing
//! chunks are requested asynchronously by the coordinator, never synthesized.
use super::{OwnerKey, TerrainReads};
use crate::world::{CHUNK_SIZE, Chunk, ChunkKey, World};
use std::io::{self, ErrorKind};
use std::sync::Arc;

pub(super) fn capture(
    world: &mut World,
    reads: &mut TerrainReads,
    owner: OwnerKey,
    radius: u8,
    missing: &mut Vec<ChunkKey>,
) -> io::Result<Option<Vec<Arc<Chunk>>>> {
    let center = owner.as_chunk().ok_or_else(|| {
        io::Error::new(ErrorKind::InvalidData, "world-reading owner is not a chunk")
    })?;
    let width = 2 * usize::from(radius) + 1;
    let mut chunks = Vec::with_capacity(width * width * width);
    for dx in -i32::from(radius)..=i32::from(radius) {
        for dy in -i32::from(radius)..=i32::from(radius) {
            for dz in -i32::from(radius)..=i32::from(radius) {
                let key = match (
                    center.x.checked_add(dx),
                    center.y.checked_add(dy),
                    center.z.checked_add(dz),
                ) {
                    (Some(x), Some(y), Some(z)) => ChunkKey { x, y, z },
                    _ => {
                        return Err(io::Error::new(
                            ErrorKind::InvalidInput,
                            "owner neighborhood outside world coordinates",
                        ));
                    }
                };
                let cell = [key.x, key.y, key.z].map(|n| n.checked_mul(CHUNK_SIZE as i32));
                let [Some(x), Some(y), Some(z)] = cell else {
                    return Err(io::Error::new(
                        ErrorKind::InvalidInput,
                        "owner neighborhood outside world coordinates",
                    ));
                };
                if let Some(chunk) = world.cached_arc_chunk(key) {
                    if reads.read(world, x, y, z)?.is_none() {
                        return Err(io::Error::new(
                            ErrorKind::WouldBlock,
                            "captured chunk left the authoritative cache",
                        ));
                    }
                    chunks.push(chunk);
                } else if missing.len() < 8 && !missing.contains(&key) {
                    missing.push(key);
                }
            }
        }
    }
    Ok((chunks.len() == width * width * width).then_some(chunks))
}
