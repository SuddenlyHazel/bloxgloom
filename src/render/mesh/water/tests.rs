use super::*;
use crate::world::{AIR, CHUNK_VOLUME, STONE, WATER};
fn chunk(key: ChunkKey, id: world::BlockId) -> Chunk {
    Chunk {
        key,
        version: 1,
        blocks: vec![id; CHUNK_VOLUME].into(),
    }
}
fn mesh(chunk: &Chunk, known: &HashMap<ChunkKey, Arc<Chunk>>) -> ChunkMesh {
    super::super::mesh_chunk_with_catalog(chunk, None, 0, content::catalog(), known)
}
#[test]
fn full_water_chunk_is_six_greedy_faces_and_authoritative_neighbors_hide_seams() {
    let key = ChunkKey { x: -1, y: 1, z: 0 };
    let water = chunk(key, WATER);
    let isolated = mesh(&water, &Default::default());
    assert!(isolated.indices.is_empty() && isolated.cutout_indices.is_empty());
    assert_eq!(isolated.water_indices.len(), 36);
    assert_eq!(isolated.water_vertices.len(), 24 * FLOATS);
    let right = chunk(ChunkKey { x: 0, ..key }, WATER);
    let upper = chunk(ChunkKey { y: 2, ..key }, WATER);
    let known = HashMap::from([(right.key, Arc::new(right)), (upper.key, Arc::new(upper))]);
    assert_eq!(mesh(&water, &known).water_indices.len(), 24);
    // Missing neighbors remain exposed. No procedural terrain may hide them.
    assert_eq!(
        mesh(&water, &HashMap::from([(key, Arc::new(water.clone()))]))
            .water_indices
            .len(),
        36
    );
}
#[test]
fn water_keeps_riverbed_visible_and_hides_submerged_internal_faces() {
    let mut river = chunk(ChunkKey { x: 0, y: 0, z: 0 }, AIR);
    river.blocks.set(Chunk::index([3, 1, 3]).unwrap(), STONE);
    river.blocks.set(Chunk::index([3, 2, 3]).unwrap(), WATER);
    river.blocks.set(Chunk::index([3, 3, 3]).unwrap(), WATER);
    let mesh = mesh(&river, &Default::default());
    assert_eq!(
        mesh.indices.len(),
        36,
        "the opaque riverbed remains visible through water"
    );
    assert_eq!(
        mesh.water_indices.len(),
        30,
        "four merged sides plus top, no bed or internal face"
    );
    assert!(
        mesh.water_vertices
            .chunks_exact(FLOATS)
            .all(|v| (0.0..1.0).contains(&v[9]))
    );
}
