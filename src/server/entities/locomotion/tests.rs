use super::*;
use crate::world::{AIR, CHUNK_VOLUME, Chunk, ChunkKey, STONE};

pub(in crate::server::entities) fn view(
    blocks: &[(i32, i32, i32)],
    holes: &[(i32, i32)],
) -> VoxelView {
    let mut chunks = Vec::new();
    for x in -1..=1 {
        for y in 4..=6 {
            for z in -1..=1 {
                let key = ChunkKey { x, y, z };
                let mut chunk = Chunk::from_blocks(key, 1, vec![AIR; CHUNK_VOLUME]);
                for lx in 0..16 {
                    for lz in 0..16 {
                        if y == 4 && !holes.contains(&(x * 16 + lx as i32, z * 16 + lz as i32)) {
                            chunk.blocks.set(Chunk::index([lx, 15, lz]).unwrap(), STONE);
                        }
                    }
                }
                for &(bx, by, bz) in blocks {
                    let (owner, local) = crate::world::world_to_chunk(bx, by, bz);
                    if owner == key {
                        chunk.blocks.set(Chunk::index(local).unwrap(), STONE);
                    }
                }
                chunks.push(chunk);
            }
        }
    }
    VoxelView::from_chunks(chunks).unwrap()
}

const BODY: Body = Body {
    half_width: 0.36,
    height: 0.94,
    speed: 1.5625,
};

#[test]
fn gravity_accelerates_and_sweeps_to_exact_landing_without_tunneling() {
    let view = view(&[], &[]);
    let first = BODY.advance(&view, [0.5, 85.0, 0.5], 0.0, None).unwrap();
    let second = BODY
        .advance(&view, first.position, first.vertical_velocity, None)
        .unwrap();
    assert!(first.position[1] - second.position[1] > 85.0 - first.position[1]);
    let mut moving = second;
    for _ in 0..100 {
        moving = BODY
            .advance(&view, moving.position, moving.vertical_velocity, None)
            .unwrap();
        assert!(moving.position[1] >= 80.0);
    }
    assert_eq!(moving.position, [0.5, 80.0, 0.5]);
    assert!(moving.grounded);
    assert_eq!(moving.vertical_velocity, 0.0);
    let fast = BODY.advance(&view, [0.5, 80.1, 0.5], -24.0, None).unwrap();
    assert_eq!(fast.position[1], 80.0);
}

#[test]
fn ground_motion_respects_walls_cliffs_seams_and_embedded_edits() {
    let flat = view(&[], &[]);
    let start = [15.99, 80.0, 0.5];
    assert!(
        BODY.advance(&flat, start, 0.0, Some([16.5, 80.0, 0.5]))
            .unwrap()
            .position[0]
            > 16.0
    );
    let wall = view(&[(16, 80, 0)], &[]);
    let start = [15.6, 80.0, 0.5];
    assert_eq!(
        BODY.advance(&wall, start, 0.0, Some([16.5, 80.0, 0.5]))
            .unwrap()
            .position,
        start
    );
    let cliff = view(&[], &[(16, 0)]);
    assert_eq!(
        BODY.advance(&cliff, start, 0.0, Some([16.5, 80.0, 0.5]))
            .unwrap()
            .position,
        start
    );
    assert_eq!(
        BODY.advance(&wall, [16.5, 80.0, 0.5], 0.0, None)
            .unwrap()
            .position,
        [16.5, 80.0, 0.5]
    );
}

#[test]
fn wide_and_tall_bodies_check_interior_voxels_not_only_corners() {
    let body = Body {
        half_width: 1.4,
        height: 3.0,
        speed: 1.0,
    };
    assert!(
        !body
            .clear(&view(&[(8, 81, 8)], &[]), [8.5, 80.0, 8.5])
            .unwrap()
    );
    assert!(
        !body
            .supported(&view(&[], &[(8, 8)]), [8.5, 80.0, 8.5])
            .unwrap()
    );
}
