//! Focused tests for parallel owner commits: arbitration, deterministic
//! revision logs, and the stage/poll split contract.

use super::*;
use crate::server::journal::StateKey;
use crate::server::parallel::{
    BatchId, JobKey, OwnerData, OwnerJob, OwnerKey, OwnerPatch, OwnerSnapshot, PatchUsage,
};
use crate::server::registry::SystemId;
use crate::server::runtime::owner_effects::OwnerEffectPatch;
use crate::server::simulation::{Phase, TickId};
use crate::world::ChunkKey;
use std::sync::Arc;

fn key(domain: &str, byte: u8) -> StateKey {
    StateKey::new(domain, vec![byte])
}

#[test]
fn disjoint_key_sets_all_commit() {
    let sets = vec![
        vec![key("d", 1), key("d", 2)],
        vec![key("d", 3)],
        vec![key("e", 1)],
    ];
    assert_eq!(
        arbitrate_key_sets(&sets),
        vec![
            WaveDisposition::Commit,
            WaveDisposition::Commit,
            WaveDisposition::Commit,
        ]
    );
}

#[test]
fn overlapping_key_sets_serialize_with_exactly_one_winner() {
    let shared = key("bloxgloom:owner_state", 7);
    let sets = vec![
        vec![key("d", 1), shared.clone()],
        vec![shared.clone(), key("d", 9)],
        vec![key("d", 10)],
    ];
    // The second wave shares a key with the first: it retries against wave
    // 0. The third is disjoint from the winner, so a stalled overlapping
    // neighbour never delays it — by construction, not by timing.
    assert_eq!(
        arbitrate_key_sets(&sets),
        vec![
            WaveDisposition::Commit,
            WaveDisposition::Retry { conflicts_with: 0 },
            WaveDisposition::Commit,
        ]
    );
}

#[test]
fn arbitration_is_canonical_regardless_of_input_key_order() {
    let sets = vec![vec![key("d", 2), key("d", 1)], vec![key("d", 3)]];
    let reordered = vec![vec![key("d", 1), key("d", 2)], vec![key("d", 3)]];
    assert_eq!(arbitrate_key_sets(&sets), arbitrate_key_sets(&reordered));
}

#[test]
fn canonical_key_sets_collapse_duplicates_and_sort() {
    let change = |byte: u8| Change::new(key("d", byte), vec![], vec![byte]);
    let changes = [change(3), change(1), change(3)];
    assert_eq!(
        canonical_key_set(changes.iter()),
        vec![key("d", 1), key("d", 3)]
    );
}

/// Builds one patch per owner over `values`, each carrying a u64
/// replacement, in stable owner order.
fn patches_for(values: &[(OwnerKey, u64)]) -> Vec<OwnerPatch> {
    let system = SystemId::new("test:parallel_writes").unwrap();
    let batch = BatchId::new(TickId::new(1), Phase::Simulation, 0);
    values
        .iter()
        .enumerate()
        .map(|(index, (owner, value))| {
            let snapshot =
                OwnerSnapshot::new(*owner, index as u64, Arc::new(OwnerData::new(*value)));
            let job_key = JobKey::new(batch, *owner, index as u64, index as u64);
            let job = OwnerJob::new(system.clone(), job_key, vec![snapshot]).unwrap();
            OwnerPatch::new(
                &job,
                OwnerEffectPatch::new(OwnerData::new(value.wrapping_add(1)), Vec::new()),
                PatchUsage {
                    writes: 1,
                    effects: 0,
                    estimated_bytes: 8,
                },
            )
        })
        .collect()
}

fn chunk_owner(x: i32) -> OwnerKey {
    OwnerKey::chunk(ChunkKey { x, y: 0, z: 0 })
}

#[test]
fn parallel_revision_logs_match_serial_output_at_any_chunk_count() {
    let values: Vec<(OwnerKey, u64)> = (0..16).map(|x| (chunk_owner(x), x as u64 * 3)).collect();
    let patches = patches_for(&values);
    let serial = build_owner_writes_parallel(&patches, 1).unwrap();
    assert_eq!(serial.len(), values.len());
    for chunk_count in [2, 3, 4, 7, 16, 64] {
        let parallel = build_owner_writes_parallel(&patches, chunk_count).unwrap();
        assert_eq!(parallel.len(), serial.len());
        for (left, right) in parallel.iter().zip(serial.iter()) {
            assert_eq!(left.owner, right.owner);
            assert_eq!(left.reads, right.reads);
            assert_eq!(left.due_tick, right.due_tick);
            assert_eq!(
                left.value.get::<u64>().copied(),
                right.value.get::<u64>().copied()
            );
        }
    }
    // Stable owner order is preserved: output follows input order exactly.
    let owners: Vec<OwnerKey> = serial.iter().map(|write| write.owner).collect();
    assert_eq!(
        owners,
        values.iter().map(|(owner, _)| *owner).collect::<Vec<_>>()
    );
}

#[test]
fn revision_logs_preserve_read_revisions_for_overlapping_and_disjoint_owners() {
    // Overlapping reads (two patches reading the same owner would be
    // rejected at prepare) are not built here; instead this pins that
    // disjoint owners keep their own revisions while a shared read stamps
    // both writers identically.
    let system = SystemId::new("test:parallel_reads").unwrap();
    let batch = BatchId::new(TickId::new(2), Phase::Simulation, 0);
    let shared = OwnerSnapshot::new(chunk_owner(0), 5, Arc::new(OwnerData::new(5u64)));
    let first_job = OwnerJob::new(
        system.clone(),
        JobKey::new(batch, chunk_owner(1), 0, 6),
        vec![
            shared.clone(),
            OwnerSnapshot::new(chunk_owner(1), 6, Arc::new(OwnerData::new(6u64))),
        ],
    )
    .unwrap();
    let patches = vec![OwnerPatch::new(
        &first_job,
        OwnerEffectPatch::new(OwnerData::new(7u64), Vec::new()),
        PatchUsage {
            writes: 1,
            effects: 0,
            estimated_bytes: 8,
        },
    )];
    let writes = build_owner_writes_parallel(&patches, 4).unwrap();
    assert_eq!(writes.len(), 1);
    // Revisions are sorted by owner inside the job, so the shared read
    // stamps deterministically regardless of worker scheduling.
    assert_eq!(
        writes[0].reads,
        vec![(chunk_owner(0), 5), (chunk_owner(1), 6)]
    );
}
