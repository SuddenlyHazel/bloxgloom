//! Custom lifecycle through registered planners, shared batches and checkpoint rebuild.
use super::*;
use crate::server::entities::{EntityPatch, EntityView, decode_checkpoint, encode_checkpoint};
use bloxgloom_host_api::content::{Components, DropPolicy, Item};

fn custom_world(policy: DropPolicy) -> (DropWorld, ItemId) {
    let mut catalog = Catalog::builtins();
    catalog
        .public_item(&Item {
            key: "test:policy".into(),
            name: "Policy".into(),
            texture: "bloxgloom:stone".into(),
            swatch: [1.0; 4],
            placeable: None,
            sprite: true,
            drop_size: Default::default(),
            drop_animation: Default::default(),
            drop_policy: policy,
            components: Components::None,
        })
        .unwrap();
    let item = catalog.item_by_key("test:policy").unwrap();
    (drop_world_in(catalog), item)
}

#[test]
fn custom_merge_refresh_reindexes_deadline_and_expired_targets_never_revive() {
    let (mut world, item) = custom_world(DropPolicy {
        merge_range: 3.0,
        lifetime_ms: 2_000,
        ..Default::default()
    });
    let a = spawn_at(
        &mut world,
        [0.5, 4.0, 0.5],
        item,
        120,
        Duration::ZERO,
        1_000,
    )[0];
    // Custom range, stable oldest-ID selection, and the unaltered 128 cap.
    spawn_at(&mut world, [2.5, 4.0, 0.5], item, 20, Duration::ZERO, 2_000);
    assert_eq!(stack(&world, a).unwrap().count, 128);
    assert_eq!(world.store.len(), 2);
    assert!(
        !queries::has_expired(&world.store, 3_000),
        "old created age must be removed from due index"
    );
    let bytes = encode_checkpoint(&world.store).unwrap();
    world.store = decode_checkpoint(&bytes, Arc::clone(&world.types)).unwrap();
    assert!(!queries::has_expired(&world.store, 3_999));
    let stale = planning::plan_expired(&world.store, &world.catalog, 4_000, 1)
        .unwrap()
        .unwrap();
    assert_eq!(stale.entity_ids(), vec![a]);
    // A prepared expiry cannot bypass a later authoritative payload revision.
    let snapshot = world.store.snapshot(a).unwrap();
    let mut payload = snapshot
        .private_payload
        .downcast_ref::<DropEntityPayload>()
        .unwrap()
        .clone();
    payload.created_unix_ms = 3_000;
    let refreshed = world
        .store
        .prepare_update(
            a,
            snapshot.revision,
            EntityPatch {
                payload: Some(payload.into_entity_payload()),
                ..Default::default()
            },
        )
        .unwrap();
    world.store.apply_committed(refreshed).unwrap();
    assert!(world.store.apply_committed(stale).is_err());
    // At the exact deadline the partial stack cannot merge, even before expiry commits.
    spawn_at(&mut world, [2.5, 4.0, 0.5], item, 5, Duration::ZERO, 4_000);
    assert_eq!(world.store.len(), 3);
    assert_eq!(apply_expired(&mut world, 1, 4_000), 1);
    assert_eq!(
        nearby(&world, [0.5, 4.0, 0.5])
            .iter()
            .map(|d| u32::from(d.count))
            .sum::<u32>(),
        133
    );
    assert_store_consistent(&world);
}

#[test]
fn custom_pickup_range_delay_and_lifetime_share_authoritative_policy() {
    let (mut world, item) = custom_world(DropPolicy {
        pickup_range: 5.0,
        merge_range: 0.0,
        lifetime_ms: 2_000,
        ..Default::default()
    });
    let now = super::super::unix_ms();
    let fresh = spawn_at(&mut world, [0.0, 4.0, 0.0], item, 1, Duration::ZERO, now)[0];
    let expired = spawn_at(
        &mut world,
        [0.0, 4.0, 0.0],
        item,
        1,
        Duration::ZERO,
        now - 3_000,
    )[0];
    let delayed = spawn_at(
        &mut world,
        [0.0, 4.0, 0.0],
        item,
        1,
        Duration::from_secs(60),
        now,
    )[0];
    assert_eq!(
        pickup_candidates(&world, [5.0, 4.0, 0.0])
            .iter()
            .map(|d| d.id)
            .collect::<Vec<_>>(),
        vec![fresh.get()]
    );
    assert!(queries::pickup_eligible(
        &world.store,
        &world.catalog,
        fresh.get(),
        [5.0, 4.0, 0.0]
    ));
    assert!(!queries::pickup_eligible(
        &world.store,
        &world.catalog,
        fresh.get(),
        [5.01, 4.0, 0.0]
    ));
    assert!(!queries::extractable(&world.store, &world.catalog, expired));
    assert!(!queries::extractable(&world.store, &world.catalog, delayed));
    assert_eq!(
        world.store.expired_entities(now, 1),
        vec![expired],
        "deadline order, not lowest ID"
    );
}

#[test]
fn expanded_merge_range_fences_negative_reads_across_chunk_seam() {
    let (mut world, item) = custom_world(DropPolicy {
        merge_range: 3.0,
        ..Default::default()
    });
    let full = spawn_at(
        &mut world,
        [16.25, 4.0, 0.5],
        item,
        128,
        Duration::ZERO,
        1_000,
    )[0];
    // The existing full stack is not selected, but its later availability must
    // invalidate this negative read. A stock-radius capture misses its page.
    let fresh = planning::plan_spawns(
        &world.store,
        &world.catalog,
        &[([13.5, 4.0, 0.5], item, 1, Duration::ZERO)],
        2,
        1_001,
    )
    .unwrap()
    .unwrap();
    take(&mut world, full, 1);
    assert!(world.store.apply_committed(fresh).is_err());
    spawn_at(&mut world, [13.5, 4.0, 0.5], item, 1, Duration::ZERO, 1_001);
    assert_eq!(world.store.len(), 1);
    assert_eq!(stack(&world, full).unwrap().count, 128);
}

#[test]
fn registered_falling_uses_custom_gravity_terminal_speed_radius_and_restarts() {
    use crate::server::voxel_view::VoxelView;
    use crate::world::{AIR, CHUNK_VOLUME, Chunk, ChunkKey, STONE};
    let policy = DropPolicy {
        gravity: 12.0,
        terminal_speed: 4.0,
        radius: 0.4,
        ..Default::default()
    };
    let (mut world, item) = custom_world(policy);
    let mut blocks = vec![AIR; CHUNK_VOLUME];
    for x in 0..16 {
        for z in 0..16 {
            blocks[Chunk::index([x, 2, z]).unwrap()] = STONE;
        }
    }
    let view = VoxelView::from_chunks_in(
        [Chunk::from_blocks(ChunkKey { x: 0, y: 0, z: 0 }, 1, blocks)],
        Arc::clone(&world.catalog),
    )
    .unwrap();
    let id = spawn_at(&mut world, [1.5, 4.5, 1.5], item, 1, Duration::ZERO, 1_000)[0];
    let snapshot = world.store.snapshot(id).unwrap();
    let neighbours = EntityView::assemble(Vec::new(), id);
    let descriptor = world.types.descriptor(DROP_ENTITY_TYPE).unwrap();
    let first = descriptor
        .plan_tick(&snapshot, 2, &world.catalog, &view, &neighbours)
        .unwrap();
    let dt = crate::server::simulation::FIXED_STEP.as_secs_f32();
    let speed = first
        .payload
        .as_ref()
        .unwrap()
        .downcast_ref::<DropEntityPayload>()
        .unwrap()
        .vertical_speed;
    assert_eq!(speed, -policy.gravity * dt);
    let update = world
        .store
        .prepare_update(
            id,
            snapshot.revision,
            EntityPatch {
                payload: first.payload,
                position: first.position,
                next_tick: Some(first.next_tick),
            },
        )
        .unwrap();
    world.store.apply_committed(update).unwrap();
    world.store = decode_checkpoint(
        &encode_checkpoint(&world.store).unwrap(),
        Arc::clone(&world.types),
    )
    .unwrap();
    let mut snapshot = world.store.snapshot(id).unwrap();
    assert_eq!(
        snapshot
            .private_payload
            .downcast_ref::<DropEntityPayload>()
            .unwrap()
            .vertical_speed,
        speed
    );
    // A persisted falling speed is clamped by the same declared terminal speed.
    snapshot.private_payload = snapshot
        .private_payload
        .downcast_ref::<DropEntityPayload>()
        .unwrap()
        .clone()
        .with_vertical_speed(-30.0)
        .into_entity_payload();
    let terminal = descriptor
        .plan_tick(&snapshot, 3, &world.catalog, &view, &neighbours)
        .unwrap();
    assert_eq!(
        terminal
            .payload
            .unwrap()
            .downcast_ref::<DropEntityPayload>()
            .unwrap()
            .vertical_speed,
        -4.0
    );
    snapshot.location = crate::server::entities::EntityLocation::Mobile {
        position: [1.5, 3.41, 1.5],
    };
    let landed = descriptor
        .plan_tick(&snapshot, 4, &world.catalog, &view, &neighbours)
        .unwrap();
    assert_eq!(landed.position, Some([1.5, 3.0 + policy.radius, 1.5]));
    assert_eq!(landed.next_tick, None);
}
