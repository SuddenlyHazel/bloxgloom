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
fn fluid_frontier_keeps_caps_and_known_air_proves_six_greedy_faces() {
    let key = ChunkKey { x: -1, y: 1, z: 0 };
    let water = chunk(key, WATER);
    let isolated = mesh(&water, &Default::default());
    assert!(isolated.indices.is_empty() && isolated.cutout_indices.is_empty());
    assert_eq!(isolated.water_indices.len(), 12);
    assert_eq!(isolated.water_vertices.len(), 8 * FLOATS);
    assert!(
        isolated
            .water_vertices
            .chunks_exact(FLOATS)
            .all(|v| v[4].abs() == 1.0),
        "unknown lateral edges must not invent translucent shore walls"
    );
    let air_keys = [
        ChunkKey { x: -2, ..key },
        ChunkKey { x: 0, ..key },
        ChunkKey { y: 0, ..key },
        ChunkKey { y: 2, ..key },
        ChunkKey { z: -1, ..key },
        ChunkKey { z: 1, ..key },
    ];
    let mut known: HashMap<_, _> = air_keys
        .into_iter()
        .map(|key| (key, Arc::new(chunk(key, AIR))))
        .collect();
    assert_eq!(mesh(&water, &known).water_indices.len(), 36);
    let right = chunk(ChunkKey { x: 0, ..key }, WATER);
    let upper = chunk(ChunkKey { y: 2, ..key }, WATER);
    known.insert(right.key, Arc::new(right));
    known.insert(upper.key, Arc::new(upper));
    assert_eq!(mesh(&water, &known).water_indices.len(), 24);
    // Having only the current snapshot never certifies an exterior shoreline.
    assert_eq!(
        mesh(&water, &HashMap::from([(key, Arc::new(water.clone()))]))
            .water_indices
            .len(),
        12
    );
}

#[test]
fn negative_seam_wall_appears_only_after_air_snapshot_and_retires_after_water() {
    let key = ChunkKey { x: -1, y: 1, z: 0 };
    let mut water = chunk(key, AIR);
    water.blocks.set(Chunk::index([15, 3, 3]).unwrap(), WATER);
    let mut known = HashMap::new();
    assert_eq!(mesh(&water, &known).water_indices.len(), 30);
    let mut neighbor = chunk(ChunkKey { x: 0, ..key }, AIR);
    known.insert(neighbor.key, Arc::new(neighbor.clone()));
    assert_eq!(mesh(&water, &known).water_indices.len(), 36);
    neighbor.blocks.set(Chunk::index([0, 3, 3]).unwrap(), WATER);
    known.insert(neighbor.key, Arc::new(neighbor.clone()));
    assert_eq!(mesh(&water, &known).water_indices.len(), 30);
    neighbor.blocks.set(Chunk::index([0, 3, 3]).unwrap(), STONE);
    known.insert(neighbor.key, Arc::new(neighbor));
    assert_eq!(mesh(&water, &known).water_indices.len(), 30);
    known.clear();
    assert_eq!(mesh(&water, &known).water_indices.len(), 30);
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
