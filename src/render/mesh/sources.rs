//! Worker-side source extraction. Metadata follows the versioned chunk mesh,
//! so stale jobs cannot revive removed lamps and unloading drops sources too.
use super::{Catalog, ResolvedChunk};
use crate::render::local_shadow::Source;
use crate::world::CHUNK_SIZE;
use glam::Vec3;

const MAX_CHUNK_SOURCES: usize = 32;

pub(super) fn collect(chunk: &ResolvedChunk, origin: [f32; 3], catalog: &Catalog) -> Vec<Source> {
    // Resolve emitter properties once per palette entry, not per voxel.
    let palette: Vec<_> = chunk
        .palette
        .iter()
        .map(|block| {
            let range = f32::from(catalog.emission(block.id));
            let reflectance = catalog.reflectance(block.id);
            let peak = *reflectance.iter().max().unwrap_or(&0);
            let color = if peak == 0 {
                [1.0; 3]
            } else {
                reflectance.map(|value| f32::from(value) / f32::from(peak))
            };
            (range, color)
        })
        .collect();
    if palette.iter().all(|(range, _)| *range == 0.0) {
        return Vec::new();
    }
    let mut sources: Vec<Source> = Vec::new();
    for (index, &palette_index) in chunk.indices.iter().enumerate() {
        let (range, color) = palette[usize::from(palette_index)];
        if range <= 0.0 {
            continue;
        }
        // Stable strongest-first bounded selection. Equal sources retain the
        // chunk's canonical x/z/y ordering, independent of hash iteration.
        let insert = sources.partition_point(|source| source.range >= range);
        if insert >= MAX_CHUNK_SOURCES {
            continue;
        }
        let position = Vec3::from_array(origin)
            + Vec3::new(
                (index % CHUNK_SIZE) as f32 + 0.5,
                (index / (CHUNK_SIZE * CHUNK_SIZE)) as f32 + 0.5,
                (index / CHUNK_SIZE % CHUNK_SIZE) as f32 + 0.5,
            );
        if sources.len() == MAX_CHUNK_SOURCES {
            sources.pop();
        }
        sources.insert(
            insert,
            Source {
                position,
                range,
                color,
            },
        );
    }
    sources
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::mesh::mesh_chunk;
    use crate::world::{AIR, CHUNK_VOLUME, Chunk, ChunkKey, GLOWSTONE, STONE};

    #[test]
    fn source_metadata_has_world_position_tint_and_disappears_after_removal() {
        let mut chunk =
            Chunk::from_blocks(ChunkKey { x: -2, y: 3, z: 1 }, 1, vec![AIR; CHUNK_VOLUME]);
        let index = Chunk::index([3, 4, 5]).unwrap();
        chunk.blocks.set(index, GLOWSTONE);
        let mesh = mesh_chunk(&chunk);
        assert_eq!(mesh.local_sources.len(), 1);
        let source = mesh.local_sources[0];
        assert_eq!(source.position, Vec3::new(-28.5, 52.5, 21.5));
        assert_eq!(source.range, 15.0);
        assert!(source.color.iter().all(|value| (0.0..=1.0).contains(value)));
        assert_eq!(source.color.into_iter().fold(0.0_f32, f32::max), 1.0);
        chunk.blocks.set(index, AIR);
        assert!(mesh_chunk(&chunk).local_sources.is_empty());
    }

    #[test]
    fn enclosed_source_is_retained_without_visible_emitter_faces() {
        let mut chunk =
            Chunk::from_blocks(ChunkKey { x: 0, y: 0, z: 0 }, 0, vec![STONE; CHUNK_VOLUME]);
        chunk
            .blocks
            .set(Chunk::index([8, 8, 8]).unwrap(), GLOWSTONE);
        let mesh = mesh_chunk(&chunk);
        assert_eq!(mesh.local_sources.len(), 1);
        assert_eq!(mesh.local_sources[0].position, Vec3::splat(8.5));
    }

    #[test]
    fn dense_source_metadata_is_bounded_and_deterministic() {
        let chunk = Chunk::from_blocks(
            ChunkKey { x: 0, y: 0, z: 0 },
            0,
            vec![GLOWSTONE; CHUNK_VOLUME],
        );
        let first = mesh_chunk(&chunk).local_sources;
        assert_eq!(first.len(), MAX_CHUNK_SOURCES);
        assert_eq!(first, mesh_chunk(&chunk).local_sources);
        let empty = Chunk::from_blocks(chunk.key, 0, vec![AIR; CHUNK_VOLUME]);
        assert!(mesh_chunk(&empty).local_sources.is_empty());
    }
}
