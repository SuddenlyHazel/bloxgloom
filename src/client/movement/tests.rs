use super::*;
use crate::world::{AIR, CHUNK_VOLUME, STONE};

#[test]
fn descent_prediction_stops_at_streamed_ground() {
    let mut chunk = Chunk {
        key: ChunkKey { x: 0, y: 0, z: 0 },
        version: 1,
        blocks: vec![AIR; CHUNK_VOLUME].into(),
    };
    chunk.blocks.set(Chunk::index([0, 0, 0]).unwrap(), STONE);
    let mut chunks = HashMap::new();
    chunks.insert(chunk.key, Arc::new(chunk));
    let position = Vec3::new(0.5, 1.0, 0.5);
    let catalog = Catalog::builtins();

    assert_eq!(
        predict_player_movement(&chunks, &catalog, position, Vec3::new(0.0, -0.2, 0.0)),
        position
    );
    assert_eq!(
        predict_player_movement(
            &HashMap::new(),
            &catalog,
            position,
            Vec3::new(0.0, -0.2, 0.0)
        ),
        position
    );
}
