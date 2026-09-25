//! Step-one behaviour pins for world drops, now on the shared entity path.
//!
//! These tests capture observable drop behaviour. They were ported
//! mechanically from the pre-migration `Drops` API onto entity-backed
//! fixtures: only call syntax changed, every behavioural assertion is
//! unmodified.

use super::*;
use crate::inventory::STACK_LIMIT;
use crate::server::entities::EntityId;
use std::time::Duration;

use super::super::LIFETIME;

#[test]
fn pin_spawn_merges_nearby_and_splits_at_stack_cap() {
    let mut world = drop_world();
    spawn(&mut world, [4.0, 4.0, 4.0], item(2), 300, Duration::ZERO);
    let mut counts: Vec<u16> = nearby(&world, [4.0, 4.0, 4.0])
        .iter()
        .map(|drop| drop.count)
        .collect();
    counts.sort_unstable();
    assert_eq!(counts, vec![44, 128, 128]);
    assert_eq!(
        counts.iter().map(|count| u32::from(*count)).sum::<u32>(),
        300
    );
    assert_store_consistent(&world);
}

#[test]
fn pin_spawn_merging_onto_partial_stack_never_exceeds_cap() {
    let mut world = drop_world();
    insert_entry(
        &mut world,
        1,
        [0.0, 0.0, 0.0],
        2,
        100,
        Duration::ZERO,
        Duration::ZERO,
    );
    spawn(&mut world, [0.0, 0.0, 0.0], item(2), 100, Duration::ZERO);
    let mut counts: Vec<u16> = nearby(&world, [0.0, 0.0, 0.0])
        .iter()
        .map(|drop| drop.count)
        .collect();
    counts.sort_unstable();
    assert_eq!(counts, vec![72, 128]);
    assert!(counts.iter().all(|count| *count <= STACK_LIMIT));
    assert_store_consistent(&world);
}

#[test]
fn pin_spawn_conserves_total_item_count() {
    let mut world = drop_world();
    spawn(&mut world, [0.0, 0.0, 0.0], item(2), 10, Duration::ZERO);
    spawn(&mut world, [50.0, 0.0, 0.0], item(2), 200, Duration::ZERO);
    let total: u32 = nearby(&world, [25.0, 0.0, 0.0])
        .iter()
        .map(|drop| u32::from(drop.count))
        .sum();
    assert_eq!(total, 210);
}

#[test]
fn pin_pickup_delay_gates_candidates_by_server_age() {
    let mut world = drop_world();
    insert_entry(
        &mut world,
        1,
        [0.0, 0.0, 0.0],
        2,
        1,
        Duration::ZERO,
        Duration::from_secs(3_600),
    );
    insert_entry(
        &mut world,
        2,
        [0.5, 0.0, 0.0],
        2,
        1,
        Duration::ZERO,
        Duration::ZERO,
    );
    let candidates = pickup_candidates(&world, [0.0, 0.0, 0.0]);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].id, 2);
    // The delayed drop is still visible to clients; only pickup is gated.
    assert_eq!(nearby(&world, [0.0, 0.0, 0.0]).len(), 2);
}

#[test]
fn pin_expired_drop_is_listed_but_never_pickable_then_removed() {
    let mut world = drop_world();
    insert_entry(
        &mut world,
        1,
        [0.0, 0.0, 0.0],
        2,
        5,
        LIFETIME + Duration::from_secs(60),
        Duration::ZERO,
    );
    // Expired drops never occupy the airborne set and are never pickable,
    // but clients can still see them until the expiry plan applies.
    assert_eq!(active_len(&world), 0);
    assert!(pickup_candidates(&world, [0.0, 0.0, 0.0]).is_empty());
    assert_eq!(nearby(&world, [0.0, 0.0, 0.0]).len(), 1);

    let removed = apply_expired(&mut world, usize::MAX, super::super::unix_ms());
    assert_eq!(removed, 1);
    assert!(stack(&world, EntityId::new(1).unwrap()).is_none());
    assert!(nearby(&world, [0.0, 0.0, 0.0]).is_empty());
    assert_store_consistent(&world);
}

#[test]
fn pin_fresh_drops_are_never_planned_as_expired() {
    let mut world = drop_world();
    spawn(&mut world, [0.0, 0.0, 0.0], item(2), 7, Duration::ZERO);
    assert_eq!(
        apply_expired(&mut world, usize::MAX, super::super::unix_ms()),
        0
    );
}

#[test]
fn pin_take_applies_partial_and_full_removal() {
    let mut world = drop_world();
    insert_entry(
        &mut world,
        1,
        [0.0, 0.0, 0.0],
        2,
        10,
        Duration::ZERO,
        Duration::ZERO,
    );
    take(&mut world, EntityId::new(1).unwrap(), 4);
    assert_eq!(
        stack(&world, EntityId::new(1).unwrap()).map(|stack| stack.count),
        Some(6)
    );
    assert_store_consistent(&world);

    take(&mut world, EntityId::new(1).unwrap(), 6);
    assert!(stack(&world, EntityId::new(1).unwrap()).is_none());
    assert!(nearby(&world, [0.0, 0.0, 0.0]).is_empty());
    assert_store_consistent(&world);
}

#[test]
fn pin_snapshot_round_trip_preserves_state_and_allocator() {
    use crate::server::entities::{decode_checkpoint, encode_checkpoint};
    let root = temp_root("bloxgloom-drop-pin-roundtrip");
    std::fs::create_dir_all(&root).unwrap();
    let stone = crate::items::ItemId::new(crate::world::STONE.get());
    let mut world = drop_world();
    spawn(&mut world, [1.5, 2.5, 3.5], stone, 200, Duration::ZERO);
    spawn(&mut world, [40.0, 2.5, 3.5], stone, 3, Duration::ZERO);
    let before = stable_items(&nearby(&world, [20.0, 2.5, 3.5]));
    assert_eq!(before.len(), 3);
    let checkpoint = crate::server::entities::EntityCheckpointStore::new(&root).unwrap();
    checkpoint
        .write(&encode_checkpoint(&world.store).unwrap())
        .unwrap();

    let reread = checkpoint.read().unwrap().unwrap();
    let restored_store = decode_checkpoint(&reread, Arc::clone(&world.types)).unwrap();
    let restored = DropWorld {
        store: restored_store,
        catalog: Arc::clone(&world.catalog),
        types: Arc::clone(&world.types),
    };
    assert_eq!(stable_items(&nearby(&restored, [20.0, 2.5, 3.5])), before);
    for (_, _, _, position) in &before {
        assert!(position.iter().all(|coordinate| coordinate.is_finite()));
    }
    // The allocator survives the restart: overflow allocates a fresh ID and
    // nothing is reused or lost.
    let used: Vec<u64> = before.iter().map(|entry| entry.0).collect();
    let mut restarted = restored;
    spawn(&mut restarted, [1.5, 2.5, 3.5], stone, 60, Duration::ZERO);
    let after = nearby(&restarted, [1.5, 2.5, 3.5]);
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
    let mut world = drop_world();
    spawn(&mut world, [0.0, 0.0, 0.0], item(2), 1, Duration::ZERO);
    let revision = world.store.revision();
    assert_eq!(world.store.revision(), revision);
    // Observing without a mutation must reproduce the token match.
    let again = world.store.revision();
    assert_eq!(again, revision);

    spawn(&mut world, [9.0, 0.0, 0.0], item(2), 1, Duration::ZERO);
    assert_ne!(world.store.revision(), revision);
    let moved = world.store.revision();
    assert_eq!(world.store.revision(), moved);

    take(&mut world, EntityId::new(1).unwrap(), 1);
    assert_ne!(world.store.revision(), moved);
}
