//! Immutable, revisioned voxel snapshots for authoritative parallel work.
//!
//! A view contains only chunks explicitly supplied by the simulation thread.
//! Reads never generate terrain or touch storage: a coordinate outside the
//! captured set is an error, so callers must reject work that cannot be
//! completed from authoritative snapshots.

use crate::content::Catalog;
use crate::world::{self, BlockId, CHUNK_VOLUME, Chunk, ChunkKey};
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
    catalog: Arc<Catalog>,
    environment: Option<super::environment::Capture>,
}

impl VoxelView {
    pub(super) fn with_environment(mut self, environment: super::environment::Capture) -> Self {
        self.environment = Some(environment);
        self
    }
    pub(super) fn environment(&self) -> Option<bloxgloom_host_api::gameplay::Environment> {
        self.environment.as_ref().map(|capture| capture.value)
    }
    pub(super) fn environment_capture(&self) -> Option<&super::environment::Capture> {
        self.environment.as_ref()
    }

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
        Self::build(chunks, Arc::new(crate::content::catalog().clone()), true)
    }

    #[allow(
        dead_code,
        reason = "Extensions can validate snapshots against their frozen catalog."
    )]
    pub fn from_chunks_in<I, C>(chunks: I, catalog: Arc<Catalog>) -> Result<Self, SnapshotError>
    where
        I: IntoIterator<Item = C>,
        C: Into<Arc<Chunk>>,
    {
        Self::build(chunks, catalog, true)
    }

    /// Fast path for chunks obtained only from `World::cached_arc_chunk`.
    /// `World` validated/generated their voxels at load or edit time, so a
    /// per-tick scan of all 4,096 blocks in every view would duplicate work.
    #[cfg(test)]
    pub(super) fn from_resident_chunks<I>(chunks: I) -> Result<Self, SnapshotError>
    where
        I: IntoIterator<Item = Arc<Chunk>>,
    {
        Self::build(chunks, Arc::new(crate::content::catalog().clone()), false)
    }

    pub(super) fn from_resident_chunks_in<I>(
        chunks: I,
        catalog: Arc<Catalog>,
    ) -> Result<Self, SnapshotError>
    where
        I: IntoIterator<Item = Arc<Chunk>>,
    {
        Self::build(chunks, catalog, false)
    }

    fn build<I, C>(
        chunks: I,
        catalog: Arc<Catalog>,
        validate_voxels: bool,
    ) -> Result<Self, SnapshotError>
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
                    .find(|(_, block)| catalog.state(*block).is_none())
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
            catalog,
            environment: None,
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

    pub fn is_solid(&self, x: i32, y: i32, z: i32) -> Result<bool, MissingChunk> {
        Ok(self.catalog.block_flags(self.block(x, y, z)?) & crate::content::SOLID != 0)
    }

    pub(super) fn player_rules(&self) -> bloxgloom_host_api::player::PlayerRules {
        self.catalog.player_rules()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MovementError {
    MissingChunk(MissingChunk),
    InvalidCoordinates,
    OutOfBounds,
}

/// Resolves movement in short axis-aligned steps, preserving the server's
/// player hitbox and X/Z/Y axis order. Any unavailable collision sample rejects
/// the whole movement result, so a caller cannot commit a partial position.
#[cfg(test)]
pub fn resolve_player_movement(
    view: &VoxelView,
    position: [f32; 3],
    delta: [f32; 3],
) -> Result<[f32; 3], MovementError> {
    resolve_player_movement_with_body(view, view.player_rules().body(), position, delta)
}

pub(super) fn resolve_player_movement_with_body(
    view: &VoxelView,
    body: bloxgloom_host_api::player::Body,
    position: [f32; 3],
    delta: [f32; 3],
) -> Result<[f32; 3], MovementError> {
    crate::physics::resolve_player_movement(body, position, delta, |x, y, z| {
        Ok(view.catalog.block_flags(view.block(x, y, z)?) & crate::content::SOLID != 0)
    })
    .map_err(|error| match error {
        crate::physics::ResolveError::Missing(chunk) => MovementError::MissingChunk(chunk),
        crate::physics::ResolveError::InvalidCoordinates => MovementError::InvalidCoordinates,
        crate::physics::ResolveError::OutOfBounds => MovementError::OutOfBounds,
    })
}

/// Tests the catalog-selected body at feet, torso and head sample heights.
/// Missing samples are explicit errors rather than guessed empty space.
#[cfg(test)]
pub fn player_collides(view: &VoxelView, feet: [f32; 3]) -> Result<bool, MissingChunk> {
    crate::physics::player_collides(view.player_rules().body(), feet, |x, y, z| {
        Ok(view.catalog.block_flags(view.block(x, y, z)?) & crate::content::SOLID != 0)
    })
}

#[cfg(test)]
mod tests;

#[cfg(test)]
fn air_chunk(key: ChunkKey, version: u64) -> Chunk {
    Chunk {
        key,
        version,
        blocks: vec![AIR; CHUNK_VOLUME].into(),
    }
}

#[cfg(test)]
fn set_block(chunk: &mut Chunk, local: [usize; 3], block: BlockId) {
    let index = Chunk::index(local).unwrap();
    chunk.blocks.set(index, block);
}

#[cfg(test)]
fn key(x: i32, y: i32, z: i32) -> ChunkKey {
    ChunkKey { x, y, z }
}
