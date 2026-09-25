//! Step-one behaviour pins for world drops.
//!
//! These tests capture current observable drop behaviour before the migration
//! onto the shared entity path. They must pass UNMODIFIED after the
//! migration: if one has to change, that is a behaviour change.

use super::*;
use std::time::Duration;

#[test]
fn pin_spawn_merges_nearby_and_splits_at_stack_cap() {
    let mut drops = Drops::new();
    drops.spawn([4.0, 4.0, 4.0], item(2), 300, Duration::ZERO);
    let mut counts: Vec<u16> = drops
        .nearby([4.0, 4.0, 4.0])
        .iter()
        .map(|drop| drop.count)
        .collect();
    counts.sort_unstable();
    assert_eq!(counts, vec![44, 128, 128]);
    assert_eq!(
        counts.iter().map(|count| u32::from(*count)).sum::<u32>(),
        300
    );
    assert_spatial_members_match_entries(&drops);
}

#[test]
fn pin_spawn_merging_onto_partial_stack_never_exceeds_cap() {
    let mut drops = Drops::new();
    insert_entry(
        &mut drops,
        1,
        [0.0, 0.0, 0.0],
        2,
        100,
        Duration::ZERO,
        Duration::ZERO,
    );
    drops.spawn([0.0, 0.0, 0.0], item(2), 100, Duration::ZERO);
    let mut counts: Vec<u16> = drops
        .nearby([0.0, 0.0, 0.0])
        .iter()
        .map(|drop| drop.count)
        .collect();
    counts.sort_unstable();
    assert_eq!(counts, vec![72, 128]);
    assert!(counts.iter().all(|count| *count <= STACK_LIMIT));
    assert_spatial_members_match_entries(&drops);
}

#[test]
fn pin_spawn_conserves_total_item_count() {
    let mut drops = Drops::new();
    drops.spawn([0.0, 0.0, 0.0], item(2), 10, Duration::ZERO);
    drops.spawn([50.0, 0.0, 0.0], item(2), 200, Duration::ZERO);
    let total: u32 = drops
        .nearby([25.0, 0.0, 0.0])
        .iter()
        .map(|drop| u32::from(drop.count))
        .sum();
    assert_eq!(total, 210);
}

#[test]
fn pin_pickup_delay_gates_candidates_by_server_age() {
    let mut drops = Drops::new();
    insert_entry(
        &mut drops,
        1,
        [0.0, 0.0, 0.0],
        2,
        1,
        Duration::ZERO,
        Duration::from_secs(3_600),
    );
    insert_entry(
        &mut drops,
        2,
        [0.5, 0.0, 0.0],
        2,
        1,
        Duration::ZERO,
        Duration::ZERO,
    );
    let candidates = drops.pickup_candidates([0.0, 0.0, 0.0]);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].id, 2);
    // The delayed drop is still visible to clients; only pickup is gated.
    assert_eq!(drops.nearby([0.0, 0.0, 0.0]).len(), 2);
}

#[test]
fn pin_expired_drop_is_listed_but_never_pickable_then_removed() {
    let mut drops = Drops::new();
    insert_entry(
        &mut drops,
        1,
        [0.0, 0.0, 0.0],
        2,
        5,
        LIFETIME + Duration::from_secs(60),
        Duration::ZERO,
    );
    // Expired drops never occupy the airborne set and are never pickable,
    // but clients can still see them until the expiry plan applies.
    assert_eq!(drops.active_len(), 0);
    assert!(drops.pickup_candidates([0.0, 0.0, 0.0]).is_empty());
    assert_eq!(drops.nearby([0.0, 0.0, 0.0]).len(), 1);

    let plan = drops.plan_expired(usize::MAX);
    assert_eq!(plan.changes.len(), 1);
    assert_eq!(plan.changes[0].id, 1);
    assert!(plan.changes[0].after.is_empty());
    drops.apply_plan(&plan).unwrap();
    assert!(drops.nearby([0.0, 0.0, 0.0]).is_empty());
    assert_spatial_members_match_entries(&drops);
}

#[test]
fn pin_fresh_drops_are_never_planned_as_expired() {
    let mut drops = Drops::new();
    drops.spawn([0.0, 0.0, 0.0], item(2), 7, Duration::ZERO);
    assert!(drops.plan_expired(usize::MAX).changes.is_empty());
}

#[test]
fn pin_take_applies_partial_and_full_removal() {
    let mut drops = Drops::new();
    insert_entry(
        &mut drops,
        1,
        [0.0, 0.0, 0.0],
        2,
        10,
        Duration::ZERO,
        Duration::ZERO,
    );
    let partial = drops.plan_take(&[(1, 4)]).unwrap();
    drops.apply_plan(&partial).unwrap();
    assert_eq!(drops.stack(1).map(|stack| stack.count), Some(6));
    assert_spatial_members_match_entries(&drops);

    let full = drops.plan_take(&[(1, 6)]).unwrap();
    drops.apply_plan(&full).unwrap();
    assert!(drops.stack(1).is_none());
    assert!(drops.nearby([0.0, 0.0, 0.0]).is_empty());
    assert_spatial_members_match_entries(&drops);
}

#[test]
fn pin_snapshot_round_trip_preserves_state_and_allocator() {
    let root = temp_root("bloxgloom-drop-pin-roundtrip");
    std::fs::create_dir_all(&root).unwrap();
    let stone = crate::items::ItemId::new(crate::world::STONE.get());
    let mut drops = Drops::open(&root).unwrap();
    drops.spawn([1.5, 2.5, 3.5], stone, 200, Duration::ZERO);
    drops.spawn([40.0, 2.5, 3.5], stone, 3, Duration::ZERO);
    let before = stable_items(&drops.nearby([20.0, 2.5, 3.5]));
    assert_eq!(before.len(), 3);
    drops.save().unwrap();

    let mut restored = Drops::open(&root).unwrap();
    assert_eq!(stable_items(&restored.nearby([20.0, 2.5, 3.5])), before);
    for (_, _, _, position) in &before {
        assert!(position.iter().all(|coordinate| coordinate.is_finite()));
    }
    // The allocator survives the restart: overflow allocates a fresh ID and
    // nothing is reused or lost.
    let used: Vec<u64> = before.iter().map(|entry| entry.0).collect();
    restored.spawn([1.5, 2.5, 3.5], stone, 60, Duration::ZERO);
    let after = restored.nearby([1.5, 2.5, 3.5]);
    assert!(after.iter().any(|drop| !used.contains(&drop.id)));
    assert_eq!(
        after
            .iter()
            .filter(|drop| drop.position == [1.5, 2.5, 3.5])
            .map(|drop| u32::from(drop.count))
            .sum::<u32>(),
        260
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn pin_checkpoint_generation_token_changes_only_on_mutation() {
    let mut drops = Drops::new();
    drops.spawn([0.0, 0.0, 0.0], item(2), 1, Duration::ZERO);
    let bytes = drops.snapshot_bytes().unwrap();
    assert!(drops.matches_checkpoint_generation(&bytes));
    // Reserializing without a mutation must reproduce the token match.
    let again = drops.snapshot_bytes().unwrap();
    assert!(drops.matches_checkpoint_generation(&again));

    drops.spawn([9.0, 0.0, 0.0], item(2), 1, Duration::ZERO);
    assert!(!drops.matches_checkpoint_generation(&bytes));
    let moved = drops.snapshot_bytes().unwrap();
    assert!(drops.matches_checkpoint_generation(&moved));

    let take = drops.plan_take(&[(1, 1)]).unwrap();
    drops.apply_plan(&take).unwrap();
    assert!(!drops.matches_checkpoint_generation(&moved));
}
