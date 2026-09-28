use super::*;

#[test]
fn movement_samples_and_placement_bounds_agree_at_block_edges() {
    let feet = [0.5, 1.0, 0.5];
    for block in [[0, 1, 0], [0, 2, 0]] {
        assert!(BUILTIN_BODY.intersects_block(block, feet));
        assert!(
            BUILTIN_BODY
                .collides(feet, |x, y, z| Ok::<_, ()>([x, y, z] == block))
                .unwrap()
        );
    }
    assert!(!BUILTIN_BODY.intersects_block([1, 2, 0], feet));
    assert!(
        !BUILTIN_BODY
            .collides(feet, |x, y, z| Ok::<_, ()>([x, y, z] == [1, 2, 0]))
            .unwrap()
    );
}

#[test]
fn unavailable_voxel_stops_collision_instead_of_becoming_empty() {
    let feet = [0.5, 1.0, 0.5];
    assert_eq!(
        BUILTIN_BODY.collides(feet, |_, _, _| Err::<bool, _>(
            "missing authoritative voxel"
        )),
        Err("missing authoritative voxel")
    );
    assert!(!BUILTIN_BODY.intersects_block([0, 3, 0], feet));
    assert!(!BUILTIN_BODY.intersects_block([1, 1, 0], feet));
}
