use super::*;
use crate::server::parallel::OwnerKey;
use crate::server::registry::SystemId;
use crate::world::ChunkKey;
use std::collections::BTreeMap;
use std::sync::Arc;

struct U64Codec;

impl OwnerValueCodec for U64Codec {
    fn decode(&self, payload: &[u8]) -> Result<OwnerData, OwnerCodecError> {
        if payload.len() != 8 {
            return Err(OwnerCodecError::InvalidData);
        }
        Ok(OwnerData::new(u64::from_le_bytes(
            payload.try_into().expect("checked length"),
        )))
    }

    fn encode(&self, value: &OwnerData) -> Result<Vec<u8>, OwnerCodecError> {
        value
            .get::<u64>()
            .map(|value| value.to_le_bytes().to_vec())
            .ok_or(OwnerCodecError::InvalidData)
    }
}

struct BytesCodec;

impl OwnerValueCodec for BytesCodec {
    fn decode(&self, payload: &[u8]) -> Result<OwnerData, OwnerCodecError> {
        Ok(OwnerData::new(payload.to_vec()))
    }

    fn encode(&self, value: &OwnerData) -> Result<Vec<u8>, OwnerCodecError> {
        value
            .get::<Vec<u8>>()
            .cloned()
            .ok_or(OwnerCodecError::InvalidData)
    }
}

struct MutableTemplateCodec(std::sync::atomic::AtomicBool);

impl OwnerValueCodec for MutableTemplateCodec {
    fn accepts_intents(&self) -> bool {
        true
    }

    fn intent_bootstrap(&self) -> Option<&[u8]> {
        Some(if self.0.load(std::sync::atomic::Ordering::Relaxed) {
            &[2]
        } else {
            &[1]
        })
    }

    fn decode(&self, payload: &[u8]) -> Result<OwnerData, OwnerCodecError> {
        BytesCodec.decode(payload)
    }

    fn encode(&self, value: &OwnerData) -> Result<Vec<u8>, OwnerCodecError> {
        BytesCodec.encode(value)
    }
}

#[test]
fn intent_bootstrap_template_is_frozen_when_store_is_constructed() {
    use bloxgloom_host_api::system::{IntentDelivery, IntentId, Owner};
    use std::sync::atomic::{AtomicBool, Ordering};

    let codec = Arc::new(MutableTemplateCodec(AtomicBool::new(false)));
    let id = system("test:bootstrap-frozen");
    let mut store = DurableOwnerStore::new(vec![
        OwnerSystemConfig::new(id.clone(), codec.clone(), 1, 1, OwnerPartition::Chunk).unwrap(),
    ])
    .unwrap();
    store
        .insert(&id, chunk(0), OwnerData::new(vec![0u8]))
        .unwrap();
    codec.0.store(true, Ordering::Relaxed);
    let mut wave = store
        .prepare(
            &id,
            vec![OwnerWrite::new(chunk(0), 0, OwnerData::new(vec![3u8]))],
        )
        .unwrap();
    store
        .prepare_intent_bootstraps(
            &mut wave,
            &[(
                chunk(1),
                IntentDelivery {
                    id: IntentId {
                        source: Owner::Chunk([0, 0, 0]),
                        revision: 1,
                        ordinal: 0,
                    },
                    produced_tick: 1,
                    payload: vec![9],
                },
            )],
        )
        .unwrap();
    store.commit(wave, receipt()).unwrap();
    assert_eq!(
        store.snapshot(&id, chunk(1)).unwrap().1.get::<Vec<u8>>(),
        Some(&vec![1])
    );
}

fn system(name: &str) -> SystemId {
    SystemId::new(name).unwrap()
}

fn chunk(x: i32) -> OwnerKey {
    OwnerKey::Chunk(ChunkKey { x, y: 0, z: 0 })
}

fn counter_store() -> DurableOwnerStore {
    DurableOwnerStore::new(vec![
        OwnerSystemConfig::new(
            system("test:counters"),
            Arc::new(U64Codec),
            1,
            8,
            OwnerPartition::Chunk,
        )
        .unwrap(),
    ])
    .unwrap()
}

fn receipt() -> OwnerWalReceipt {
    OwnerWalReceipt { sequence: 7 }
}

#[test]
fn stale_read_rejects_the_whole_wave_before_anything_applies() {
    let mut store = counter_store();
    let id = system("test:counters");
    store.insert(&id, chunk(0), OwnerData::new(10u64)).unwrap();
    store.insert(&id, chunk(1), OwnerData::new(20u64)).unwrap();

    let wave = store
        .prepare(
            &id,
            vec![
                OwnerWrite::new(chunk(0), 0, OwnerData::new(11u64)),
                OwnerWrite {
                    owner: chunk(1),
                    reads: vec![(chunk(1), 0), (chunk(0), 99)],
                    value: OwnerData::new(21u64),
                    due_tick: None,
                },
            ],
        )
        .unwrap_err();
    assert_eq!(
        wave,
        OwnerDurableError::StaleRevision {
            system: id.clone(),
            owner: chunk(0),
            expected: 99,
            actual: Some(0),
        }
    );
    assert_eq!(
        store.snapshot(&id, chunk(0)).unwrap().1.get::<u64>(),
        Some(&10)
    );
    assert_eq!(
        store.snapshot(&id, chunk(1)).unwrap().1.get::<u64>(),
        Some(&20)
    );
}

#[test]
fn commit_after_a_concurrent_wave_rejects_whole_and_applies_nothing() {
    let mut store = counter_store();
    let id = system("test:counters");
    store.insert(&id, chunk(0), OwnerData::new(10u64)).unwrap();
    store.insert(&id, chunk(1), OwnerData::new(20u64)).unwrap();

    let first = store
        .prepare(
            &id,
            vec![OwnerWrite::new(chunk(0), 0, OwnerData::new(11u64))],
        )
        .unwrap();
    let second = store
        .prepare(
            &id,
            vec![
                OwnerWrite::new(chunk(0), 0, OwnerData::new(12u64)),
                OwnerWrite::new(chunk(1), 0, OwnerData::new(22u64)),
            ],
        )
        .unwrap();
    assert_eq!(store.commit(second, receipt()).unwrap(), 2);

    // The first wave's before-values no longer match: nothing may apply.
    assert!(matches!(
        store.commit(first, receipt()),
        Err(OwnerDurableError::StaleRevision { .. })
    ));
    assert_eq!(store.revision(&id, chunk(0)), Some(1));
    assert_eq!(
        store.snapshot(&id, chunk(0)).unwrap().1.get::<u64>(),
        Some(&12)
    );
    assert_eq!(
        store.snapshot(&id, chunk(1)).unwrap().1.get::<u64>(),
        Some(&22)
    );
}

#[test]
fn dropped_prepared_wave_changes_nothing() {
    let mut store = counter_store();
    let id = system("test:counters");
    store.insert(&id, chunk(0), OwnerData::new(10u64)).unwrap();
    let prepared = store
        .prepare(
            &id,
            vec![OwnerWrite::new(chunk(0), 0, OwnerData::new(11u64))],
        )
        .unwrap();
    assert_eq!(prepared.changes().len(), 1);
    drop(prepared);
    assert_eq!(store.revision(&id, chunk(0)), Some(0));
    assert_eq!(
        store.snapshot(&id, chunk(0)).unwrap().1.get::<u64>(),
        Some(&10)
    );
}

#[test]
fn scheduling_is_proportional_to_due_owners_not_total_owners() {
    let mut store = counter_store();
    let id = system("test:counters");
    for x in 0..512 {
        store
            .insert(&id, chunk(x), OwnerData::new(x as u64))
            .unwrap();
    }
    // Only three owners carry a due tick; everything else is idle.
    for (x, due) in [(3, 9), (100, 9), (400, 12)] {
        let prepared = store
            .prepare(
                &id,
                vec![OwnerWrite::new(chunk(x), 0, OwnerData::new(x as u64)).scheduled(Some(due))],
            )
            .unwrap();
        store.commit(prepared, receipt()).unwrap();
    }

    let due_now: Vec<_> = store
        .due_entries(9, None, 16)
        .into_iter()
        .map(|(_, _, owner)| owner)
        .collect();
    assert_eq!(due_now, vec![chunk(3), chunk(100)]);

    // The counting probe: handlers run only for due owners.
    let mut handler_runs = 0usize;
    for (_, system, owner) in store.due_entries(9, None, 16) {
        let (_, value) = store.snapshot(&system, owner).unwrap();
        assert!(value.get::<u64>().is_some());
        handler_runs += 1;
    }
    assert_eq!(handler_runs, 2);
    assert_eq!(store.cell_count(), 512);

    // Cursor wrap: after the last due key, earlier due keys appear again.
    let wrapped: Vec<_> = store
        .due_entries(12, Some((12, id.clone(), chunk(400))), 16)
        .into_iter()
        .map(|(_, _, owner)| owner)
        .collect();
    assert_eq!(wrapped, vec![chunk(3), chunk(100), chunk(400)]);
}

#[test]
fn runnable_capture_feeds_one_due_owner_per_one_job_wave() {
    let mut store = counter_store();
    let id = system("test:counters");
    for x in 0..128 {
        store
            .insert(&id, chunk(x), OwnerData::new(x as u64))
            .unwrap();
        let prepared = store
            .prepare(
                &id,
                vec![OwnerWrite::new(chunk(x), 0, OwnerData::new(x as u64)).scheduled(Some(5))],
            )
            .unwrap();
        store.commit(prepared, receipt()).unwrap();
    }
    assert_eq!(store.ready_due.len(), 0);
    assert_eq!(store.runnable_from(&id, 4, None, 1), Vec::<OwnerKey>::new());
    assert_eq!(store.ready_due.len(), 0);
    assert_eq!(store.runnable_from(&id, 5, None, 1), vec![chunk(0)]);
    assert_eq!(store.ready_due.len(), 1);
    assert_eq!(
        store.runnable_from(&id, 5, Some(chunk(1)), 1),
        vec![chunk(1)]
    );
    assert_eq!(store.ready_due.len(), 2);
    assert_eq!(store.schedule_by_system.len(), 126);
    assert_eq!(
        store.runnable_from(&id, 5, Some(chunk(2)), 4),
        vec![chunk(2), chunk(3), chunk(4), chunk(5)]
    );
    assert_eq!(
        store.ready_due.len(),
        6,
        "feed/capture is bounded by the job allowance"
    );
    assert_eq!(store.schedule_by_system.len(), 122);
    assert_eq!(
        store.schedule.len(),
        128,
        "feeding never consumes persisted eligibility"
    );
}

#[test]
fn fed_deadlines_are_not_duplicated_and_replay_clears_stale_readiness() {
    let mut store = counter_store();
    let id = system("test:counters");
    store.insert(&id, chunk(0), OwnerData::new(0u64)).unwrap();
    let prepared = store
        .prepare(
            &id,
            vec![OwnerWrite::new(chunk(0), 0, OwnerData::new(1u64)).scheduled(Some(5))],
        )
        .unwrap();
    store.commit(prepared, receipt()).unwrap();
    assert_eq!(store.runnable_from(&id, 5, None, 8), vec![chunk(0)]);
    assert!(store.schedule_by_system.is_empty());
    // A deferral leaves one ready entry, not an endless feed loop or lost work.
    assert_eq!(store.runnable_from(&id, 5, None, 8), vec![chunk(0)]);
    let prepared = store
        .prepare(
            &id,
            vec![OwnerWrite::new(chunk(0), 1, OwnerData::new(2u64)).scheduled(Some(10))],
        )
        .unwrap();
    store.apply_replayed(prepared.changes()).unwrap();
    assert!(store.ready_due.is_empty());
    assert!(store.runnable_from(&id, 9, None, 8).is_empty());
    assert_eq!(store.runnable_from(&id, 10, None, 8), vec![chunk(0)]);
}

#[test]
fn byte_bound_is_enforced_at_insert_and_at_prepare_without_truncation() {
    let mut store = DurableOwnerStore::new(vec![
        OwnerSystemConfig::new(
            system("test:blobs"),
            Arc::new(BytesCodec),
            1,
            8,
            OwnerPartition::Chunk,
        )
        .unwrap(),
    ])
    .unwrap();
    let id = system("test:blobs");
    let big = OwnerData::new(vec![7u8; 16]);

    let error = store.insert(&id, chunk(0), big).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::WouldBlock);
    assert_ne!(error.kind(), ErrorKind::InvalidData);
    assert_eq!(store.revision(&id, chunk(0)), None);

    store
        .insert(&id, chunk(0), OwnerData::new(vec![1u8; 8]))
        .unwrap();
    let error = store
        .prepare(
            &id,
            vec![OwnerWrite::new(chunk(0), 0, OwnerData::new(vec![2u8; 16]))],
        )
        .unwrap_err();
    assert!(matches!(
        error,
        OwnerDurableError::ValueTooLarge {
            actual: 16,
            limit: 8,
            ..
        }
    ));
    assert_eq!(error.kind(), ErrorKind::WouldBlock);
    // Failedclosed: the stored value is exactly what was inserted.
    assert_eq!(
        store.snapshot(&id, chunk(0)).unwrap().1.get::<Vec<u8>>(),
        Some(&vec![1u8; 8])
    );
}

#[test]
fn capacity_defers_while_corruption_stops() {
    let mut store = DurableOwnerStore::new(vec![
        OwnerSystemConfig::new(
            system("test:blobs"),
            Arc::new(BytesCodec),
            1,
            8,
            OwnerPartition::Chunk,
        )
        .unwrap(),
    ])
    .unwrap();
    let id = system("test:blobs");
    store
        .insert(&id, chunk(0), OwnerData::new(vec![1u8; 8]))
        .unwrap();
    let too_many = OwnerDurableError::TooManyOwners { system: id.clone() };
    assert_eq!(too_many.kind(), ErrorKind::WouldBlock);
    assert_ne!(too_many.kind(), ErrorKind::InvalidData);

    // Genuine corruption reports InvalidData: the coordinator must stop.
    let mut latest = BTreeMap::new();
    let key = owner_state_key(&id, chunk(1));
    latest.insert(key, vec![0u8; 4]);
    let error = DurableOwnerStore::recover(
        vec![
            OwnerSystemConfig::new(
                system("test:blobs"),
                Arc::new(BytesCodec),
                1,
                8,
                OwnerPartition::Chunk,
            )
            .unwrap(),
        ],
        &latest,
    )
    .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidData);
}

#[test]
fn committed_waves_mark_active_and_update_the_due_index() {
    let mut store = counter_store();
    let id = system("test:counters");
    store.insert(&id, chunk(0), OwnerData::new(1u64)).unwrap();
    store.insert(&id, chunk(1), OwnerData::new(2u64)).unwrap();
    assert_eq!(store.active_len(), 2);
    assert_eq!(store.take_active(10).len(), 2);
    assert_eq!(store.active_len(), 0);

    let prepared = store
        .prepare(
            &id,
            vec![OwnerWrite::new(chunk(0), 0, OwnerData::new(3u64)).scheduled(Some(5))],
        )
        .unwrap();
    store.commit(prepared, receipt()).unwrap();
    // A scheduled replacement leaves the active lane and becomes eligible
    // through its persisted deadline instead.
    assert_eq!(store.active_len(), 0);
    assert_eq!(
        store.due_entries(5, None, 8),
        vec![(5, id.clone(), chunk(0))]
    );
}

#[test]
fn wake_fan_out_follows_declared_partitions() {
    let store = DurableOwnerStore::new(vec![
        OwnerSystemConfig::new(
            system("test:chunks"),
            Arc::new(U64Codec),
            1,
            8,
            OwnerPartition::Chunk,
        )
        .unwrap(),
        OwnerSystemConfig::new(
            system("test:entities"),
            Arc::new(U64Codec),
            1,
            8,
            OwnerPartition::Entity,
        )
        .unwrap(),
    ])
    .unwrap();
    let chunks = system("test:chunks");
    let entities = system("test:entities");
    assert!(store.accepts_owner(&chunks, chunk(0)));
    assert!(!store.accepts_owner(&chunks, OwnerKey::Entity(1)));
    assert!(store.accepts_owner(&entities, OwnerKey::Entity(1)));
    assert!(!store.accepts_owner(&entities, chunk(0)));
    // Unknown systems accept nothing: fan-out never flags where no
    // descriptor exists.
    assert!(!store.accepts_owner(&system("test:missing"), chunk(0)));
}
