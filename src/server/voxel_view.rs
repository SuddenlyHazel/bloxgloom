//! Immutable, revisioned voxel snapshots for authoritative parallel work.
//!
//! A view contains only chunks explicitly supplied by the simulation thread.
//! Reads never generate terrain or touch storage: a coordinate outside the
//! captured set is an error, so callers must reject work that cannot be
//! completed from authoritative snapshots.

use crate::world::{self, BlockId, CHUNK_VOLUME, Chunk, ChunkKey, is_solid};
use std::collections::HashMap;
use std::sync::Arc;

#[cfg(test)]
use crate::world::AIR;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MissingChunk {
    pub key: ChunkKey,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotError {
    InvalidBlockCount {
        key: ChunkKey,
        expected: usize,
        actual: usize,
    },
    DuplicateChunk {
        key: ChunkKey,
    },
    InvalidBlock {
        key: ChunkKey,
        index: usize,
        block: BlockId,
    },
}

/// A fixed set of immutable chunks and their captured versions.
///
/// Construct this on the simulation coordinator from already-owned snapshots.
/// Share one `Arc<VoxelView>` among worker jobs; cloning the view itself would
/// copy its lookup table and revision list.
#[derive(Debug)]
pub struct VoxelView {
    chunks: HashMap<ChunkKey, Arc<Chunk>>,
    revisions: Vec<(ChunkKey, u64)>,
}

impl VoxelView {
    /// Captures exactly the chunks supplied. Both `Chunk` and `Arc<Chunk>` are
    /// accepted; duplicate keys and malformed block arrays are rejected.
    /// This validated constructor is available to extension systems that
    /// produce snapshots outside the built-in world's resident-cache path.
    #[allow(dead_code)]
    pub fn from_chunks<I, C>(chunks: I) -> Result<Self, SnapshotError>
    where
        I: IntoIterator<Item = C>,
        C: Into<Arc<Chunk>>,
    {
        Self::build(chunks, true)
    }

    /// Fast path for chunks obtained only from `World::cached_arc_chunk`.
    /// `World` validated/generated their voxels at load or edit time, so a
    /// per-tick scan of all 4,096 blocks in every view would duplicate work.
    pub(super) fn from_resident_chunks<I>(chunks: I) -> Result<Self, SnapshotError>
    where
        I: IntoIterator<Item = Arc<Chunk>>,
    {
        Self::build(chunks, false)
    }

    fn build<I, C>(chunks: I, validate_voxels: bool) -> Result<Self, SnapshotError>
    where
        I: IntoIterator<Item = C>,
        C: Into<Arc<Chunk>>,
    {
        let mut by_key = HashMap::new();
        for chunk in chunks {
            let chunk = chunk.into();
            if chunk.blocks.len() != CHUNK_VOLUME {
                return Err(SnapshotError::InvalidBlockCount {
                    key: chunk.key,
                    expected: CHUNK_VOLUME,
                    actual: chunk.blocks.len(),
                });
            }
            if validate_voxels
                && let Some((index, block)) = chunk
                    .blocks
                    .iter()
                    .copied()
                    .enumerate()
                    .find(|(_, block)| !world::valid_block(*block))
            {
                return Err(SnapshotError::InvalidBlock {
                    key: chunk.key,
                    index,
                    block,
                });
            }
            let key = chunk.key;
            if by_key.insert(key, chunk).is_some() {
                return Err(SnapshotError::DuplicateChunk { key });
            }
        }

        let mut revisions: Vec<_> = by_key
            .values()
            .map(|chunk| (chunk.key, chunk.version))
            .collect();
        revisions.sort_unstable_by_key(|(key, _)| (key.x, key.y, key.z));
        Ok(Self {
            chunks: by_key,
            revisions,
        })
    }

    /// Returns the block from a captured chunk, or identifies the missing
    /// chunk. Negative coordinates use Euclidean chunk division.
    #[inline]
    pub fn block(&self, x: i32, y: i32, z: i32) -> Result<BlockId, MissingChunk> {
        let (key, local) = world::world_to_chunk(x, y, z);
        let chunk = self.chunks.get(&key).ok_or(MissingChunk { key })?;
        let index = Chunk::index(local).expect("world_to_chunk always returns local coordinates");
        Ok(chunk.blocks[index])
    }

    /// Captured revisions in stable lexicographic chunk order.
    ///
    /// Extension schedulers and diagnostics can inspect the exact revisions
    /// whose snapshots are represented by this view.
    #[allow(dead_code)]
    #[inline]
    pub fn revisions(&self) -> &[(ChunkKey, u64)] {
        &self.revisions
    }

    /// Checks whether every captured chunk still has the version used by this
    /// view. The coordinator supplies its current cached-version lookup at the
    /// phase barrier; absent or changed chunks make the result stale.
    pub fn revisions_match(&self, mut current: impl FnMut(ChunkKey) -> Option<u64>) -> bool {
        self.revisions
            .iter()
            .all(|(key, version)| current(*key) == Some(*version))
    }

    /// Returns whether this view contains an authoritative snapshot for `key`.
    /// Extension systems can use this to preflight work before sampling voxels.
    #[allow(dead_code)]
    #[inline]
    pub fn contains_chunk(&self, key: ChunkKey) -> bool {
        self.chunks.contains_key(&key)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MovementError {
    MissingChunk(MissingChunk),
    InvalidCoordinates,
    OutOfBounds,
}

// Server-authenticated movement normally needs at most 11 steps per axis;
// 64 leaves generous headroom while bounding worst-case collision probes.
const MAX_MOVEMENT_STEPS_PER_AXIS: f32 = 64.0;

/// Resolves movement in short axis-aligned steps, preserving the server's
/// player hitbox and X/Z/Y axis order. Any unavailable collision sample rejects
/// the whole movement result, so a caller cannot commit a partial position.
pub fn resolve_player_movement(
    view: &VoxelView,
    mut position: [f32; 3],
    delta: [f32; 3],
) -> Result<[f32; 3], MovementError> {
    if position
        .iter()
        .chain(delta.iter())
        .any(|coordinate| !coordinate.is_finite())
    {
        return Err(MovementError::InvalidCoordinates);
    }

    // Resolve in short axis-aligned steps so even delayed input cannot tunnel
    // through a one-block wall.
    for axis in [0, 2, 1] {
        let step_count = (delta[axis].abs() / 0.25).ceil().max(1.0);
        if step_count > MAX_MOVEMENT_STEPS_PER_AXIS {
            return Err(MovementError::OutOfBounds);
        }
        let steps = step_count as usize;
        let step = delta[axis] / steps as f32;
        for _ in 0..steps {
            let mut candidate = position;
            candidate[axis] += step;
            if candidate.iter().any(|value| value.abs() >= 1_000_000.0) {
                break;
            }
            if player_collides(view, candidate).map_err(MovementError::MissingChunk)? {
                break;
            }
            position = candidate;
        }
    }
    Ok(position)
}

/// Tests the existing player hitbox: 0.6 block wide and 1.7 blocks tall, with
/// samples at the feet, torso, and head. Missing samples are explicit errors.
pub fn player_collides(view: &VoxelView, feet: [f32; 3]) -> Result<bool, MissingChunk> {
    for x in [feet[0] - 0.3, feet[0] + 0.3] {
        for y in [feet[1] + 0.05, feet[1] + 0.9, feet[1] + 1.75] {
            for z in [feet[2] - 0.3, feet[2] + 0.3] {
                if is_solid(view.block(x.floor() as i32, y.floor() as i32, z.floor() as i32)?) {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
fn air_chunk(key: ChunkKey, version: u64) -> Chunk {
    Chunk {
        key,
        version,
        blocks: vec![AIR; CHUNK_VOLUME],
    }
}

#[cfg(test)]
fn set_block(chunk: &mut Chunk, local: [usize; 3], block: BlockId) {
    let index = Chunk::index(local).unwrap();
    chunk.blocks[index] = block;
}

#[cfg(test)]
fn key(x: i32, y: i32, z: i32) -> ChunkKey {
    ChunkKey { x, y, z }
}
