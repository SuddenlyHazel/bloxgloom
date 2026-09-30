//! Client prediction against streamed authoritative chunks.

use crate::content::Catalog;
use crate::world::{Chunk, ChunkKey};
use glam::Vec3;
use std::collections::HashMap;
use std::sync::Arc;
mod teleport;

pub(super) fn predict_player_movement(
    chunks: &HashMap<ChunkKey, Arc<Chunk>>,
    catalog: &Catalog,
    position: Vec3,
    delta: Vec3,
) -> Vec3 {
    crate::physics::resolve_player_movement(
        catalog.player_rules().body(),
        position.to_array(),
        delta.to_array(),
        |x, y, z| {
            let (key, local) = crate::world::world_to_chunk(x, y, z);
            let block = chunks
                .get(&key)
                .and_then(|chunk| chunk.block(local))
                .ok_or(())?;
            Ok::<bool, ()>(catalog.block_flags(block) & crate::content::SOLID != 0)
        },
    )
    .map(Vec3::from_array)
    // Missing chunks are unknown, not air. Server input still goes through.
    .unwrap_or(position)
}

#[cfg(test)]
pub(crate) mod teleport_tests;
#[cfg(test)]
mod tests;
