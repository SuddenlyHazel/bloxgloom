//! Fixtures for entity-backed drop behaviour pins.
use super::entity::{DROP_ENTITY_TYPE, DropEntityPayload, register_entity_type};
use super::planning;
use super::queries;
use crate::content::Catalog;
use crate::inventory::Stack;
use crate::items::ItemId;
use crate::protocol::DroppedItem;
use crate::server::entities::{
    EntityId, EntitySpawn, EntityStore, EntityTypeRegistry, EntityTypeRegistryBuilder,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

pub(super) fn temp_root(prefix: &str) -> std::path::PathBuf {
    static NEXT_TEMP_ROOT: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "{prefix}-{}-{}",
        std::process::id(),
        NEXT_TEMP_ROOT.fetch_add(1, Ordering::Relaxed)
    ))
}

pub(super) const fn item(id: u32) -> ItemId {
    ItemId::new(id)
}

pub(super) struct DropWorld {
    pub(super) store: EntityStore,
    pub(super) catalog: Arc<Catalog>,
    pub(super) types: Arc<EntityTypeRegistry>,
}

pub(super) fn drop_world() -> DropWorld {
    let catalog = Arc::new(Catalog::builtins());
    let mut builder = EntityTypeRegistryBuilder::new(&catalog);
    register_entity_type(&mut builder, Arc::clone(&catalog)).unwrap();
    crate::server::entities::register_player_entity_type(&mut builder).unwrap();
    crate::server::entities::register_kiln_entity_type(&mut builder, Arc::clone(&catalog)).unwrap();
    crate::server::entities::hopper::register(&mut builder, &catalog).unwrap();
    crate::server::entities::chest::register(&mut builder, &catalog).unwrap();
    crate::server::entities::mossbun::register(&mut builder, &catalog).unwrap();
    let types = Arc::new(builder.freeze().unwrap());
    DropWorld {
        store: EntityStore::new(Arc::clone(&types)),
        catalog,
        types,
    }
}

/// Stages one spawn plan through preparation and applies it, mirroring what
/// the coordinator does after a WAL receipt. Spawns carry no mirror here:
/// these fixtures own a bare store with no checkpoint worker.
pub(super) fn spawn(
    world: &mut DropWorld,
    position: [f32; 3],
    item: ItemId,
    count: u16,
    pickup_delay: Duration,
) -> Vec<EntityId> {
    spawn_at(world, position, item, count, pickup_delay, super::unix_ms())
}

pub(super) fn spawn_at(
    world: &mut DropWorld,
    position: [f32; 3],
    item: ItemId,
    count: u16,
    pickup_delay: Duration,
    now_ms: u64,
) -> Vec<EntityId> {
    let batch = planning::plan_spawns(
        &world.store,
        &world.catalog,
        &[(position, item, count, pickup_delay)],
        1,
        now_ms,
    )
    .unwrap()
    .expect("nonzero fixture spawn plans work");
    let ids = batch.entity_ids();
    world.store.apply_committed(batch).unwrap();
    ids
}

/// Inserts one drop exactly as described, bypassing merge, for age and
/// pickup-delay fixtures.
pub(super) fn insert_entry(
    world: &mut DropWorld,
    id: u64,
    position: [f32; 3],
    item: u8,
    count: u16,
    age: Duration,
    pickup_delay: Duration,
) {
    let now_ms = super::unix_ms();
    let created_ms = now_ms.saturating_sub(age.as_millis().min(u64::MAX as u128) as u64);
    let payload = DropEntityPayload::new(
        Stack::new(ItemId::new(u32::from(item)), count),
        created_ms,
        pickup_delay,
    )
    .into_entity_payload();
    let batch = world
        .store
        .prepare_spawn(EntitySpawn::Mobile {
            entity_type: DROP_ENTITY_TYPE,
            position,
            payload,
            spawn_tick: 1,
        })
        .unwrap();
    assert_eq!(batch.entity_id().get(), id);
    world.store.apply_committed(batch).unwrap();
    if age >= super::LIFETIME {
        // Fixture parity with the old live map: a drop born past its
        // lifetime never joins the airborne set. Production reaches the
        // same shape because settled drops suspend before they can expire.
        let target = EntityId::new(id).unwrap();
        let snapshot = world.store.snapshot(target).unwrap();
        let suspend = world
            .store
            .prepare_update(
                target,
                snapshot.revision,
                crate::server::entities::EntityPatch {
                    payload: None,
                    next_tick: Some(None),
                    position: None,
                },
            )
            .unwrap();
        world.store.apply_committed(suspend).unwrap();
    }
}

pub(super) fn take(world: &mut DropWorld, id: EntityId, count: u16) {
    let batch = planning::plan_take(&world.store, &[(id, count)])
        .unwrap()
        .expect("fixture take plans work");
    world.store.apply_committed(batch).unwrap();
}

pub(super) fn apply_expired(world: &mut DropWorld, limit: usize, now_ms: u64) -> usize {
    let Some(batch) = planning::plan_expired(&world.store, now_ms, limit).unwrap() else {
        return 0;
    };
    let count = batch.entity_ids().len();
    world.store.apply_committed(batch).unwrap();
    count
}

pub(super) fn nearby(world: &DropWorld, position: [f32; 3]) -> Vec<DroppedItem> {
    queries::nearby(&world.store, position)
}

pub(super) fn pickup_candidates(world: &DropWorld, position: [f32; 3]) -> Vec<DroppedItem> {
    queries::pickup_candidates(&world.store, position)
}

pub(super) fn stack(world: &DropWorld, id: EntityId) -> Option<Stack> {
    queries::stack(&world.store, id)
}

pub(super) fn active_len(world: &DropWorld) -> usize {
    queries::airborne_count(&world.store)
}

/// Every live drop record is indexed for spatial, schedule, and chunk-owner
/// reads together; no second map shadows the store.
pub(super) fn assert_store_consistent(world: &DropWorld) {
    use std::collections::BTreeMap;
    let records: BTreeMap<_, _> = world
        .store
        .record_values()
        .map(|record| (record.id, record.clone()))
        .collect();
    world
        .store
        .indexes()
        .validate_against_records(&records)
        .unwrap();
    assert_eq!(world.store.len(), records.len());
}

pub(super) fn stable_items(items: &[DroppedItem]) -> Vec<(u64, ItemId, u16, [f32; 3])> {
    items
        .iter()
        .map(|item| (item.id, item.item, item.count, item.position))
        .collect()
}

mod bounded;
mod pins;
