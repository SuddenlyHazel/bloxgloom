use super::super::{JobCompletion, OwnerResults};
use super::*;
use crate::server::registry::SystemId;
use crate::server::simulation::{Phase, TickId};
use crate::world::ChunkKey;
use std::collections::BTreeMap;
use std::time::Duration;

fn chunk(x: i32) -> OwnerKey {
    OwnerKey::chunk(ChunkKey { x, y: 0, z: 0 })
}

fn batch() -> BatchId {
    BatchId::new(TickId::new(5), Phase::Simulation, 2)
}

fn system() -> SystemId {
    SystemId::new("test:wave").unwrap()
}

fn patch(
    batch: BatchId,
    owner: OwnerKey,
    revision: u64,
    neighbors: &[(OwnerKey, u64)],
    effects: usize,
    bytes: usize,
    deliveries: usize,
) -> OwnerPatch {
    let key = JobKey::new(batch, owner, 0, revision);
    let mut snapshots = vec![OwnerSnapshot::new(owner, revision, Arc::new(()))];
    snapshots.extend(
        neighbors
            .iter()
            .map(|(neighbor, revision)| OwnerSnapshot::new(*neighbor, *revision, Arc::new(()))),
    );
    let job = OwnerJob::new(system(), key, snapshots).unwrap();
    OwnerPatch::new(
        &job,
        deliveries,
        PatchUsage {
            writes: 1,
            effects,
            estimated_bytes: bytes,
        },
    )
}

fn results(
    batch: BatchId,
    patches: impl IntoIterator<Item = OwnerPatch>,
) -> (PhaseResults<OwnerPatch, &'static str>, Vec<JobKey>) {
    let patches: Vec<_> = patches.into_iter().collect();
    let expected = patches.iter().map(OwnerPatch::key).collect();
    let mut owners: Vec<_> = patches
        .into_iter()
        .map(|patch| OwnerResults {
            owner: patch.owner(),
            jobs: vec![JobCompletion {
                key: patch.key(),
                outcome: JobOutcome::Completed(patch),
            }],
        })
        .collect();
    owners.reverse();
    (
        PhaseResults {
            batch,
            owners,
            worker_run_time: Duration::ZERO,
        },
        expected,
    )
}

#[test]
fn whole_wave_checks_neighbor_revisions_before_returning_any_apply_handle() {
    let source = chunk(-1);
    let neighbor = chunk(0);
    let patch = patch(batch(), source, 7, &[(neighbor, 4)], 1, 8, 2);
    let revisions = BTreeMap::from([(source, 7), (neighbor, 5)]);

    let (results, expected) = results(batch(), [patch]);
    let result = ValidatedOwnerWave::validate(
        &system(),
        &expected,
        results,
        batch(),
        OwnerWaveLimits::new(4, 8, 64),
        |owner| revisions.get(&owner).copied(),
        |patch| Ok(patch.payload::<usize>().copied().unwrap()),
        |_| Ok(()),
    );
    assert_eq!(
        result.unwrap_err(),
        OwnerWaveError::Stale {
            owner: neighbor,
            expected: 4,
            actual: Some(5),
        }
    );
}

#[test]
fn validated_wave_applies_in_canonical_owner_order_after_aggregate_checks() {
    let negative = chunk(-2);
    let positive = chunk(3);
    let patches = [
        patch(batch(), positive, 8, &[], 2, 10, 3),
        patch(batch(), negative, 2, &[], 1, 12, 4),
    ];
    let revisions = BTreeMap::from([(negative, 2), (positive, 8)]);
    let (results, expected) = results(batch(), patches);
    let wave = ValidatedOwnerWave::validate(
        &system(),
        &expected,
        results,
        batch(),
        OwnerWaveLimits::new(4, 8, 32),
        |owner| revisions.get(&owner).copied(),
        |patch| Ok(patch.payload::<usize>().copied().unwrap()),
        |patch| {
            if patch.usage().writes == 1 {
                Ok(())
            } else {
                Err("unexpected write count".into())
            }
        },
    )
    .unwrap();
    assert_eq!(wave.effect_deliveries(), 7);
    assert_eq!(wave.patch_bytes(), 22);

    let mut applied = Vec::new();
    assert_eq!(
        wave.apply(|patch| {
            applied.push((patch.owner(), patch.into_payload::<usize>().unwrap()));
        }),
        2
    );
    assert_eq!(applied, vec![(negative, 4), (positive, 3)]);
}

#[test]
fn handler_or_budget_failure_aborts_the_entire_wave() {
    let owner = chunk(0);
    let patch = patch(batch(), owner, 1, &[], 2, 8, 5);
    let revisions = BTreeMap::from([(owner, 1)]);
    assert!(matches!(
        ValidatedOwnerWave::validate(
            &system(),
            &[patch.key()],
            results(batch(), [patch]).0,
            batch(),
            OwnerWaveLimits::new(4, 4, 64),
            |owner| revisions.get(&owner).copied(),
            |patch| Ok(patch.payload::<usize>().copied().unwrap()),
            |_| Ok(()),
        ),
        Err(OwnerWaveError::PhaseEffectOverflow {
            actual: 5,
            limit: 4,
        })
    ));

    let key = JobKey::new(batch(), owner, 1, 1);
    let failed = PhaseResults {
        batch: batch(),
        owners: vec![OwnerResults {
            owner,
            jobs: vec![JobCompletion {
                key,
                outcome: JobOutcome::Failed("handler rejection"),
            }],
        }],
        worker_run_time: Duration::ZERO,
    };
    assert!(matches!(
        ValidatedOwnerWave::validate(
            &system(),
            &[key],
            failed,
            batch(),
            OwnerWaveLimits::new(4, 4, 64),
            |_| Some(1),
            |_| Ok(0),
            |_| Ok(()),
        ),
        Err(OwnerWaveError::HandlerFailed { .. })
    ));
}

#[test]
fn a_system_wave_cannot_commit_two_patches_for_the_same_owner() {
    let owner = chunk(0);
    let first = patch(batch(), owner, 1, &[], 0, 4, 0);
    let job = OwnerJob::new(
        system(),
        JobKey::new(batch(), owner, 1, 1),
        vec![OwnerSnapshot::new(owner, 1, Arc::new(()))],
    )
    .unwrap();
    let second = OwnerPatch::new(&job, 0_usize, PatchUsage::default());
    let revisions = BTreeMap::from([(owner, 1)]);
    assert!(matches!(
        ValidatedOwnerWave::validate(
            &system(),
            &[first.key(), second.key()],
            results(batch(), [first, second]).0,
            batch(),
            OwnerWaveLimits::new(4, 4, 64),
            |owner| revisions.get(&owner).copied(),
            |patch| Ok(patch.payload::<usize>().copied().unwrap()),
            |_| Ok(()),
        ),
        Err(OwnerWaveError::DuplicateOwner { owner: duplicate }) if duplicate == owner
    ));
}
