use super::*;

#[test]
fn walking_prediction_keeps_horizontal_input_and_ignores_free_flight_height() {
    let catalog = crate::content::Catalog::builtins();
    let key = ChunkKey { x: 0, y: 0, z: 0 };
    let chunks = HashMap::from([(
        key,
        Arc::new(Chunk::from_blocks(
            key,
            1,
            vec![crate::world::AIR; crate::world::CHUNK_VOLUME],
        )),
    )]);
    let position = Vec3::new(1.5, 1.0, 1.5);
    assert_eq!(
        predict_player_movement_with_mode(
            &chunks,
            &catalog,
            position,
            Vec3::new(0.1, 1.0, 0.0),
            false,
            false
        ),
        Vec3::new(1.6, 1.0, 1.5)
    );
}
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
    // Prediction uses the connection-reconstructed catalog, not a global body.
    let local = Catalog::builtins();
    let catalog = crate::content::ContentManifest::from_catalog(&local)
        .resolve_catalog(&local)
        .unwrap();

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
