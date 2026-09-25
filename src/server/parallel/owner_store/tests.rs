use super::*;
use crate::server::parallel::{
    BatchId, JobCompletion, JobKey, JobOutcome, OwnerJob, OwnerPatch, OwnerResults,
    OwnerWaveLimits, PatchUsage, PhaseResults,
};
use crate::server::registry::SystemId;
use crate::server::simulation::{Phase, TickId};
use crate::world::ChunkKey;
use std::time::Duration;

fn chunk(x: i32) -> OwnerKey {
    OwnerKey::chunk(ChunkKey { x, y: 0, z: 0 })
}

fn wave(
    store: &OwnerStore<u64>,
    system: &SystemId,
    batch: BatchId,
    specifications: &[(OwnerKey, u64, &[(OwnerKey, u64)], u64)],
) -> ValidatedOwnerWave {
    let mut expected = Vec::new();
    let mut owners: Vec<OwnerResults<OwnerPatch, String>> = Vec::new();
    for (index, (owner, revision, neighbors, replacement)) in specifications.iter().enumerate() {
        let key = JobKey::new(batch, *owner, index as u64, *revision);
        let mut snapshots = vec![store.snapshot(*owner).unwrap()];
        snapshots.extend(
            neighbors
                .iter()
                .map(|(neighbor, _)| store.snapshot(*neighbor).unwrap()),
        );
        let job = OwnerJob::new(system.clone(), key, snapshots).unwrap();
        let patch = OwnerPatch::new(
            &job,
            *replacement,
            PatchUsage {
                writes: 1,
                effects: 0,
                estimated_bytes: std::mem::size_of::<u64>(),
            },
        );
        expected.push(key);
        owners.push(OwnerResults {
            owner: *owner,
            jobs: vec![JobCompletion {
                key,
                outcome: JobOutcome::Completed(patch),
            }],
        });
    }
    owners.reverse();
    let results = PhaseResults {
        batch,
        owners,
        worker_run_time: Duration::ZERO,
    };
    ValidatedOwnerWave::validate(
        system,
        &expected,
        results,
        batch,
        OwnerWaveLimits::new(16, 0, 1_024),
        |owner| store.revision(owner),
        |_| Ok(0),
        |_| Ok(()),
    )
    .unwrap()
}

#[test]
fn replacement_apply_preflights_the_whole_wave_and_advances_owner_revisions() {
    let negative = chunk(-1);
    let positive = chunk(0);
    let mut store = OwnerStore::new();
    store.insert(negative, 4, 10_u64).unwrap();
    store.insert(positive, 8, 20_u64).unwrap();
    let old_snapshot = store.snapshot(negative).unwrap();
    let system = SystemId::new("test:owner_store").unwrap();
    let batch = BatchId::new(TickId::new(6), Phase::Simulation, 0);
    let validated = wave(
        &store,
        &system,
        batch,
        &[
            (positive, 8, &[(negative, 4)], 21),
            (negative, 4, &[(positive, 8)], 11),
        ],
    );

    assert_eq!(store.apply_validated(validated).unwrap(), 2);
    assert_eq!(store.revision(negative), Some(5));
    assert_eq!(store.revision(positive), Some(9));
    assert_eq!(store.snapshot(negative).unwrap().value::<u64>(), Some(&11));
    assert_eq!(store.snapshot(positive).unwrap().value::<u64>(), Some(&21));
    assert_eq!(old_snapshot.value::<u64>(), Some(&10));
}

#[test]
fn stale_neighbor_after_validation_aborts_before_any_replacement() {
    let source = chunk(0);
    let neighbor = chunk(1);
    let mut store = OwnerStore::new();
    store.insert(source, 3, 7_u64).unwrap();
    store.insert(neighbor, 5, 9_u64).unwrap();
    let system = SystemId::new("test:owner_store").unwrap();
    let batch = BatchId::new(TickId::new(7), Phase::Simulation, 0);
    let source_wave = wave(&store, &system, batch, &[(source, 3, &[(neighbor, 5)], 8)]);
    let neighbor_wave = wave(&store, &system, batch, &[(neighbor, 5, &[], 10)]);
    store.apply_validated(neighbor_wave).unwrap();

    assert_eq!(
        store.apply_validated(source_wave),
        Err(OwnerStoreError::StaleRevision {
            owner: neighbor,
            expected: 5,
            actual: Some(6),
        })
    );
    assert_eq!(store.revision(source), Some(3));
    assert_eq!(store.snapshot(source).unwrap().value::<u64>(), Some(&7));
}

#[test]
fn snapshot_capture_sorts_and_rejects_duplicate_or_missing_owners() {
    let negative = chunk(-2);
    let positive = chunk(3);
    let mut store = OwnerStore::new();
    store.insert(negative, 0, 1_u64).unwrap();
    store.insert(positive, 0, 2_u64).unwrap();
    assert_eq!(
        store
            .snapshots([positive, negative])
            .unwrap()
            .iter()
            .map(OwnerSnapshot::owner)
            .collect::<Vec<_>>(),
        [negative, positive]
    );
    assert!(matches!(
        store.snapshots([negative, negative]),
        Err(OwnerStoreError::DuplicateOwner { owner }) if owner == negative
    ));
    assert!(matches!(
        store.snapshots([chunk(9)]),
        Err(OwnerStoreError::UnknownOwner { owner }) if owner == chunk(9)
    ));
}

#[test]
fn duplicate_insertion_does_not_replace_an_owner_value() {
    let owner = chunk(1);
    let mut store = OwnerStore::new();
    store.insert(owner, 0, 4_u64).unwrap();
    assert_eq!(
        store.insert(owner, 9, 6),
        Err(OwnerStoreError::DuplicateOwner { owner })
    );
    assert_eq!(store.snapshot(owner).unwrap().value::<u64>(), Some(&4));
}

#[test]
fn bounded_selection_wraps_and_resumes_after_a_missing_cursor() {
    let mut store = OwnerStore::new();
    for x in [1, 3, 5, 7] {
        store.insert(chunk(x), 0, x as u64).unwrap();
    }
    assert_eq!(store.owners_from(None, 2), [chunk(1), chunk(3)]);
    assert_eq!(store.successor(chunk(3)), Some(chunk(5)));
    assert_eq!(store.owners_from(Some(chunk(5)), 2), [chunk(5), chunk(7)]);
    assert_eq!(
        store.owners_from(Some(chunk(6)), 3),
        [chunk(7), chunk(1), chunk(3)]
    );
    assert_eq!(
        store.owners_from(Some(chunk(7)), 4),
        [chunk(7), chunk(1), chunk(3), chunk(5)]
    );
    assert!(store.owners_from(None, 0).is_empty());
}
