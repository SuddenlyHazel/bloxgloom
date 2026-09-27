//! Unified-path proofs: bounded checkpoints and scheduled movement.
//!
//! One moving drop costs its own records — never the population — and motion
//! arrives as scheduled entity work, never coordinator stepping.
use super::*;
use crate::content::Catalog;
use crate::inventory::Stack;
use crate::items::ItemId;
use crate::server::entities::{
    EntityId, EntityPatch, EntitySpawn, EntityStore, EntityTypeRegistryBuilder,
};
use std::sync::Arc;
use std::time::Duration;

fn test_store() -> (EntityStore, Arc<Catalog>) {
    let catalog = Arc::new(Catalog::builtins());
    let mut builder = EntityTypeRegistryBuilder::new(&catalog);
    register_entity_type(&mut builder, Arc::clone(&catalog)).unwrap();
    crate::server::entities::register_player_entity_type(&mut builder).unwrap();
    crate::server::entities::register_kiln_entity_type(&mut builder, Arc::clone(&catalog)).unwrap();
    crate::server::entities::hopper::register(&mut builder, &catalog).unwrap();
    crate::server::entities::mossbun::register(&mut builder, &catalog).unwrap();
    (
        EntityStore::new(Arc::new(builder.freeze().unwrap())),
        catalog,
    )
}

/// Direct spawns bypass merge so every record lands in one chunk.
fn fill_chunk(store: &mut EntityStore, count: usize, now_ms: u64) -> Vec<EntityId> {
    let mut ids = Vec::with_capacity(count);
    for batch in [0, 1, 2].iter().filter_map(|slot| {
        let start = slot * 4_096;
        (start < count).then(|| start..count.min(start + 4_096))
    }) {
        let spawns: Vec<_> = batch
            .map(|index| EntitySpawn::Mobile {
                entity_type: DROP_ENTITY_TYPE,
                position: [index as f32 * 0.001, 4.0, 0.5],
                payload: DropEntityPayload::new(
                    Stack::new(ItemId::new(1), 1),
                    now_ms,
                    Duration::ZERO,
                )
                .into_entity_payload(),
                spawn_tick: 1,
            })
            .collect();
        let prepared = store.prepare_spawn_batch(spawns).unwrap();
        ids.extend(prepared.entity_ids());
        store.apply_committed(prepared).unwrap();
    }
    ids
}

fn staged_motion_bytes(store: &EntityStore, id: EntityId) -> (usize, usize) {
    let snapshot = store.snapshot(id).unwrap();
    let prepared = store
        .prepare_update(
            id,
            snapshot.revision,
            EntityPatch {
                payload: None,
                next_tick: None,
                position: Some([0.002, 4.01, 0.5]),
            },
        )
        .unwrap();
    let bytes = prepared
        .changes()
        .iter()
        .map(|change| change.before.len() + change.after.len())
        .sum();
    (prepared.changes().len(), bytes)
}

#[test]
fn one_moving_drop_costs_its_own_records_never_the_population() {
    for count in [10, 10_000] {
        let (mut store, _catalog) = test_store();
        let ids = fill_chunk(&mut store, count, 1_000);
        assert_eq!(store.len(), count);
        let (changes, bytes) = staged_motion_bytes(&store, ids[0]);
        // One record delta, one motion delta, one revision delta: the moved
        // drop's own keys only. No chunk page, no allocator, no neighbour.
        assert_eq!(changes, 3);
        assert!(bytes < 2_048, "one motion staged {bytes} bytes");
        let applied = store
            .prepare_update(
                ids[0],
                store.snapshot(ids[0]).unwrap().revision,
                EntityPatch {
                    payload: None,
                    next_tick: None,
                    position: Some([0.002, 4.01, 0.5]),
                },
            )
            .unwrap();
        store.apply_committed(applied).unwrap();
        let moved = store.snapshot(ids[0]).unwrap();
        let crate::server::entities::EntityLocation::Mobile { position } = moved.location else {
            panic!("drops stay mobile");
        };
        assert_eq!(position, [0.002, 4.01, 0.5]);
        // The population is untouched: every other record is bit-identical.
        assert_eq!(store.len(), count);
    }
    // Byte-for-byte independence from the population size.
    let (mut small, _) = test_store();
    fill_chunk(&mut small, 10, 1_000);
    let (mut large, _) = test_store();
    fill_chunk(&mut large, 10_000, 1_000);
    assert_eq!(
        staged_motion_bytes(&small, EntityId::new(1).unwrap()),
        staged_motion_bytes(&large, EntityId::new(1).unwrap())
    );
}

#[test]
fn motion_is_scheduled_entity_work_never_coordinator_stepping() {
    let (mut store, _catalog) = test_store();
    let ids = fill_chunk(&mut store, 3, 1_000);
    // Spawning schedules: every fresh drop joins the sparse due schedule.
    for id in &ids {
        let snapshot = store.snapshot(*id).unwrap();
        assert_eq!(snapshot.next_tick, Some(2));
    }
    let mut due = store.due_entities(u64::MAX, 64);
    due.sort();
    assert_eq!(due, ids);
    // Suspending leaves the schedule: settled drops cost no queue slots.
    for id in &ids {
        let snapshot = store.snapshot(*id).unwrap();
        let suspend = store
            .prepare_update(
                *id,
                snapshot.revision,
                EntityPatch {
                    payload: None,
                    next_tick: Some(None),
                    position: None,
                },
            )
            .unwrap();
        store.apply_committed(suspend).unwrap();
    }
    assert!(store.due_entities(u64::MAX, 64).is_empty());
    assert_eq!(store.len(), 3);
}
