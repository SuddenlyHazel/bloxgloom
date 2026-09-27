use super::super::locomotion::tests::view;
use super::*;

fn snapshot(position: [f32; 3], bun: Mossbun) -> EntitySnapshot {
    let location = EntityLocation::Mobile { position };
    EntitySnapshot {
        id: EntityId::new(7).unwrap(),
        entity_type: crate::content::MOSSBUN_ENTITY_TYPE,
        revision: 1,
        motion_revision: 1,
        owner: location.owner().unwrap(),
        location,
        private_payload: EntityPayload::new(bun),
        next_tick: Some(4),
    }
}
fn plan(
    snapshot: &EntitySnapshot,
    tick: u64,
    view: &VoxelView,
) -> Result<EntityTickPlan, EntityError> {
    EntityTickPolicy::plan(
        &super::super::mobile::Adapter(Arc::new(crate::content::creatures::mossbun::definition())),
        snapshot,
        tick,
        &Catalog::builtins(),
        view,
        &EntityView::assemble(Vec::new(), snapshot.id),
    )
}

#[test]
fn choices_are_restartable_latency_independent_and_early_wakes_do_not_step() {
    let view = view(&[], &[]);
    let codec =
        super::super::mobile::Adapter(Arc::new(crate::content::creatures::mossbun::definition()));
    let snapshot = snapshot([8.5, 80.0, 8.5], Mossbun::default());
    let first = plan(&snapshot, 4, &view).unwrap();
    let late = plan(&snapshot, 100, &view).unwrap();
    assert_eq!(
        codec.encode(first.payload.as_ref().unwrap()).unwrap(),
        codec.encode(late.payload.as_ref().unwrap()).unwrap()
    );
    let mut walking = snapshot.clone();
    walking.private_payload = first.payload.unwrap();
    walking.next_tick = Some(8);
    let early = plan(&walking, 7, &view).unwrap();
    assert!(early.payload.is_none() && early.position.is_none());
    assert_eq!(early.next_tick, Some(8));
}

#[test]
fn idle_support_rechecks_do_not_advance_ai_and_airborne_hints_do_not_accelerate_time() {
    let flat = view(&[], &[]);
    let mut resting = snapshot(
        [8.5, 80.0, 8.5],
        Mossbun {
            think_at: 100,
            grounded: true,
            ..Default::default()
        },
    );
    let idle = plan(&resting, 4, &flat).unwrap();
    assert_eq!(idle.next_tick, Some(14));
    assert_eq!(
        idle.payload
            .unwrap()
            .downcast_ref::<Mossbun>()
            .unwrap()
            .cycle,
        0
    );
    resting.next_tick = Some(14);
    // No notification is needed: the next persisted support check finds the hole.
    let hole = view(&[], &[(8, 8)]);
    let falling = plan(&resting, 14, &hole).unwrap();
    assert!(falling.position.unwrap()[1] < 80.0);
    resting.location = EntityLocation::Mobile {
        position: falling.position.unwrap(),
    };
    resting.private_payload = falling.payload.unwrap();
    resting.next_tick = Some(20);
    let early = plan(&resting, 16, &hole).unwrap();
    assert!(early.position.is_none() && early.payload.is_none());
    assert_eq!(early.next_tick, Some(20));
}

#[test]
fn a_new_obstacle_invalidates_the_current_waypoint_before_movement() {
    let bun = Mossbun {
        goal: Some([10, 8]),
        waypoint: Some([9, 8]),
        steps: 16,
        grounded: true,
        ..Default::default()
    };
    let state = snapshot([8.5, 80.0, 8.5], bun);
    let result = plan(&state, 4, &view(&[(9, 80, 8)], &[])).unwrap();
    let next = result.position.unwrap();
    assert_eq!(next[0], 8.5);
    assert_ne!(next[2], 8.5, "replan around the newly blocked direct edge");
}

#[test]
fn creature_follows_route_around_obstacle_and_idles_at_goal() {
    let view = view(&[(9, 80, 8)], &[]);
    let mut snapshot = snapshot(
        [8.5, 80.0, 8.5],
        Mossbun {
            goal: Some([10, 8]),
            steps: 16,
            ..Default::default()
        },
    );
    let mut detoured = false;
    let mut arrived = false;
    for tick in (4..300).step_by(2) {
        let result = plan(&snapshot, tick, &view).unwrap();
        if let Some(position) = result.position {
            assert!(BODY.clear(&view, position).unwrap());
            detoured |= (position[2] - 8.5).abs() > 0.5;
            snapshot.location = EntityLocation::Mobile { position };
        }
        snapshot.next_tick = result.next_tick;
        snapshot.private_payload = result.payload.unwrap();
        if snapshot
            .private_payload
            .downcast_ref::<Mossbun>()
            .unwrap()
            .goal
            .is_none()
        {
            let EntityLocation::Mobile { position } = snapshot.location else {
                unreachable!()
            };
            assert!(
                glam::Vec3::from_array(position).distance(glam::Vec3::new(10.5, 80.0, 8.5)) < 0.02
            );
            assert!(
                snapshot
                    .private_payload
                    .downcast_ref::<Mossbun>()
                    .unwrap()
                    .think_at
                    >= tick + 40
            );
            assert_eq!(snapshot.next_tick, Some(tick + 10));
            arrived = true;
            break;
        }
    }
    assert!(detoured && arrived);
}

#[test]
fn fall_velocity_and_navigation_state_round_trip_canonically() {
    let codec =
        super::super::mobile::Adapter(Arc::new(crate::content::creatures::mossbun::definition()));
    let payload = EntityPayload::new(Mossbun {
        cycle: 42,
        facing: 3,
        steps: 16,
        goal: Some([-2, 3]),
        waypoint: Some([-1, 2]),
        vertical_velocity: -3.2,
        grounded: false,
        think_at: 123,
    });
    let bytes = codec.encode(&payload).unwrap();
    assert_eq!(bytes.len(), 41);
    assert_eq!(codec.encode(&codec.decode(&bytes).unwrap()).unwrap(), bytes);
    assert_eq!(codec.public_view(&payload).unwrap(), [3, 1]);
    for bytes in [vec![], vec![0; 42], vec![255; 41]] {
        assert!(codec.decode(&bytes).is_err());
    }
    let mut invalid = bytes;
    invalid[10..14].copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(codec.decode(&invalid).is_err());
    for position in [
        [f32::NAN, 80.0, 0.0],
        [1_000_000.0, 80.0, 0.0],
        [0.0, crate::world::BEDROCK_Y as f32, 0.0],
    ] {
        assert!(
            codec
                .validate_location(&EntityLocation::Mobile { position })
                .is_err()
        );
    }
}

#[test]
fn resolved_registration_store_validation_and_checkpoint_use_the_same_schema() {
    let local = Catalog::builtins();
    let mut manifest = crate::content::ContentManifest::from_catalog(&local);
    manifest
        .entries
        .iter_mut()
        .find(|e| e.kind == b'E' && e.key == "bloxgloom:mossbun")
        .unwrap()
        .id = 71_234;
    manifest.entries.sort_unstable_by_key(|e| (e.kind, e.id));
    let catalog = Arc::new(manifest.resolve_catalog(&local).unwrap());
    let startup = crate::server::startup::ServerStartup::new(catalog.clone());
    let types = startup.entity_types_for(catalog).unwrap();
    let id = crate::content::EntityTypeId(71_234);
    let descriptor = types.descriptor(id).unwrap();
    assert_eq!(
        descriptor.schema_fingerprint(),
        crate::content::MOSSBUN_SCHEMA_FINGERPRINT
    );
    assert!(!descriptor.tick_reads_neighbours());
    assert!(descriptor.decode_payload(1, &[0; 10]).is_err());
    let mut store = EntityStore::new(types.clone());
    let spawn = |position| EntitySpawn::Mobile {
        entity_type: id,
        position,
        payload: EntityPayload::new(Mossbun::default()),
        spawn_tick: 10,
    };
    assert!(store.prepare_spawn(spawn([f32::NAN, 80.0, 0.0])).is_err());
    let prepared = store.prepare_spawn(spawn([0.5, 80.0, 0.5])).unwrap();
    let entity_id = prepared.entity_id();
    store.apply_committed(prepared).unwrap();
    let bytes = encode_checkpoint(&store).unwrap();
    let recovered = decode_checkpoint(&bytes, types).unwrap();
    let snapshot = recovered.snapshot(entity_id).unwrap();
    assert_eq!(snapshot.entity_type, id);
    assert_eq!(snapshot.next_tick, Some(11));
    assert_eq!(
        snapshot.private_payload.downcast_ref::<Mossbun>(),
        Some(&Mossbun::default())
    );
}
