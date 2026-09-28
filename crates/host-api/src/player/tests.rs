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

#[test]
fn spawn_search_preserves_startup_highest_and_live_up_then_down_order() {
    let policy = BUILTIN_SPAWN;
    assert_eq!(policy.ceiling(120), 152);
    let startup = policy.startup_support_levels(-3, 0).collect::<Vec<_>>();
    assert_eq!(startup.first(), Some(&31));
    assert_eq!(startup.last(), Some(&-3));
    let live = policy.cached_feet_levels(2, -3).collect::<Vec<_>>();
    assert_eq!(&live[..3], &[2, 3, 4]);
    assert_eq!(&live[live.len() - 4..], &[1, 0, -1, -2]);
    assert_eq!(policy.feet(-2), [0.5, -2.0, 0.5]);
}

#[test]
fn builtin_input_rate_stays_below_authoritative_budget() {
    assert_eq!(BUILTIN_MOTION.intent_blocks_per_second, 8.0);
    assert_eq!(BUILTIN_MOTION.budget_blocks_per_second, 10.0);
    assert!(
        f64::from(BUILTIN_MOTION.intent_blocks_per_second)
            < BUILTIN_MOTION.budget_blocks_per_second
    );
}
