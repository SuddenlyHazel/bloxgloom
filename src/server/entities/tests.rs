use super::*;
use crate::content::{BlockStateId, Catalog, EntityTypeDef};
use crate::server::entities::registry::EntityTypeRegistration;
use crate::world::{ChunkKey, WOOD};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

const DROP_TYPE: crate::content::EntityTypeId = crate::content::EntityTypeId(1);
const PLAYER_TYPE: crate::content::EntityTypeId = crate::content::EntityTypeId(2);
const KILN_TYPE: crate::content::EntityTypeId = crate::content::EntityTypeId(3);

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

struct KilnCodec;

#[derive(Clone, Debug, Eq, PartialEq)]
struct KilnPayload {
    facing: u8,
    lit: bool,
    fuel: u16,
    progress: u32,
    slots: [u16; 16],
}

impl EntityPayloadCodec for KilnCodec {
    fn decode(&self, payload: &[u8]) -> Result<EntityPayload, EntityCodecError> {
        if payload.len() != 40 || payload[0] > 3 || payload[1] > 1 {
            return Err(EntityCodecError::InvalidData);
        }
        let mut slots = [0; 16];
        for (index, slot) in payload[8..].chunks_exact(2).enumerate() {
            slots[index] = u16::from_le_bytes(slot.try_into().unwrap());
            if slots[index] > 128 {
                return Err(EntityCodecError::InvalidData);
            }
        }
        Ok(EntityPayload::new(KilnPayload {
            facing: payload[0],
            lit: payload[1] == 1,
            fuel: u16::from_le_bytes(payload[2..4].try_into().unwrap()),
            progress: u32::from_le_bytes(payload[4..8].try_into().unwrap()),
            slots,
        }))
    }

    fn encode(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        Ok(encode_kiln(
            payload
                .downcast_ref::<KilnPayload>()
                .ok_or(EntityCodecError::InvalidData)?,
        ))
    }

    fn public_view(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        let value = payload
            .downcast_ref::<KilnPayload>()
            .ok_or(EntityCodecError::InvalidData)?;
        Ok(vec![value.facing, u8::from(value.lit)])
    }
}

fn fixture_registry() -> Arc<EntityTypeRegistry> {
    let mut catalog = Catalog::builtins();
    catalog
        .register_entity_type(EntityTypeDef {
            id: KILN_TYPE,
            key: "bloxgloom:kiln".into(),
            schema_version: 1,
            schema_fingerprint: 0x4b49_4c4e_0000_0001,
        })
        .unwrap();
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
        (
            KILN_TYPE,
            EntityOwnership::anchored([WOOD], 8),
            TickPolicy::Interval(20),
            40,
            Arc::new(KilnCodec) as Arc<dyn EntityPayloadCodec>,
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

fn kiln_payload(
    facing: u8,
    lit: bool,
    fuel: u16,
    progress: u32,
    slots: [u16; 16],
) -> EntityPayload {
    EntityPayload::new(KilnPayload {
        facing,
        lit,
        fuel,
        progress,
        slots,
    })
}

fn encode_kiln(payload: &KilnPayload) -> Vec<u8> {
    let mut bytes = vec![payload.facing, u8::from(payload.lit)];
    bytes.extend(payload.fuel.to_le_bytes());
    bytes.extend(payload.progress.to_le_bytes());
    for count in payload.slots {
        bytes.extend(count.to_le_bytes());
    }
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
fn anchored_footprint_indexes_both_sides_of_negative_chunk_seam_atomically() {
    let mut store = EntityStore::new(fixture_registry());
    let anchor = CellCoord::new(15, -1, -1);
    let footprint = vec![anchor, CellCoord::new(16, -1, -1)];
    let mut slots = [0; 16];
    slots[0] = 13;
    let payload = kiln_payload(1, true, 600, 37, slots);
    let prepared = store
        .prepare_spawn(EntitySpawn::Anchored {
            entity_type: KILN_TYPE,
            anchor,
            anchor_state: WOOD,
            footprint: footprint.clone(),
            payload: payload.clone(),
            spawn_tick: 2,
        })
        .unwrap();
    let id = prepared.entity_id();
    assert!(
        prepared
            .changes()
            .iter()
            .any(|change| change.key.domain == "bloxgloom:entity_cell")
    );
    store.apply_committed(prepared).unwrap();
    let left = ChunkKey { x: 0, y: -1, z: -1 };
    let right = ChunkKey { x: 1, y: -1, z: -1 };
    for chunk in [left, right] {
        let views = store.public_views_for_chunk(chunk);
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].id, id);
        assert_eq!(views[0].payload, [1, 1]);
        assert!(!views[0].payload.windows(2).any(|window| window == [88, 2]));
    }
    assert_eq!(
        store
            .snapshot(id)
            .unwrap()
            .private_payload
            .downcast_ref::<KilnPayload>(),
        Some(&KilnPayload {
            facing: 1,
            lit: true,
            fuel: 600,
            progress: 37,
            slots,
        })
    );

    let overlap = store.prepare_spawn(EntitySpawn::Anchored {
        entity_type: KILN_TYPE,
        anchor: CellCoord::new(16, -1, -1),
        anchor_state: WOOD,
        footprint: vec![CellCoord::new(16, -1, -1)],
        payload: kiln_payload(0, false, 0, 0, [0; 16]),
        spawn_tick: 3,
    });
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
    assert!(store.public_views_for_chunk(left).is_empty());
    assert!(store.public_views_for_chunk(right).is_empty());
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

    // BGEN header (34 bytes), value length (4 bytes), then BGER type ID at +14.
    let mut unknown = encoded;
    let record_start = 38;
    unknown[record_start + 14..record_start + 18].copy_from_slice(&99u32.to_le_bytes());
    let record_len = u32::from_le_bytes(unknown[34..38].try_into().unwrap()) as usize;
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

    assert!(
        catalog
            .register_entity_type(EntityTypeDef {
                id: KILN_TYPE,
                key: "bloxgloom:kiln".into(),
                schema_version: 1,
                schema_fingerprint: 1,
            })
            .is_ok()
    );
    let mut builder = EntityTypeRegistryBuilder::new(&catalog);
    let bad_states = BTreeSet::from([BlockStateId(1_000_000)]);
    assert_eq!(
        builder.register(EntityTypeRegistration {
            id: KILN_TYPE,
            ownership: EntityOwnership::Anchored {
                compatible_anchor_states: bad_states,
                max_footprint_cells: 2,
            },
            tick_policy: TickPolicy::Never,
            max_payload_bytes: 40,
            codec: Arc::new(KilnCodec),
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
