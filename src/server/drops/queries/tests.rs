//! Entity-backed drop query coverage: visibility, pickup gating, wakes.
use super::*;
use crate::content::Catalog;
use crate::inventory::Stack;
use crate::items::ItemId;
use crate::server::entities::{EntitySpawn, EntityStore, EntityTypeRegistryBuilder};
use std::sync::Arc;
use std::time::Duration;

fn test_store() -> (EntityStore, Arc<Catalog>) {
    let catalog = Arc::new(Catalog::builtins());
    let mut builder = EntityTypeRegistryBuilder::new(&catalog);
    super::super::entity::register_entity_type(&mut builder, Arc::clone(&catalog)).unwrap();
    crate::server::entities::register_player_entity_type(&mut builder).unwrap();
    crate::server::entities::register_kiln_entity_type(&mut builder, Arc::clone(&catalog)).unwrap();
    crate::server::entities::mossbun::register(&mut builder, &catalog).unwrap();
    (
        EntityStore::new(Arc::new(builder.freeze().unwrap())),
        catalog,
    )
}

fn spawn_direct(
    store: &mut EntityStore,
    position: [f32; 3],
    count: u16,
    created_ms: u64,
    delay: Duration,
) -> EntityId {
    let payload = DropEntityPayload::new(Stack::new(ItemId::new(1), count), created_ms, delay)
        .into_entity_payload();
    let batch = store
        .prepare_spawn(EntitySpawn::Mobile {
            entity_type: DROP_ENTITY_TYPE,
            position,
            payload,
            spawn_tick: 1,
        })
        .unwrap();
    let id = batch.entity_id();
    store.apply_committed(batch).unwrap();
    id
}

#[test]
fn pickup_delay_gates_candidates_but_not_visibility() {
    let (mut store, _catalog) = test_store();
    let born_ms = super::super::unix_ms();
    spawn_direct(
        &mut store,
        [0.0, 0.0, 0.0],
        1,
        born_ms,
        Duration::from_secs(3_600),
    );
    let gated = spawn_direct(&mut store, [0.5, 0.0, 0.0], 1, born_ms, Duration::ZERO);
    // Wall clock has just passed both births, so only the delay gates.
    let candidates = pickup_candidates(&store, [0.0, 0.0, 0.0]);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].id, gated.get());
    assert_eq!(nearby(&store, [0.0, 0.0, 0.0]).len(), 2);
}

#[test]
fn expired_drop_is_visible_but_never_pickable() {
    let (mut store, _catalog) = test_store();
    // Youth is wall-clock: only a fresh birth is eligible.
    let born_ms = super::super::unix_ms();
    spawn_direct(&mut store, [0.0, 0.0, 0.0], 5, born_ms, Duration::ZERO);
    assert_eq!(pickup_candidates(&store, [0.0, 0.0, 0.0]).len(), 1);
    assert_eq!(nearby(&store, [0.0, 0.0, 0.0]).len(), 1);
    assert!(!has_expired(&store, born_ms + 1_000));
    assert!(has_expired(&store, born_ms + LIFETIME.as_millis() as u64));
}

#[test]
fn airborne_count_tracks_schedule_not_records() {
    let (mut store, _catalog) = test_store();
    let id = spawn_direct(&mut store, [0.0, 0.0, 0.0], 1, 1_000, Duration::ZERO);
    // Fresh spawns tick every tick until they settle.
    assert_eq!(airborne_count(&store), 1);
    let snapshot = store.snapshot(id).unwrap();
    let suspend = store
        .prepare_update(
            id,
            snapshot.revision,
            crate::server::entities::EntityPatch {
                payload: None,
                next_tick: Some(None),
                position: None,
            },
        )
        .unwrap();
    store.apply_committed(suspend).unwrap();
    assert_eq!(airborne_count(&store), 0);
    assert_eq!(store.len(), 1);
    // The settled drop still owns its chunk for wake routing.
    let chunk = crate::world::world_to_chunk(0, 0, 0).0;
    assert_eq!(store.terrain_wake_ids(chunk, 16), vec![id]);
    assert!(
        store
            .terrain_wake_ids(crate::world::ChunkKey { x: 9, y: 9, z: 9 }, 16)
            .is_empty()
    );
}

#[test]
fn immutable_mobile_pages_keep_old_capture_and_project_nearest_without_full_output_allocation() {
    let (mut store, _) = test_store();
    let id = spawn_direct(&mut store, [0.5, 0.5, 0.5], 1, 1000, Duration::ZERO);
    let old = capture_nearby(&store, [0.5, 0.5, 0.5]).unwrap();
    let motion = store
        .prepare_update(
            id,
            store.snapshot(id).unwrap().revision,
            crate::server::entities::EntityPatch {
                position: Some([5.5, 0.5, 0.5]),
                ..Default::default()
            },
        )
        .unwrap();
    store.apply_committed(motion).unwrap();
    assert_eq!(
        project_nearby(old, [0.5, 0.5, 0.5], 2000)[0].position,
        [0.5, 0.5, 0.5]
    );
    let current = project_nearby(
        capture_nearby(&store, [0.5, 0.5, 0.5]).unwrap(),
        [0.5, 0.5, 0.5],
        2000,
    );
    assert_eq!(current[0].position, [5.5, 0.5, 0.5]);
    assert_eq!(current[0].age_ms, 1000);
    let spawns = (0..300)
        .map(|_| EntitySpawn::Mobile {
            entity_type: DROP_ENTITY_TYPE,
            position: [0.5, 0.5, 0.5],
            payload: DropEntityPayload::new(Stack::new(ItemId::new(1), 1), 1000, Duration::ZERO)
                .into_entity_payload(),
            spawn_tick: 1,
        })
        .collect();
    let batch = store.prepare_spawn_batch(spawns).unwrap();
    store.apply_committed(batch).unwrap();
    let nearest = project_nearby(
        capture_nearby(&store, [0.5, 0.5, 0.5]).unwrap(),
        [0.5, 0.5, 0.5],
        2000,
    );
    assert_eq!(nearest.len(), 256);
    assert_eq!(nearest[0].id, id.get() + 1);
    assert_eq!(nearest[255].id, id.get() + 256);
}
