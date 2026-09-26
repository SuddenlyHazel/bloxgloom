use super::*;
use crate::world::{AIR, CHUNK_VOLUME, Chunk, ChunkKey, STONE};

fn terrain_view(wall: bool, cliff: bool, missing_seam: bool) -> VoxelView {
    let mut chunks = Vec::new();
    for x in 0..=1 {
        if x == 1 && missing_seam {
            continue;
        }
        for y in 4..=5 {
            let mut chunk = Chunk::from_blocks(ChunkKey { x, y, z: 0 }, 1, vec![AIR; CHUNK_VOLUME]);
            for lx in 0..16 {
                for z in 0..16 {
                    if y == 4 && !(cliff && x == 1) {
                        chunk.blocks.set(Chunk::index([lx, 15, z]).unwrap(), STONE);
                    }
                }
            }
            if wall && x == 1 && y == 5 {
                chunk.blocks.set(Chunk::index([0, 0, 8]).unwrap(), STONE);
            }
            chunks.push(chunk);
        }
    }
    VoxelView::from_chunks(chunks).unwrap()
}

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
    Wander.plan(
        snapshot,
        tick,
        &Catalog::builtins(),
        view,
        &EntityView::assemble(Vec::new(), snapshot.id),
    )
}

#[test]
fn steps_cross_resident_seams_but_stop_at_walls_cliffs_and_missing_terrain() {
    let snapshot = snapshot(
        [15.625, 80.0, 8.5],
        Mossbun {
            cycle: 4,
            facing: 1,
            steps: 8,
        },
    );
    assert_eq!(
        plan(&snapshot, 4, &terrain_view(false, false, false))
            .unwrap()
            .position,
        Some([15.75, 80.0, 8.5])
    );
    for (wall, cliff) in [(true, false), (false, true)] {
        let result = plan(&snapshot, 4, &terrain_view(wall, cliff, false)).unwrap();
        assert_eq!(result.position, None);
        assert_eq!(
            result
                .payload
                .unwrap()
                .downcast_ref::<Mossbun>()
                .unwrap()
                .steps,
            0
        );
        assert!(result.next_tick.unwrap() >= 44);
    }
    assert!(matches!(
        plan(&snapshot, 4, &terrain_view(false, false, true)),
        Err(EntityError::ViewOutOfRange)
    ));
}

#[test]
fn choices_are_restartable_latency_independent_and_early_wakes_do_not_step() {
    let view = terrain_view(false, false, false);
    let snapshot = snapshot([8.5, 80.0, 8.5], Mossbun::default());
    let first = plan(&snapshot, 4, &view).unwrap();
    let late = plan(&snapshot, 100, &view).unwrap();
    assert_eq!(
        Codec.encode(first.payload.as_ref().unwrap()).unwrap(),
        Codec.encode(late.payload.as_ref().unwrap()).unwrap()
    );
    let bun = *first.payload.unwrap().downcast_ref::<Mossbun>().unwrap();
    assert!((8..=16).contains(&bun.steps));
    let mut walking = snapshot.clone();
    walking.private_payload = EntityPayload::new(bun);
    walking.next_tick = Some(8);
    let early = plan(&walking, 7, &view).unwrap();
    assert!(early.payload.is_none() && early.position.is_none());
    assert_eq!(early.next_tick, Some(8));
    let step = plan(&walking, 8, &view).unwrap();
    assert!(step.position.is_some());
    walking.private_payload = EntityPayload::new(Mossbun { steps: 1, ..bun });
    let idle = plan(&walking, 8, &view).unwrap();
    assert_eq!(
        idle.payload
            .unwrap()
            .downcast_ref::<Mossbun>()
            .unwrap()
            .steps,
        0
    );
    assert!(idle.next_tick.unwrap() >= 48);
}

#[test]
fn unsupported_bun_settles_without_penetrating_floor() {
    let view = terrain_view(false, false, false);
    let snapshot = snapshot([8.5, 80.125, 8.5], Mossbun::default());
    assert_eq!(
        plan(&snapshot, 4, &view).unwrap().position,
        Some([8.5, 80.0, 8.5])
    );
}

#[test]
fn codec_is_bounded_and_canonical() {
    let payload = EntityPayload::new(Mossbun {
        cycle: 42,
        facing: 3,
        steps: 16,
    });
    let bytes = Codec.encode(&payload).unwrap();
    assert_eq!(bytes.len(), 10);
    assert_eq!(Codec.encode(&Codec.decode(&bytes).unwrap()).unwrap(), bytes);
    assert_eq!(Codec.public_view(&payload).unwrap(), [3, 1]);
    for bytes in [vec![], vec![0; 11], vec![255; 10]] {
        assert!(Codec.decode(&bytes).is_err());
    }
    for position in [
        [f32::NAN, 80.0, 0.0],
        [1_000_000.0, 80.0, 0.0],
        [0.0, crate::world::BEDROCK_Y as f32, 0.0],
    ] {
        assert!(
            Codec
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
    assert!(descriptor.decode_payload(2, &[0; 10]).is_err());
    let mut store = EntityStore::new(types.clone());
    let spawn = |position| EntitySpawn::Mobile {
        entity_type: id,
        position,
        payload: EntityPayload::new(Mossbun::default()),
        spawn_tick: 10,
    };
    assert!(store.prepare_spawn(spawn([f32::NAN, 80.0, 0.0])).is_err());
    assert!(
        store
            .prepare_spawn(spawn([1_000_000.0, 80.0, 0.0]))
            .is_err()
    );
    let prepared = store.prepare_spawn(spawn([0.5, 80.0, 0.5])).unwrap();
    let entity_id = prepared.entity_id();
    store.apply_committed(prepared).unwrap();
    assert!(
        store
            .prepare_update(
                entity_id,
                1,
                EntityPatch {
                    position: Some([f32::INFINITY, 80.0, 0.5]),
                    ..Default::default()
                }
            )
            .is_err()
    );
    let bytes = encode_checkpoint(&store).unwrap();
    let recovered = decode_checkpoint(&bytes, types).unwrap();
    let snapshot = recovered.snapshot(entity_id).unwrap();
    assert_eq!(snapshot.entity_type, id);
    assert_eq!(snapshot.next_tick, Some(14));
    assert_eq!(
        snapshot.private_payload.downcast_ref::<Mossbun>(),
        Some(&Mossbun::default())
    );
}
