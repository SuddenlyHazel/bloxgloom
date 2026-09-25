//! Parallel owner-wave commits behind one publication barrier.
//!
//! Preparation and value encoding are pure CPU work: workers build validated
//! patches from immutable snapshots and never mutate the store or touch the
//! journal. This module holds the deterministic glue between that worker
//! phase and the single ordered publication:
//!
//! * [`OwnerWaveDurables`] bundles the wave-attached durable pieces that
//!   [`SystemRuntime::commit_owner_wave`](super::systems::SystemRuntime) used
//!   to take as six separate parameters.
//! * [`arbitrate_key_sets`] serializes only overlapping key sets: two
//!   prepared transactions with disjoint [`StateKey`]s never block each
//!   other, while two sharing a key serialize — exactly one wins, the other
//!   retries. Ordering is canonical (sorted keys, input wave order), so the
//!   outcome never depends on worker scheduling.
//! * [`build_owner_writes_parallel`] rebuilds the per-commit revision logs
//!   (the `OwnerWrite` vec publication consumes) on scoped worker threads,
//!   still collected in stable owner order behind the one barrier.
//! * Stage / receipt / apply stay split: the coordinator stages a complete
//!   transaction and may keep making progress while its fsync is in flight,
//!   but nothing becomes visible before its receipt.
//!
//! Built-in durable actions (block edits, pickups, kiln actions, admin
//! grants, drop removal) are NOT parallelized here: they stay one global
//! coordinator batch. This module covers owner slices only.

use super::super::journal::{Change, StateKey};
use super::super::parallel::{OwnerKey, OwnerPatch};
use super::super::registry::SystemId;
use super::super::simulation::TickId;
use super::owner_durable::{OwnerWrite, PreparedOwnerWave};
use super::owner_effects::OwnerEffectPatch;
use super::owner_wake::PreparedWakeSets;
use std::collections::BTreeSet;
use std::io;

/// Wave-attached durable pieces for one owner commit.
///
/// This bundles what `commit_owner_wave` used to take as six parameters
/// (`prepared`, `tick`, `durability`, `wake_sets`, `durable_served`,
/// `cursor`) into one struct. The journal handle (`&mut Durability`) is
/// still passed separately at the call site; everything attached to the wave
/// itself travels here.
pub(in crate::server) struct OwnerWaveDurables {
    pub prepared: PreparedOwnerWave,
    pub tick: TickId,
    pub wake_sets: PreparedWakeSets,
    pub durable_served: Vec<(SystemId, OwnerKey)>,
    pub cursor: Option<Change>,
}

impl OwnerWaveDurables {
    pub fn new(
        prepared: PreparedOwnerWave,
        tick: TickId,
        wake_sets: PreparedWakeSets,
        durable_served: Vec<(SystemId, OwnerKey)>,
        cursor: Option<Change>,
    ) -> Self {
        Self {
            prepared,
            tick,
            wake_sets,
            durable_served,
            cursor,
        }
    }
}

impl std::fmt::Debug for OwnerWaveDurables {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OwnerWaveDurables")
            .field("tick", &self.tick.get())
            .field("changes", &self.prepared.changes().len())
            .field("wake_sets", &self.wake_sets.changes().len())
            .field("served", &self.durable_served.len())
            .field("cursor", &self.cursor.is_some())
            .finish()
    }
}

/// Arbitration outcome for one prepared wave, in canonical wave order.
//
// Live dispatch still stages one wave at a time (overlap falls back to the
// reservation check in `stage_owner_wave`); multi-wave dispatch is the next
// item and will arbitrate through this type.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::server) enum WaveDisposition {
    /// No key overlaps any earlier wave: may stage alongside it.
    Commit,
    /// Shares at least one [`StateKey`] with an earlier wave: exactly one
    /// wins, this one retries after the winner's receipt.
    Retry { conflicts_with: usize },
}

/// Arbitrates prepared waves so only overlapping key sets serialize.
///
/// The input order is the canonical commit order (callers pass waves sorted
/// by system then owner); each wave's keys are compared as a sorted set, so
/// the dispositions are identical regardless of worker count or scheduling.
/// Disjoint waves all report [`WaveDisposition::Commit`]; a wave sharing any
/// key with an earlier committing wave reports [`WaveDisposition::Retry`]
/// against the earliest such wave. Retried waves never block disjoint ones:
/// arbitration is by construction, not by timing.
//
// Live dispatch still stages one wave at a time; multi-wave dispatch is the
// next item and will arbitrate through this function.
#[allow(dead_code)]
pub(in crate::server) fn arbitrate_key_sets(key_sets: &[Vec<StateKey>]) -> Vec<WaveDisposition> {
    let sorted: Vec<BTreeSet<&StateKey>> =
        key_sets.iter().map(|keys| keys.iter().collect()).collect();
    let mut dispositions = Vec::with_capacity(key_sets.len());
    let mut winners: Vec<BTreeSet<&StateKey>> = Vec::new();
    let mut winner_index: Vec<usize> = Vec::new();
    for (index, keys) in sorted.iter().enumerate() {
        let mut conflict = None;
        for (winner_slot, winner_keys) in winners.iter().enumerate() {
            if keys.intersection(winner_keys).next().is_some() {
                conflict = Some(winner_index[winner_slot]);
                break;
            }
        }
        match conflict {
            Some(first) => {
                dispositions.push(WaveDisposition::Retry {
                    conflicts_with: first,
                });
                let _ = index;
            }
            None => {
                dispositions.push(WaveDisposition::Commit);
                winners.push(keys.clone());
                winner_index.push(index);
            }
        }
    }
    dispositions
}

/// Collects one wave's full change key set in canonical order for
/// arbitration. Duplicate keys within one wave are collapsed: a transaction
/// never touches the same key twice.
//
// Live dispatch still stages one wave at a time; multi-wave dispatch is the
// next item and will collect key sets through this function.
#[allow(dead_code)]
pub(in crate::server) fn canonical_key_set<'a>(
    changes: impl Iterator<Item = &'a Change>,
) -> Vec<StateKey> {
    let set: BTreeSet<StateKey> = changes.map(|change| change.key.clone()).collect();
    set.into_iter().collect()
}

/// Rebuilds the per-commit revision logs (`OwnerWrite`s) on scoped worker
/// threads while preserving stable owner order.
///
/// Replacement extraction is a cheap serial pre-pass of pointer clones and
/// revision copies (patches carry a `Send`-only payload and cannot be shared
/// across threads). The revision-log fill itself — one `OwnerWrite` per
/// patch — then runs partitioned across scoped worker threads and is
/// collected in input (key-sorted) order, so the resulting writes, and
/// therefore the staged WAL changes, are byte-identical to the serial path
/// regardless of thread scheduling. A patch without a replacement is a
/// caller bug and fails the whole wave before anything is staged.
pub(in crate::server) fn build_owner_writes_parallel(
    patches: &[OwnerPatch],
    worker_chunks: usize,
) -> io::Result<Vec<OwnerWrite>> {
    if patches.is_empty() {
        return Ok(Vec::new());
    }
    let mut inputs = Vec::with_capacity(patches.len());
    for patch in patches {
        let Some(replacement) = OwnerEffectPatch::replacement(patch) else {
            return Err(io::Error::other(format!(
                "registered owner patch for {:?} has no replacement",
                patch.owner()
            )));
        };
        inputs.push((patch.owner(), patch.revisions().to_vec(), replacement));
    }
    let chunks = worker_chunks.max(1).min(inputs.len());
    let chunk_len = inputs.len().div_ceil(chunks);
    let mut ordered: Vec<Option<Vec<OwnerWrite>>> = Vec::with_capacity(chunks);
    ordered.resize_with(chunks, || None);
    std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(chunks);
        for input_chunk in inputs.chunks(chunk_len) {
            handles.push(scope.spawn(move || {
                let mut writes = Vec::with_capacity(input_chunk.len());
                for (owner, revisions, value) in input_chunk {
                    writes.push(OwnerWrite {
                        owner: *owner,
                        reads: revisions
                            .iter()
                            .map(|stamp| (stamp.owner, stamp.revision))
                            .collect(),
                        value: value.clone(),
                        due_tick: None,
                    });
                }
                writes
            }));
        }
        for (slot, handle) in ordered.iter_mut().zip(handles) {
            *slot = Some(
                handle
                    .join()
                    .map_err(|_| io::Error::other("owner revision-log worker panicked"))?,
            );
        }
        Ok::<(), io::Error>(())
    })?;
    Ok(ordered.into_iter().flatten().flatten().collect())
}

#[cfg(test)]
#[path = "owner_commit/tests.rs"]
mod tests;
