//! Bounded captures of immutable resident authority for distant workers.
use crate::world::{BlockId, CHUNK_SIZE, Chunk, PalettedBlocks, World};
use std::{io, sync::Arc};

impl World {
    /// Capture observed authoritative coverage without retaining gameplay pins.
    /// Arc snapshots remain immutable across eviction and committed replacement.
    pub(crate) fn lod_resident(&self, bounds: [i32; 4]) -> io::Result<Vec<Arc<Chunk>>> {
        const MAX_CHUNKS: usize = 2048;
        const MAX_BYTES: usize = 8 * 1024 * 1024;
        let mut snapshots = Vec::new();
        let mut bytes = 0usize;
        for (&key, entry) in self.cache.entries() {
            let x = i64::from(key.x) * CHUNK_SIZE as i64;
            let z = i64::from(key.z) * CHUNK_SIZE as i64;
            if x < i64::from(bounds[0])
                || x >= i64::from(bounds[2])
                || z < i64::from(bounds[1])
                || z >= i64::from(bounds[3])
            {
                continue;
            }
            let owner = entry.read();
            // Frozen builtin sampling already handles untouched terrain;
            // retaining those versions adds no source authority or coverage.
            if self.generator.is_builtin() && owner.chunk.version == 0 {
                continue;
            }
            let chunk = Arc::clone(&owner.chunk);
            drop(owner);
            bytes += std::mem::size_of::<Chunk>()
                + match &chunk.blocks {
                    PalettedBlocks::Uniform { .. } => 0,
                    PalettedBlocks::Palette8 {
                        palette,
                        indices,
                        counts,
                    } => {
                        palette.capacity() * std::mem::size_of::<BlockId>()
                            + indices.capacity()
                            + counts.capacity() * 2
                    }
                    PalettedBlocks::Palette16 {
                        palette,
                        indices,
                        counts,
                    } => {
                        palette.capacity() * std::mem::size_of::<BlockId>()
                            + indices.capacity() * 2
                            + counts.capacity() * 2
                    }
                    PalettedBlocks::InvalidLength(blocks) => {
                        blocks.capacity() * std::mem::size_of::<BlockId>()
                    }
                };
            if snapshots.len() >= MAX_CHUNKS || bytes > MAX_BYTES {
                return Err(io::Error::other("LOD resident snapshot budget exceeded"));
            }
            snapshots.push(chunk);
        }
        Ok(snapshots)
    }
}
