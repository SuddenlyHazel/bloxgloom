use super::*;
use crate::world::TALL_GRASS;
use std::collections::HashMap;

fn raycast_blocks(
    origin: Vec3,
    direction: Vec3,
    reach: f32,
    blocks: &HashMap<[i32; 3], BlockId>,
) -> Option<Hit> {
    raycast(origin, direction, reach, |x, y, z| {
        Some(*blocks.get(&[x, y, z]).unwrap_or(&AIR))
    })
}

#[test]
fn reports_target_face_and_adjacent_cell() {
    let blocks = HashMap::from([([3, 2, 1], BlockId::new(2))]);
    let hit = raycast_blocks(Vec3::new(0.5, 2.5, 1.5), Vec3::X, 8.0, &blocks).unwrap();
    assert_eq!(hit.block, [3, 2, 1]);
    assert_eq!(hit.adjacent, [2, 2, 1]);
    assert_eq!(hit.face, Face::NegX);
    assert_eq!(hit.block_id, BlockId::new(2));
    assert!((hit.distance - 2.5).abs() < 1.0e-6);
}

#[test]
fn traverses_negative_coordinates_and_negative_faces() {
    let blocks = HashMap::from([([-3, 0, 0], BlockId::new(3))]);
    let hit = raycast_blocks(Vec3::new(-0.5, 0.5, 0.5), -Vec3::X, 8.0, &blocks).unwrap();
    assert_eq!(hit.block, [-3, 0, 0]);
    assert_eq!(hit.adjacent, [-2, 0, 0]);
    assert_eq!(hit.face, Face::PosX);
    assert!((hit.distance - 1.5).abs() < 1.0e-6);
}

#[test]
fn integer_plane_moving_negative_starts_in_the_entered_voxel() {
    let blocks = HashMap::from([([-1, 0, 0], BlockId::new(1)), ([0, 0, 0], BlockId::new(2))]);
    let hit = raycast_blocks(Vec3::new(0.0, 0.5, 0.5), -Vec3::X, 1.0, &blocks).unwrap();
    assert_eq!(hit.block, [-1, 0, 0]);
    assert_eq!(hit.adjacent, [0, 0, 0]);
    assert_eq!(hit.face, Face::PosX);
    assert_eq!(hit.distance, 0.0);
}

#[test]
fn parallel_axis_uses_half_open_boundary_ownership() {
    let blocks = HashMap::from([([0, 0, 0], BlockId::new(1)), ([-1, 0, 0], BlockId::new(2))]);
    let hit = raycast_blocks(Vec3::new(0.0, 0.5, 0.5), Vec3::Y, 1.0, &blocks).unwrap();
    assert_eq!(hit.block, [0, 0, 0]);
}

#[test]
fn simultaneous_corner_crossing_advances_all_axes() {
    let blocks = HashMap::from([([1, 0, 0], BlockId::new(1)), ([1, 1, 0], BlockId::new(2))]);
    let hit = raycast_blocks(
        Vec3::new(0.5, 0.5, 0.5),
        Vec3::new(1.0, 1.0, 0.0),
        2.0,
        &blocks,
    )
    .unwrap();
    assert_eq!(hit.block, [1, 1, 0]);
    assert_eq!(hit.face, Face::NegX);
    assert_eq!(hit.adjacent, [0, 1, 0]);
}

#[test]
fn clamps_reach_and_rejects_invalid_rays() {
    let blocks = HashMap::from([([9, 0, 0], BlockId::new(1))]);
    assert!(raycast_blocks(Vec3::new(0.5, 0.5, 0.5), Vec3::X, 100.0, &blocks).is_none());
    assert!(raycast(Vec3::ZERO, Vec3::ZERO, 8.0, |_, _, _| Some(BlockId::new(1))).is_none());
    assert!(
        raycast(Vec3::ZERO, Vec3::X, f32::NAN, |_, _, _| Some(BlockId::new(
            1
        )))
        .is_none()
    );
}

#[test]
fn aiming_past_grass_edges_reaches_ground_but_center_hits_flower() {
    let blocks = HashMap::from([
        ([1, 0, 0], TALL_GRASS),
        ([2, 0, 0], BlockId::new(2)),
        ([1, 0, 1], crate::world::RED_FLOWER),
    ]);
    let past_grass = raycast_blocks(Vec3::new(0.5, 0.5, 0.1), Vec3::X, 8.0, &blocks).unwrap();
    assert_eq!(past_grass.block, [2, 0, 0]);
    let flower = raycast_blocks(Vec3::new(0.5, 0.5, 1.5), Vec3::X, 8.0, &blocks).unwrap();
    assert_eq!(flower.block, [1, 0, 1]);
    assert!(flower.distance > 0.5);
}

#[test]
fn water_is_transparent_to_edit_targeting_from_above_and_underwater() {
    let blocks = HashMap::from([
        ([0, 2, 0], crate::world::WATER),
        ([0, 1, 0], crate::world::WATER),
        ([0, 0, 0], crate::world::STONE),
    ]);
    for y in [3.5, 1.5] {
        let hit = raycast_blocks(Vec3::new(0.5, y, 0.5), -Vec3::Y, 8.0, &blocks).unwrap();
        assert_eq!(hit.block, [0, 0, 0]);
    }
}
