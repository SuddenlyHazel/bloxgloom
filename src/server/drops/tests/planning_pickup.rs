use super::*;
use std::time::Duration;

#[test]
fn nearby_and_pickup_keep_exact_sort_order_and_256_item_limit() {
    let mut drops = Drops::new();
    for id in (1..=300).rev() {
        insert_entry(
            &mut drops,
            id,
            [0.0, 0.0, 0.0],
            2,
            1,
            Duration::ZERO,
            Duration::ZERO,
        );
    }
    assert_eq!(
        drops
            .pickup_candidates([0.0, 0.0, 0.0])
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>(),
        (1..=256).collect::<Vec<_>>()
    );
    assert_eq!(
        drops
            .nearby([0.0, 0.0, 0.0])
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>(),
        (1..=256).collect::<Vec<_>>()
    );
}

#[test]
fn pickup_and_nearby_keep_radius_and_age_filters() {
    let mut drops = Drops::new();
    insert_entry(
        &mut drops,
        1,
        [2.25, 0.0, 0.0],
        2,
        1,
        Duration::ZERO,
        Duration::ZERO,
    );
    insert_entry(
        &mut drops,
        2,
        [2.2501, 0.0, 0.0],
        2,
        1,
        Duration::ZERO,
        Duration::ZERO,
    );
    insert_entry(
        &mut drops,
        3,
        [0.0, 0.0, 0.0],
        2,
        1,
        Duration::ZERO,
        Duration::from_secs(3_600),
    );
    insert_entry(
        &mut drops,
        4,
        [0.0, 0.0, 0.0],
        2,
        1,
        LIFETIME + Duration::from_secs(1),
        Duration::ZERO,
    );
    insert_entry(
        &mut drops,
        5,
        [64.0, 0.0, 0.0],
        2,
        1,
        Duration::ZERO,
        Duration::ZERO,
    );
    insert_entry(
        &mut drops,
        6,
        [64.001, 0.0, 0.0],
        2,
        1,
        Duration::ZERO,
        Duration::ZERO,
    );

    assert_eq!(
        drops
            .pickup_candidates([0.0, 0.0, 0.0])
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>(),
        vec![1]
    );
    assert_eq!(
        drops
            .nearby([0.0, 0.0, 0.0])
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>(),
        vec![3, 4, 1, 2, 5]
    );
}

#[test]
fn durable_spawn_merges_local_targets_in_id_order_and_tracks_new_stacks() {
    let mut drops = Drops::new();
    insert_entry(
        &mut drops,
        10,
        [-15.5, 4.0, -0.1],
        2,
        127,
        Duration::ZERO,
        Duration::ZERO,
    );
    insert_entry(
        &mut drops,
        2,
        [-16.5, 4.0, -0.1],
        2,
        127,
        Duration::ZERO,
        Duration::ZERO,
    );
    let plan = drops
        .plan_spawn([-16.0, 4.0, -0.1], item(2), 2, Duration::from_millis(25))
        .unwrap();
    assert_eq!(
        plan.changes
            .iter()
            .map(|change| change.id)
            .collect::<Vec<_>>(),
        vec![2, 10]
    );
    assert_eq!(
        journal::decode_owner(2, &plan.changes[0].after).unwrap().1,
        STACK_LIMIT
    );
    assert_eq!(
        journal::decode_owner(10, &plan.changes[1].after).unwrap().1,
        STACK_LIMIT
    );

    let mut empty = Drops::new();
    let plan = empty
        .plan_spawns(&[
            ([-0.1, 0.0, 0.0], item(2), 120, Duration::ZERO),
            ([-0.1, 0.0, 0.0], item(2), 20, Duration::ZERO),
        ])
        .unwrap();
    assert_eq!(plan.changes.len(), 2);
    assert_eq!(plan.changes[0].id, 1);
    assert_eq!(plan.changes[1].id, 2);
    assert_eq!(
        journal::decode_owner(1, &plan.changes[0].after).unwrap().1,
        STACK_LIMIT
    );
    assert_eq!(
        journal::decode_owner(2, &plan.changes[1].after).unwrap().1,
        12
    );
    empty.apply_plan(&plan).unwrap();
    assert_spatial_members_match_entries(&empty);
    assert_eq!(empty.nearby([-0.1, 0.0, 0.0]).len(), 2);
}

#[test]
fn expiry_index_refreshes_on_merge_and_drains_backlog_in_bounded_batches() {
    let mut drops = Drops::new();
    insert_entry(
        &mut drops,
        1,
        [-0.25, 4.0, 0.0],
        2,
        100,
        LIFETIME - Duration::from_secs(2),
        Duration::ZERO,
    );
    let refresh = drops
        .plan_spawn([-0.25, 4.0, 0.0], item(2), 1, Duration::ZERO)
        .unwrap();
    drops.apply_plan(&refresh).unwrap();
    assert!(drops.plan_expired(1).changes.is_empty());

    for id in 2..=601 {
        insert_entry(
            &mut drops,
            id,
            [id as f32 * 40.0, 0.0, 0.0],
            2,
            1,
            LIFETIME + Duration::from_secs(1),
            Duration::ZERO,
        );
    }
    let first = drops.plan_expired(256);
    assert_eq!(first.changes.len(), 256);
    assert_eq!(
        first
            .changes
            .iter()
            .map(|change| change.id)
            .collect::<Vec<_>>(),
        (2..=257).collect::<Vec<_>>()
    );
    drops.apply_plan(&first).unwrap();
    let second = drops.plan_expired(256);
    assert_eq!(second.changes.len(), 256);
    assert_eq!(
        second
            .changes
            .iter()
            .map(|change| change.id)
            .collect::<Vec<_>>(),
        (258..=513).collect::<Vec<_>>()
    );
    drops.apply_plan(&second).unwrap();
    let remaining = drops.plan_expired(256);
    assert_eq!(remaining.changes.len(), 88);
    drops.apply_plan(&remaining).unwrap();
    assert!(drops.plan_expired(256).changes.is_empty());
    assert!(drops.entries.contains_key(&1));
    assert_spatial_members_match_entries(&drops);
}

#[test]
fn durable_count_change_does_not_wake_a_settled_drop() {
    let mut drops = Drops::new();
    insert_entry(
        &mut drops,
        1,
        [-0.25, 4.0, 0.0],
        2,
        100,
        Duration::ZERO,
        Duration::ZERO,
    );
    drops.active.clear();

    let partial_take = drops.plan_take(&[(1, 1)]).unwrap();
    drops.apply_plan(&partial_take).unwrap();
    assert!(!drops.active.contains(&1));

    let merge = drops
        .plan_spawn([-0.25, 4.0, 0.0], item(2), 1, Duration::ZERO)
        .unwrap();
    drops.apply_plan(&merge).unwrap();
    assert!(!drops.active.contains(&1));
    assert_spatial_members_match_entries(&drops);
}
