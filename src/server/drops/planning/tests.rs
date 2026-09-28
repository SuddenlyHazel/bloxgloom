//! Entity-backed drop planning: merge, split, take, and expiry.
use super::*;
use crate::content::Catalog;
use crate::server::entities::{EntityStore, EntityTypeRegistryBuilder};
use std::sync::Arc;
use std::time::Duration;

fn test_store() -> (EntityStore, Arc<Catalog>) {
    let catalog = Arc::new(Catalog::builtins());
    let mut builder = EntityTypeRegistryBuilder::new(&catalog);
    super::super::entity::register_entity_type(&mut builder, Arc::clone(&catalog)).unwrap();
    crate::server::entities::register_player_entity_type(&mut builder).unwrap();
    crate::server::entities::register_kiln_entity_type(&mut builder, Arc::clone(&catalog)).unwrap();
    crate::server::entities::hopper::register(&mut builder, &catalog).unwrap();
    crate::server::entities::chest::register(&mut builder, &catalog).unwrap();
    crate::server::entities::mossbun::register(&mut builder, &catalog).unwrap();
    (
        EntityStore::new(Arc::new(builder.freeze().unwrap())),
        catalog,
    )
}

fn item() -> ItemId {
    ItemId::new(1)
}

fn apply(store: &mut EntityStore, batch: PreparedEntityBatch) {
    store.apply_committed(batch).unwrap();
}

fn spawn(
    store: &mut EntityStore,
    catalog: &Catalog,
    position: [f32; 3],
    count: u16,
    now_ms: u64,
) -> Vec<EntityId> {
    let batch = plan_spawns(
        store,
        catalog,
        &[(position, item(), count, Duration::ZERO)],
        1,
        now_ms,
    )
    .unwrap()
    .expect("nonzero spawn plans work");
    let ids = batch.entity_ids();
    apply(store, batch);
    ids
}

fn total(store: &EntityStore, at: [f32; 3]) -> u32 {
    super::super::queries::nearby(store, at)
        .iter()
        .map(|drop| u32::from(drop.count))
        .sum()
}

fn grand_total(store: &EntityStore) -> u32 {
    // One midpoint query sees both clusters without double counting.
    total(store, [25.0, 0.0, 0.0])
}

#[test]
fn spawn_merges_nearby_and_splits_at_stack_cap() {
    let (mut store, catalog) = test_store();
    spawn(&mut store, &catalog, [4.0, 4.0, 4.0], 300, 1_000);
    let mut counts: Vec<u16> = super::super::queries::nearby(&store, [4.0, 4.0, 4.0])
        .iter()
        .map(|drop| drop.count)
        .collect();
    counts.sort_unstable();
    assert_eq!(counts, vec![44, 128, 128]);
    assert_eq!(total(&store, [4.0, 4.0, 4.0]), 300);
}

#[test]
fn spawn_merging_onto_partial_stack_never_exceeds_cap() {
    let (mut store, catalog) = test_store();
    let first = spawn(&mut store, &catalog, [0.0, 0.0, 0.0], 100, 1_000);
    assert_eq!(first.len(), 1);
    spawn(&mut store, &catalog, [0.0, 0.0, 0.0], 100, 2_000);
    let mut counts: Vec<u16> = super::super::queries::nearby(&store, [0.0, 0.0, 0.0])
        .iter()
        .map(|drop| drop.count)
        .collect();
    counts.sort_unstable();
    assert_eq!(counts, vec![72, 128]);
    assert!(counts.iter().all(|count| *count <= STACK_LIMIT));
    assert_eq!(total(&store, [0.0, 0.0, 0.0]), 200);
}

#[test]
fn production_fill_merges_one_then_splits_remainder_without_loss() {
    let (mut store, catalog) = test_store();
    let first = spawn(&mut store, &catalog, [0.0, 0.0, 0.0], 127, 1_000)[0];
    spawn(&mut store, &catalog, [0.0, 0.0, 0.0], 255, 2_000);
    let mut counts: Vec<_> = super::super::queries::nearby(&store, [0.0, 0.0, 0.0])
        .into_iter()
        .map(|drop| drop.count)
        .collect();
    counts.sort_unstable();
    assert_eq!(counts, vec![126, 128, 128]);
    assert_eq!(
        super::super::queries::stack(&store, first).unwrap().count,
        128
    );
    assert_eq!(total(&store, [0.0, 0.0, 0.0]), 382);
}

#[test]
fn production_merge_picks_oldest_id_until_it_expires() {
    let (mut store, catalog) = test_store();
    let first = spawn(&mut store, &catalog, [0.0, 0.0, 0.0], 10, 1_000)[0];
    let second = spawn(&mut store, &catalog, [1.5, 0.0, 0.0], 10, 2_000)[0];
    spawn(&mut store, &catalog, [0.75, 0.0, 0.0], 1, 2_001);
    assert_eq!(
        super::super::queries::stack(&store, first).unwrap().count,
        11
    );
    assert_eq!(
        super::super::queries::stack(&store, second).unwrap().count,
        10
    );
    let refresh = 2_001 + LIFETIME.as_millis() as u64 / 2;
    spawn(&mut store, &catalog, [1.5, 0.0, 0.0], 1, refresh);
    let at_expiry = 2_001 + LIFETIME.as_millis() as u64;
    spawn(&mut store, &catalog, [0.75, 0.0, 0.0], 1, at_expiry);
    assert_eq!(
        super::super::queries::stack(&store, first).unwrap().count,
        11
    );
    assert_eq!(
        super::super::queries::stack(&store, second).unwrap().count,
        12
    );
    assert_eq!(total(&store, [0.75, 0.0, 0.0]), 23);
}

#[test]
fn take_applies_partial_and_full_removal() {
    let (mut store, catalog) = test_store();
    let ids = spawn(&mut store, &catalog, [0.0, 0.0, 0.0], 10, 1_000);
    let partial = plan_take(&store, &[(ids[0], 4)]).unwrap().unwrap();
    apply(&mut store, partial);
    assert_eq!(
        super::super::queries::stack(&store, ids[0]).map(|stack| stack.count),
        Some(6)
    );
    let full = plan_take(&store, &[(ids[0], 6)]).unwrap().unwrap();
    apply(&mut store, full);
    assert!(super::super::queries::stack(&store, ids[0]).is_none());
    assert!(super::super::queries::nearby(&store, [0.0, 0.0, 0.0]).is_empty());
    assert_eq!(total(&store, [0.0, 0.0, 0.0]), 0);
}

#[test]
fn spawn_and_take_conserve_total_item_count() {
    let (mut store, catalog) = test_store();
    spawn(&mut store, &catalog, [0.0, 0.0, 0.0], 10, 1_000);
    spawn(&mut store, &catalog, [50.0, 0.0, 0.0], 200, 1_000);
    assert_eq!(grand_total(&store), 210);
    let victim = super::super::queries::nearby(&store, [0.0, 0.0, 0.0])[0].id;
    let id = EntityId::new(victim).unwrap();
    let take = plan_take(&store, &[(id, 4)]).unwrap().unwrap();
    apply(&mut store, take);
    assert_eq!(grand_total(&store), 206);
}

#[test]
fn expired_drops_plan_bounded_despawns() {
    let (mut store, catalog) = test_store();
    spawn(&mut store, &catalog, [0.0, 0.0, 0.0], 7, 1_000);
    assert!(plan_expired(&store, 2_000, usize::MAX).unwrap().is_none());
    let batch = plan_expired(&store, 1_000 + LIFETIME.as_millis() as u64 + 1, usize::MAX)
        .unwrap()
        .expect("expired drop plans removal");
    assert_eq!(batch.entity_ids().len(), 1);
    apply(&mut store, batch);
    assert!(super::super::queries::nearby(&store, [0.0, 0.0, 0.0]).is_empty());
}

#[test]
fn invalid_spawns_fail_closed_without_staging() {
    let (store, catalog) = test_store();
    assert!(
        plan_spawns(
            &store,
            &catalog,
            &[([f32::NAN, 0.0, 0.0], item(), 1, Duration::ZERO)],
            1,
            1_000
        )
        .is_err()
    );
    assert!(
        plan_spawns(
            &store,
            &catalog,
            &[([0.0, 0.0, 0.0], ItemId::new(u32::MAX), 1, Duration::ZERO)],
            1,
            1_000
        )
        .is_err()
    );
    assert_eq!(store.len(), 0);
}
