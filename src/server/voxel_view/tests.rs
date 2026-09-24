use super::*;
use crate::world::{BEDROCK_Y, STONE, world_to_chunk};
use std::sync::Arc;

#[test]
fn movement_stops_at_wall_without_tunneling() {
    let mut chunk = air_chunk(key(0, 0, 0), 3);
    set_block(&mut chunk, [2, 1, 0], STONE);
    let view = VoxelView::from_chunks([chunk]).unwrap();

    let moved = resolve_player_movement(&view, [0.5, 1.0, 0.5], [3.0, 0.0, 0.0]).unwrap();

    assert_eq!(moved, [1.5, 1.0, 0.5]);
    assert!(player_collides(&view, [1.75, 1.0, 0.5]).unwrap());
}

#[test]
fn negative_coordinates_use_the_correct_chunk_at_seam() {
    let mut west = air_chunk(key(-1, 0, 0), 11);
    let east = air_chunk(key(0, 0, 0), 7);
    set_block(&mut west, [15, 1, 0], STONE);
    let view = VoxelView::from_chunks([Arc::new(east), Arc::new(west)]).unwrap();

    assert_eq!(world_to_chunk(-1, 1, 0), (key(-1, 0, 0), [15, 1, 0]));
    assert_eq!(view.block(-1, 1, 0), Ok(STONE));
    assert_eq!(view.block(0, 1, 0), Ok(AIR));
    let moved = resolve_player_movement(&view, [-1.5, 1.0, 0.5], [1.0, 0.0, 0.0]).unwrap();
    assert_eq!(moved, [-1.5, 1.0, 0.5]);
}

#[test]
fn unavailable_collision_chunk_rejects_entire_movement_result() {
    let view = VoxelView::from_chunks([air_chunk(key(0, 0, 0), 1)]).unwrap();

    let error = resolve_player_movement(&view, [15.5, 1.0, 0.5], [1.0, 0.0, 0.0]).unwrap_err();

    assert_eq!(
        error,
        MovementError::MissingChunk(MissingChunk { key: key(1, 0, 0) })
    );
}

#[test]
fn construction_and_movement_are_deterministic_independent_of_input_order() {
    let mut first = air_chunk(key(-1, 0, 0), 4);
    set_block(&mut first, [15, 1, 0], STONE);
    let second = air_chunk(key(0, 0, 0), 8);
    let forward = VoxelView::from_chunks([first.clone(), second.clone()]).unwrap();
    let reverse = VoxelView::from_chunks([second, first]).unwrap();

    assert_eq!(forward.revisions(), reverse.revisions());
    assert_eq!(
        resolve_player_movement(&forward, [-1.5, 1.0, 0.5], [-0.8, 0.0, 0.0]),
        resolve_player_movement(&reverse, [-1.5, 1.0, 0.5], [-0.8, 0.0, 0.0]),
    );
}

#[test]
fn revision_metadata_detects_stale_and_missing_chunks() {
    let view =
        VoxelView::from_chunks([air_chunk(key(-1, 0, 0), 4), air_chunk(key(0, 0, 0), 8)]).unwrap();

    assert!(view.contains_chunk(key(-1, 0, 0)));
    assert!(view.contains_chunk(key(0, 0, 0)));
    assert!(!view.contains_chunk(key(1, 0, 0)));
    assert_eq!(view.revisions(), &[(key(-1, 0, 0), 4), (key(0, 0, 0), 8)]);
    assert!(view.revisions_match(|chunk| match chunk.x {
        -1 => Some(4),
        0 => Some(8),
        _ => None,
    }));
    assert!(!view.revisions_match(|chunk| match chunk.x {
        -1 => Some(5),
        0 => Some(8),
        _ => None,
    }));
    assert!(!view.revisions_match(|chunk| (chunk.x == -1).then_some(4)));
}

#[test]
fn block_lookup_never_fills_missing_chunks_with_terrain_or_bedrock() {
    let view = VoxelView::from_chunks([air_chunk(key(0, 0, 0), 0)]).unwrap();
    let missing = view.block(-1, BEDROCK_Y, 0).unwrap_err();

    assert_eq!(
        missing,
        MissingChunk {
            key: key(-1, -4, 0)
        }
    );
}

#[test]
fn snapshot_rejects_unknown_block_ids_instead_of_treating_them_as_air() {
    let mut chunk = air_chunk(key(2, 0, -1), 0);
    chunk.blocks[17] = u8::MAX;

    let error = VoxelView::from_chunks([chunk]).unwrap_err();

    assert_eq!(
        error,
        SnapshotError::InvalidBlock {
            key: key(2, 0, -1),
            index: 17,
            block: u8::MAX,
        }
    );
}

#[test]
fn resident_view_shares_the_authoritative_chunk_allocation() {
    let chunk = Arc::new(air_chunk(key(3, 2, -1), 12));
    let view = VoxelView::from_resident_chunks([Arc::clone(&chunk)]).unwrap();

    assert!(Arc::ptr_eq(&chunk, &view.chunks[&chunk.key]));
    assert_eq!(view.revisions(), &[(chunk.key, 12)]);
    assert_eq!(
        VoxelView::from_resident_chunks([Arc::clone(&chunk), chunk]).unwrap_err(),
        SnapshotError::DuplicateChunk { key: key(3, 2, -1) }
    );
}

#[test]
fn movement_rejects_inputs_that_exceed_the_bounded_step_budget() {
    let view = VoxelView::from_chunks([air_chunk(key(0, 0, 0), 0)]).unwrap();

    assert_eq!(
        resolve_player_movement(&view, [0.5, 1.0, 0.5], [2048.25, 0.0, 0.0]),
        Err(MovementError::OutOfBounds)
    );
}
