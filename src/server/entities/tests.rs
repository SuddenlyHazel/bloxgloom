use super::*;
use crate::content::{BlockStateId, Catalog, EntityTypeDef, KILN_ENTITY_TYPE};
use crate::server::entities::registry::EntityTypeRegistration;
use crate::world::ChunkKey;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

const DROP_TYPE: crate::content::EntityTypeId = crate::content::EntityTypeId(1);
const PLAYER_TYPE: crate::content::EntityTypeId = crate::content::EntityTypeId(2);
const KILN_TYPE: crate::content::EntityTypeId = KILN_ENTITY_TYPE;

struct StackPayloadCodec;

#[derive(Clone, Debug, Eq, PartialEq)]
struct StackPayload {
    item: u32,
    count: u16,
    components: Vec<u8>,
}

impl EntityPayloadCodec for StackPayloadCodec {
    fn decode(&self, payload: &[u8]) -> Result<EntityPayload, EntityCodecError> {
        if payload.len() < 8 {
            return Err(EntityCodecError::InvalidData);
        }
        let item = u32::from_le_bytes(payload[0..4].try_into().unwrap());
        let count = u16::from_le_bytes(payload[4..6].try_into().unwrap());
        let components = usize::from(u16::from_le_bytes(payload[6..8].try_into().unwrap()));
        if item == 0
            || count == 0
            || count > 128
            || components > 1_024
            || payload.len() != 8 + components
        {
            return Err(EntityCodecError::InvalidData);
        }
        Ok(EntityPayload::new(StackPayload {
            item,
            count,
            components: payload[8..].to_vec(),
        }))
    }

    fn encode(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        Ok(encode_stack(
            payload
                .downcast_ref::<StackPayload>()
                .ok_or(EntityCodecError::InvalidData)?,
        ))
    }

    fn public_view(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        let value = payload
            .downcast_ref::<StackPayload>()
            .ok_or(EntityCodecError::InvalidData)?;
        let mut view = Vec::with_capacity(6);
        view.extend(value.item.to_le_bytes());
        view.extend(value.count.to_le_bytes());
        Ok(view)
    }
}

struct AppearanceCodec;

impl EntityPayloadCodec for AppearanceCodec {
    fn decode(&self, payload: &[u8]) -> Result<EntityPayload, EntityCodecError> {
        let value: [u8; 4] = payload
            .try_into()
            .map_err(|_| EntityCodecError::InvalidData)?;
        Ok(EntityPayload::new(value))
    }

    fn encode(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        Ok(payload
            .downcast_ref::<[u8; 4]>()
            .ok_or(EntityCodecError::InvalidData)?
            .to_vec())
    }

    fn public_view(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        self.encode(payload)
    }
}

fn fixture_registry() -> Arc<EntityTypeRegistry> {
    let catalog = Arc::new(Catalog::builtins());
    let mut builder = EntityTypeRegistryBuilder::new(&catalog);
    for (id, ownership, tick_policy, max_payload_bytes, codec) in [
        (
            DROP_TYPE,
            EntityOwnership::Mobile,
            TickPolicy::EveryTick,
            1_032,
            Arc::new(StackPayloadCodec) as Arc<dyn EntityPayloadCodec>,
        ),
        (
            PLAYER_TYPE,
            EntityOwnership::Mobile,
            TickPolicy::EveryTick,
            4,
            Arc::new(AppearanceCodec) as Arc<dyn EntityPayloadCodec>,
        ),
    ] {
        builder
            .register(EntityTypeRegistration {
                id,
                ownership,
                tick_policy,
                max_payload_bytes,
                codec,
            })
            .unwrap();
    }
    register_kiln_entity_type(&mut builder, catalog.clone()).unwrap();
    Arc::new(builder.freeze().unwrap())
}

fn stack_payload(item: u32, count: u16, components: &[u8]) -> EntityPayload {
    EntityPayload::new(StackPayload {
        item,
        count,
        components: components.to_vec(),
    })
}

fn encode_stack(payload: &StackPayload) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(8 + payload.components.len());
    bytes.extend(payload.item.to_le_bytes());
    bytes.extend(payload.count.to_le_bytes());
    bytes.extend((payload.components.len() as u16).to_le_bytes());
    bytes.extend(&payload.components);
    bytes
}

fn spawn_drop(store: &EntityStore, position: [f32; 3]) -> PreparedEntityTransaction {
    store
        .prepare_spawn(EntitySpawn::Mobile {
            entity_type: DROP_TYPE,
            position,
            payload: stack_payload(4, 7, &[9, 8, 7]),
            spawn_tick: 10,
        })
        .unwrap()
}

#[test]
fn mobile_lifecycle_is_prepared_revisioned_and_transfers_at_owner_boundary() {
    let mut store = EntityStore::new(fixture_registry());
    let spawn = spawn_drop(&store, [15.5, -0.5, -0.5]);
    assert_eq!(store.len(), 0);
    assert_eq!(store.next_id(), 1);
    assert!(
        spawn
            .changes()
            .iter()
            .any(|change| { change.key.domain == "bloxgloom:entity_allocator" })
    );
    assert!(
        spawn
            .changes()
            .iter()
            .any(|change| change.key.domain == "bloxgloom:entity")
    );
    let committed = store.apply_committed(spawn).unwrap();
    let id = match &committed.deltas[0] {
        EntityDelta::Spawned(view) => {
            assert_eq!(view.payload, [4, 0, 0, 0, 7, 0]);
            assert_eq!(
                view.owner,
                EntityOwner::Mobile(ChunkKey { x: 0, y: -1, z: -1 })
            );
            view.id
        }
        other => panic!("unexpected spawn event: {other:?}"),
    };
    assert_eq!(store.due_entities(10, 8), Vec::<EntityId>::new());
    assert_eq!(store.due_entities(11, 8), vec![id]);

    let stale = store
        .prepare_update(
            id,
            1,
            EntityPatch {
                payload: Some(stack_payload(4, 8, &[1, 2, 3])),
                ..Default::default()
            },
        )
        .unwrap();
    let winner = store
        .prepare_update(
            id,
            1,
            EntityPatch {
                payload: Some(stack_payload(4, 9, &[1, 2, 3])),
                ..Default::default()
            },
        )
        .unwrap();
    store.apply_committed(winner).unwrap();
    assert!(matches!(
        store.validate_prepared(&stale),
        Err(EntityError::InvalidTransaction | EntityError::StaleRevision { .. })
    ));
    let snapshot = store.snapshot(id).unwrap();
    assert_eq!(snapshot.revision, 2);
    assert_eq!(
        snapshot.private_payload.downcast_ref::<StackPayload>(),
        Some(&StackPayload {
            item: 4,
            count: 9,
            components: vec![1, 2, 3],
        })
    );

    let transfer = store.prepare_transfer(id, 2, [16.0, -0.5, -0.5]).unwrap();
    let transfer_chunks = transfer
        .changes()
        .iter()
        .filter(|change| change.key.domain == "bloxgloom:entity_chunk")
        .count();
    assert_eq!(transfer_chunks, 2);
    let committed = store.apply_committed(transfer).unwrap();
    assert!(matches!(
        committed.deltas[0],
        EntityDelta::Transferred { .. }
    ));
    assert_eq!(
        store.owner(id),
        Some(EntityOwner::Mobile(ChunkKey { x: 1, y: -1, z: -1 }))
    );
    assert_eq!(store.ids_for_chunk(ChunkKey { x: 0, y: -1, z: -1 }), vec![]);
    assert_eq!(
        store.ids_for_chunk(ChunkKey { x: 1, y: -1, z: -1 }),
        vec![id]
    );
}

#[test]
fn delayed_payload_receipt_merges_with_newer_checkpointed_mobile_motion() {
    let types = fixture_registry();
    let mut store = EntityStore::new(types.clone());
    let spawn = spawn_drop(&store, [2.0, 4.0, 6.0]);
    let id = spawn.entity_id();
    store.apply_committed(spawn).unwrap();
    assert_eq!(store.revision(), 1);

    let payload_update = store
        .prepare_update(
            id,
            1,
            EntityPatch {
                payload: Some(stack_payload(4, 11, &[2, 4, 6])),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(
        payload_update
            .changes()
            .iter()
            .all(|change| change.key.domain != ENTITY_MOTION_DOMAIN)
    );

    let movement = store
        .update_mobile_motion(id, 1, [2.75, 4.5, 6.25])
        .unwrap();
    assert!(matches!(
        movement.deltas.as_slice(),
        [EntityDelta::Moved(_)]
    ));
    assert_eq!(store.revision(), 1, "motion has its own revision domain");
    let motion_checkpoint = encode_checkpoint(&store).unwrap();
    let overlay = payload_update
        .changes()
        .iter()
        .map(|change| (change.key.clone(), change.after.clone()))
        .collect::<BTreeMap<_, _>>();
    store.validate_prepared(&payload_update).unwrap();
    store.apply_committed(payload_update).unwrap();
    assert_eq!(store.revision(), 2);
    let view = store.public_view(id).unwrap();
    assert_eq!(view.revision, 2);
    assert_eq!(view.motion_revision, 2);
    assert_eq!(
        view.location,
        EntityLocation::Mobile {
            position: [2.75, 4.5, 6.25]
        }
    );
    assert_eq!(
        store
            .snapshot(id)
            .unwrap()
            .private_payload
            .downcast_ref::<StackPayload>()
            .unwrap()
            .count,
        11
    );

    let restored = decode_checkpoint(&encode_checkpoint(&store).unwrap(), types.clone()).unwrap();
    let restored = restored.public_view(id).unwrap();
    assert_eq!(restored.motion_revision, 2);
    assert_eq!(restored.location, view.location);

    let mut recovered = decode_checkpoint(&motion_checkpoint, types).unwrap();
    assert!(recovered.apply_journal_overlay(&overlay).unwrap());
    let recovered = recovered.public_view(id).unwrap();
    assert_eq!(recovered.revision, 2);
    assert_eq!(recovered.motion_revision, 2);
    assert_eq!(recovered.location, view.location);
    assert_eq!(recovered.payload, view.payload);
}

#[test]
fn wal_revision_watermark_survives_despawning_highest_revision_entity() {
    let types = fixture_registry();
    let mut store = EntityStore::new(types.clone());
    let spawn = store
        .prepare_spawn_batch(vec![
            EntitySpawn::Mobile {
                entity_type: DROP_TYPE,
                position: [1.0, 2.0, 3.0],
                payload: stack_payload(4, 1, &[]),
                spawn_tick: 1,
            },
            EntitySpawn::Mobile {
                entity_type: DROP_TYPE,
                position: [4.0, 5.0, 6.0],
                payload: stack_payload(4, 1, &[]),
                spawn_tick: 1,
            },
        ])
        .unwrap();
    assert_eq!(
        spawn
            .changes()
            .iter()
            .filter(|change| change.key.domain == ENTITY_REVISION_DOMAIN)
            .count(),
        1
    );
    store.apply_committed(spawn).unwrap();
    let checkpoint = encode_checkpoint(&store).unwrap();

    let removed = EntityId::new(2).unwrap();
    let despawn = store.prepare_despawn(removed, 1).unwrap();
    let overlay = despawn
        .changes()
        .iter()
        .map(|change| (change.key.clone(), change.after.clone()))
        .collect::<BTreeMap<_, _>>();
    store.apply_committed(despawn).unwrap();
    assert_eq!(store.revision(), 2);
    assert_eq!(store.len(), 1);

    let mut recovered = decode_checkpoint(&checkpoint, types).unwrap();
    assert!(recovered.apply_journal_overlay(&overlay).unwrap());
    assert_eq!(recovered.revision(), 2);
    assert_eq!(recovered.durable_sequence(), 2);
    assert_eq!(recovered.durable_global_revision(), 2);
    assert_eq!(recovered.len(), 1);
}

#[test]
fn spawn_batch_uses_one_allocator_and_coalesces_spatial_page_changes() {
    let types = fixture_registry();
    let mut store = EntityStore::new(types.clone());
    let checkpoint = encode_checkpoint(&store).unwrap();
    let transaction = store
        .prepare_spawn_batch(vec![
            EntitySpawn::Mobile {
                entity_type: DROP_TYPE,
                position: [1.0, 2.0, 3.0],
                payload: stack_payload(4, 1, &[]),
                spawn_tick: 1,
            },
            EntitySpawn::Mobile {
                entity_type: DROP_TYPE,
                position: [1.5, 2.0, 3.0],
                payload: stack_payload(4, 2, &[]),
                spawn_tick: 1,
            },
            EntitySpawn::Mobile {
                entity_type: PLAYER_TYPE,
                position: [2.0, 2.0, 3.0],
                payload: EntityPayload::new([1u8, 2, 3, 4]),
                spawn_tick: 1,
            },
        ])
        .unwrap();
    let ids = transaction.entity_ids();
    assert_eq!(ids.iter().map(|id| id.get()).collect::<Vec<_>>(), [1, 2, 3]);
    assert_eq!(
        transaction
            .changes()
            .iter()
            .filter(|change| change.key.domain == "bloxgloom:entity_allocator")
            .count(),
        1
    );
    assert_eq!(
        transaction
            .changes()
            .iter()
            .filter(|change| change.key.domain == "bloxgloom:entity_chunk")
            .count(),
        1
    );
    assert_eq!(
        transaction
            .changes()
            .iter()
            .filter(|change| change.key.domain == ENTITY_MOTION_DOMAIN)
            .count(),
        3
    );
    let overlay = transaction
        .changes()
        .iter()
        .map(|change| (change.key.clone(), change.after.clone()))
        .collect::<BTreeMap<_, _>>();

    let commit = store.apply_committed(transaction).unwrap();
    assert_eq!(commit.deltas.len(), 3);
    assert_eq!(store.next_id(), 4);
    assert_eq!(store.ids_for_chunk(ChunkKey { x: 0, y: 0, z: 0 }), ids);
    assert_eq!(
        store
            .public_views_for_chunk(ChunkKey { x: 0, y: 0, z: 0 })
            .len(),
        3
    );

    let mut replayed = decode_checkpoint(&checkpoint, types).unwrap();
    assert!(replayed.apply_journal_overlay(&overlay).unwrap());
    assert_eq!(replayed.next_id(), 4);
    assert_eq!(replayed.ids_for_chunk(ChunkKey { x: 0, y: 0, z: 0 }), ids);
}

#[test]
fn spawn_batch_rejects_oversized_wal_before_reserving_ids() {
    let mut store = EntityStore::new(fixture_registry());
    let payload = vec![7; 1_024];
    let spawns = (0..1_024)
        .map(|index| EntitySpawn::Mobile {
            entity_type: DROP_TYPE,
            position: [index as f32, 1.0, 1.0],
            payload: stack_payload(4, 1, &payload),
            spawn_tick: 1,
        })
        .collect();
    assert_eq!(
        store.prepare_spawn_batch(spawns).unwrap_err(),
        EntityError::TransactionTooLarge
    );
    assert_eq!(store.len(), 0);
    assert_eq!(store.next_id(), 1);
}

#[test]
fn mixed_entity_batch_coalesces_updates_transfer_despawn_and_spawn_atomically() {
    let types = fixture_registry();
    let mut store = EntityStore::new(types.clone());
    let initial = store
        .prepare_spawn_batch(vec![
            EntitySpawn::Mobile {
                entity_type: DROP_TYPE,
                position: [2.0, 4.0, 6.0],
                payload: stack_payload(4, 7, &[1]),
                spawn_tick: 10,
            },
            EntitySpawn::Mobile {
                entity_type: DROP_TYPE,
                position: [15.5, 4.0, 6.0],
                payload: stack_payload(4, 8, &[2]),
                spawn_tick: 10,
            },
            EntitySpawn::Mobile {
                entity_type: DROP_TYPE,
                position: [3.0, 4.0, 6.0],
                payload: stack_payload(4, 9, &[3]),
                spawn_tick: 10,
            },
        ])
        .unwrap();
    store.apply_committed(initial).unwrap();
    let checkpoint = encode_checkpoint(&store).unwrap();

    let update = store
        .prepare_update(
            EntityId::new(1).unwrap(),
            1,
            EntityPatch {
                payload: Some(stack_payload(4, 12, &[1])),
                ..Default::default()
            },
        )
        .unwrap();
    let transfer = store
        .prepare_transfer(EntityId::new(2).unwrap(), 1, [16.25, 4.0, 6.0])
        .unwrap();
    let despawn = store.prepare_despawn(EntityId::new(3).unwrap(), 1).unwrap();
    let spawn = store
        .prepare_spawn_batch(vec![EntitySpawn::Mobile {
            entity_type: DROP_TYPE,
            position: [16.5, 4.0, 6.0],
            payload: stack_payload(4, 10, &[4]),
            spawn_tick: 11,
        }])
        .unwrap();
    let batch = store
        .combine_prepared(vec![update, transfer, despawn, spawn])
        .unwrap();
    assert_eq!(
        batch.entity_ids(),
        (1..=4)
            .map(|id| EntityId::new(id).unwrap())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        batch
            .changes()
            .iter()
            .filter(|change| change.key.domain == "bloxgloom:entity_allocator")
            .count(),
        1
    );
    assert_eq!(
        batch
            .changes()
            .iter()
            .filter(|change| change.key.domain == "bloxgloom:entity_chunk")
            .count(),
        2
    );
    let keys: BTreeSet<_> = batch
        .changes()
        .iter()
        .map(|change| change.key.clone())
        .collect();
    assert_eq!(keys.len(), batch.changes().len());

    // A checkpoint-owned motion update after prepare must survive the delayed
    // payload receipt in the combined WAL batch.
    store
        .update_mobile_motion(EntityId::new(1).unwrap(), 1, [2.75, 4.5, 6.25])
        .unwrap();
    store.validate_prepared(&batch).unwrap();
    let overlay = batch
        .changes()
        .iter()
        .map(|change| (change.key.clone(), change.after.clone()))
        .collect::<BTreeMap<_, _>>();
    let commit = store.apply_committed(batch).unwrap();
    assert_eq!(commit.deltas.len(), 4);
    assert_eq!(store.len(), 3);
    assert_eq!(store.next_id(), 5);
    assert_eq!(
        store.owner(EntityId::new(2).unwrap()),
        Some(EntityOwner::Mobile(ChunkKey { x: 1, y: 0, z: 0 }))
    );
    assert_eq!(store.public_view(EntityId::new(3).unwrap()), None);
    assert_eq!(
        store
            .public_view(EntityId::new(1).unwrap())
            .unwrap()
            .location,
        EntityLocation::Mobile {
            position: [2.75, 4.5, 6.25]
        }
    );
    assert_eq!(
        store
            .snapshot(EntityId::new(1).unwrap())
            .unwrap()
            .private_payload
            .downcast_ref::<StackPayload>()
            .unwrap()
            .count,
        12
    );

    let mut replayed = decode_checkpoint(&checkpoint, types).unwrap();
    assert!(replayed.apply_journal_overlay(&overlay).unwrap());
    assert_eq!(replayed.len(), 3);
    assert_eq!(replayed.next_id(), 5);
    assert_eq!(replayed.public_view(EntityId::new(3).unwrap()), None);
    assert_eq!(
        replayed
            .public_view(EntityId::new(1).unwrap())
            .unwrap()
            .location,
        EntityLocation::Mobile {
            position: [2.0, 4.0, 6.0]
        }
    );
}

#[test]
fn owner_transfer_fences_motion_and_wal_replay_merges_by_motion_revision() {
    let types = fixture_registry();
    let mut store = EntityStore::new(types.clone());
    let spawn = spawn_drop(&store, [15.5, -0.5, -0.5]);
    let id = spawn.entity_id();
    store.apply_committed(spawn).unwrap();
    store
        .update_mobile_motion(id, 1, [15.75, -0.5, -0.5])
        .unwrap();

    let before_transfer = encode_checkpoint(&store).unwrap();
    let transfer = store.prepare_transfer(id, 1, [16.25, -0.5, -0.5]).unwrap();
    assert!(
        transfer
            .changes()
            .iter()
            .any(|change| change.key.domain == ENTITY_MOTION_DOMAIN)
    );
    assert_eq!(
        store.update_mobile_motion(id, 2, [15.9, -0.5, -0.5]),
        Err(EntityError::MotionFenced)
    );
    let mut mirror = decode_checkpoint(&before_transfer, types.clone()).unwrap();
    mirror.apply_committed_mirror(transfer.clone()).unwrap();
    assert_eq!(
        mirror.owner(id),
        Some(EntityOwner::Mobile(ChunkKey { x: 1, y: -1, z: -1 }))
    );

    let mut stale_mirror = decode_checkpoint(&before_transfer, types.clone()).unwrap();
    stale_mirror
        .update_mobile_motion(id, 2, [15.9, -0.5, -0.5])
        .unwrap();
    assert!(matches!(
        stale_mirror.apply_committed_mirror(transfer.clone()),
        Err(EntityError::StaleMotionRevision { .. })
    ));
    assert_eq!(
        stale_mirror.owner(id),
        Some(EntityOwner::Mobile(ChunkKey { x: 0, y: -1, z: -1 }))
    );
    let overlay = transfer
        .changes()
        .iter()
        .map(|change| (change.key.clone(), change.after.clone()))
        .collect::<BTreeMap<_, _>>();

    let mut replayed = decode_checkpoint(&before_transfer, types.clone()).unwrap();
    assert!(replayed.apply_journal_overlay(&overlay).unwrap());
    assert_eq!(
        replayed.owner(id),
        Some(EntityOwner::Mobile(ChunkKey { x: 1, y: -1, z: -1 }))
    );
    assert_eq!(
        replayed.public_view(id).unwrap().location,
        EntityLocation::Mobile {
            position: [16.25, -0.5, -0.5]
        }
    );

    store.apply_committed(transfer).unwrap();
    store
        .update_mobile_motion(id, 3, [16.5, -0.5, -0.5])
        .unwrap();
    let later_checkpoint = encode_checkpoint(&store).unwrap();
    let mut recovered = decode_checkpoint(&later_checkpoint, types).unwrap();
    assert!(!recovered.apply_journal_overlay(&overlay).unwrap());
    assert_eq!(
        recovered.public_view(id).unwrap().location,
        EntityLocation::Mobile {
            position: [16.5, -0.5, -0.5]
        }
    );
    assert_eq!(recovered.public_view(id).unwrap().motion_revision, 4);
}

#[test]
fn anchored_footprint_indexes_both_sides_of_negative_chunk_seam_atomically() {
    let catalog = Catalog::builtins();
    let mut store = EntityStore::new(fixture_registry());
    assert!(matches!(
        store.types().descriptor(KILN_TYPE).unwrap().ownership(),
        EntityOwnership::Anchored { .. }
    ));
    let anchor = CellCoord::new(-1, 15, -1);
    let mut payload = KilnPayload::new(KilnFacing::East);
    payload = plan_insert(
        &payload,
        KilnSlot::Input,
        &crate::inventory::Stack::new(crate::items::ItemId(4), 13),
        &catalog,
    )
    .unwrap()
    .payload;
    payload = plan_insert(
        &payload,
        KilnSlot::Fuel,
        &crate::inventory::Stack::new(crate::items::ItemId(9), 1),
        &catalog,
    )
    .unwrap()
    .payload;
    let spawn = payload.clone().spawn(anchor, 2, &catalog).unwrap();
    let footprint = kiln_footprint(anchor).unwrap();
    assert_eq!(footprint, vec![anchor, CellCoord::new(-1, 16, -1)]);
    let prepared = store.prepare_spawn(spawn).unwrap();
    let id = prepared.entity_id();
    assert!(
        prepared
            .changes()
            .iter()
            .any(|change| change.key.domain == "bloxgloom:entity_cell")
    );
    store.apply_committed(prepared).unwrap();
    let lower = ChunkKey { x: -1, y: 0, z: -1 };
    let upper = ChunkKey { x: -1, y: 1, z: -1 };
    for chunk in [lower, upper] {
        let views = store.public_views_for_chunk(chunk);
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].id, id);
        assert_eq!(views[0].payload.len(), 3);
        assert_eq!(views[0].payload[0], 1);
        assert_eq!(views[0].payload[1], 0);
        assert_eq!(views[0].payload[2], 0);
        assert!(!views[0].payload.windows(2).any(|window| window == [4, 0]));
    }
    assert_eq!(
        store
            .snapshot(id)
            .unwrap()
            .private_payload
            .downcast_ref::<KilnPayload>(),
        Some(&payload)
    );

    let overlap_anchor = CellCoord::new(-1, 16, -1);
    let overlap = store.prepare_spawn(
        KilnPayload::new(KilnFacing::North)
            .spawn(overlap_anchor, 3, &catalog)
            .unwrap(),
    );
    assert!(matches!(overlap, Err(EntityError::FootprintOverlap(_))));

    let remove = store.prepare_despawn(id, 1).unwrap();
    assert_eq!(
        remove
            .changes()
            .iter()
            .filter(|change| change.key.domain == "bloxgloom:entity_chunk")
            .count(),
        2
    );
    store.apply_committed(remove).unwrap();
    assert!(store.public_views_for_chunk(lower).is_empty());
    assert!(store.public_views_for_chunk(upper).is_empty());
}

#[test]
fn checkpoint_round_trip_rebuilds_indexes_and_rejects_corruption_or_unknown_types() {
    let types = fixture_registry();
    let mut store = EntityStore::new(types.clone());
    store
        .apply_committed(spawn_drop(&store, [-16.0, 0.5, 16.0]))
        .unwrap();
    let encoded = encode_checkpoint(&store).unwrap();
    let restored = decode_checkpoint(&encoded, types.clone()).unwrap();
    assert_eq!(restored.next_id(), store.next_id());
    assert_eq!(restored.revision(), store.revision());
    assert_eq!(
        restored.public_views_for_chunk(ChunkKey { x: -1, y: 0, z: 1 }),
        store.public_views_for_chunk(ChunkKey { x: -1, y: 0, z: 1 })
    );

    let mut corrupt = encoded.clone();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    assert!(matches!(
        decode_checkpoint(&corrupt, types.clone()),
        Err(EntityError::CorruptCheckpoint)
    ));

    // BGEN v3 header (50 bytes), value length (4 bytes), then BGER type ID at +14.
    let mut unknown = encoded;
    let record_start = 54;
    unknown[record_start + 14..record_start + 18].copy_from_slice(&99u32.to_le_bytes());
    let record_len = u32::from_le_bytes(unknown[50..54].try_into().unwrap()) as usize;
    let record_end = record_start + record_len;
    let record_crc = super::codec::crc32(&unknown[record_start..record_end - 4]);
    unknown[record_end - 4..record_end].copy_from_slice(&record_crc.to_le_bytes());
    let outer_crc = super::codec::crc32(&unknown[..unknown.len() - 4]);
    let end = unknown.len();
    unknown[end - 4..].copy_from_slice(&outer_crc.to_le_bytes());
    assert!(matches!(
        decode_checkpoint(&unknown, types),
        Err(EntityError::UnknownRequiredType(
            crate::content::EntityTypeId(99)
        ))
    ));
}

#[test]
fn wal_overlay_rebuilds_sparse_indexes_and_is_idempotent() {
    let types = fixture_registry();
    let source = EntityStore::new(types.clone());
    let spawn = spawn_drop(&source, [15.5, -0.5, -0.5]);
    let id = spawn.entity_id();
    let overlay = spawn
        .changes()
        .iter()
        .map(|change| (change.key.clone(), change.after.clone()))
        .collect::<BTreeMap<_, _>>();

    let mut recovered = EntityStore::new(types);
    assert!(recovered.apply_journal_overlay(&overlay).unwrap());
    assert_eq!(recovered.next_id(), 2);
    assert_eq!(
        recovered.owner(id),
        Some(EntityOwner::Mobile(ChunkKey { x: 0, y: -1, z: -1 }))
    );
    assert_eq!(
        recovered.ids_for_chunk(ChunkKey { x: 0, y: -1, z: -1 }),
        vec![id]
    );
    assert!(!recovered.apply_journal_overlay(&overlay).unwrap());
}

#[test]
fn a_frozen_type_registry_requires_every_catalogued_type_and_valid_anchor_schema() {
    let mut catalog = Catalog::builtins();
    let mut builder = EntityTypeRegistryBuilder::new(&catalog);
    builder
        .register(EntityTypeRegistration {
            id: DROP_TYPE,
            ownership: EntityOwnership::Mobile,
            tick_policy: TickPolicy::Never,
            max_payload_bytes: 1_032,
            codec: Arc::new(StackPayloadCodec),
        })
        .unwrap();
    assert!(matches!(
        builder.freeze(),
        Err(EntityError::MissingTypeRegistration(PLAYER_TYPE))
    ));

    const TEST_ANCHORED_TYPE: crate::content::EntityTypeId = crate::content::EntityTypeId(4);
    catalog
        .register_entity_type(EntityTypeDef {
            id: TEST_ANCHORED_TYPE,
            key: "test:anchored".into(),
            schema_version: 1,
            schema_fingerprint: 1,
        })
        .unwrap();
    let mut builder = EntityTypeRegistryBuilder::new(&catalog);
    let bad_states = BTreeSet::from([BlockStateId(1_000_000)]);
    assert_eq!(
        builder.register(EntityTypeRegistration {
            id: TEST_ANCHORED_TYPE,
            ownership: EntityOwnership::Anchored {
                compatible_anchor_states: bad_states,
                max_footprint_cells: 2,
            },
            tick_policy: TickPolicy::Never,
            max_payload_bytes: 40,
            codec: Arc::new(AppearanceCodec),
        }),
        Err(EntityError::InvalidType)
    );
}

#[test]
fn mobile_queries_are_sparse_bounded_and_sorted_by_stable_id() {
    let mut store = EntityStore::new(fixture_registry());
    let first = spawn_drop(&store, [-16.0, 0.5, -0.5]);
    let first_id = first.entity_id();
    store.apply_committed(first).unwrap();
    let second = store
        .prepare_spawn(EntitySpawn::Mobile {
            entity_type: DROP_TYPE,
            position: [-15.0, 0.5, -0.5],
            payload: stack_payload(4, 1, &[]),
            spawn_tick: 10,
        })
        .unwrap();
    let second_id = second.entity_id();
    store.apply_committed(second).unwrap();
    assert_eq!(
        store
            .query_mobile_aabb([-17.0, 0.0, -1.0], [-14.0, 1.0, 0.0])
            .unwrap(),
        vec![first_id, second_id]
    );
    assert_eq!(
        store.query_mobile_aabb([-100_000.0; 3], [100_000.0; 3]),
        Err(EntityError::SpatialQueryTooBroad)
    );
}
