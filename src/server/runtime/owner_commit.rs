//! Parallel owner-wave commits behind one publication barrier.
//!
//! Preparation and value encoding are pure CPU work: workers build validated
//! patches from immutable snapshots and never mutate the store or touch the
//! journal. This module holds the deterministic glue between that worker
//! phase and the single ordered publication:
//!
//! * [`OwnerWaveDurables`] bundles the wave-attached durable preparation.
//!   Accepted [`OwnerCommit`] payloads live in the common durable queue.
//! * [`arbitrate_key_sets`] serializes only overlapping key sets: two
//!   prepared transactions with disjoint [`StateKey`]s never block each
//!   other, while two sharing a key serialize — exactly one wins, the other
//!   retries. Ordering is canonical (sorted keys, input wave order), so the
//!   outcome never depends on worker scheduling.
//! * [`build_owner_writes_parallel`] rebuilds the per-commit revision logs
//!   (the `OwnerWrite` vec publication consumes) on scoped worker threads,
//!   still collected in stable owner order behind the one barrier.
//! * Stage / receipt / apply stay split. The shared receipt gate applies in
//!   admission order; the coordinator waits at explicit logical boundaries.
//!
//! Built-in durable actions (block edits, pickups, kiln actions, admin
//! grants, drop removal) are NOT parallelized here: they stay one global
//! coordinator batch. This module covers owner slices only.

use super::super::journal::{Change, StateKey};
use super::super::parallel::{OwnerKey, OwnerPatch, OwnerSchedule};
use super::super::registry::SystemId;
use super::super::simulation::TickId;
use super::owner_durable::{OwnerWrite, PreparedOwnerWave};
use super::owner_effects::OwnerEffectPatch;
use super::owner_wake::PreparedWakeSets;
use crate::server::durable::{CommitAction, TerrainReads};
use std::collections::BTreeSet;
use std::io;

/// Specialized apply data owned by the common durable queue after admission.
/// No receipt or reservation is held by the feature runtime.
#[derive(Debug)]
pub(in crate::server) struct OwnerCommit {
    pub prepared: PreparedOwnerWave,
    pub wake_sets: PreparedWakeSets,
    pub durable_served: Vec<(SystemId, OwnerKey)>,
    pub cursor: Option<Change>,
    pub live_wakes: Vec<(SystemId, OwnerKey)>,
    pub tick: TickId,
    pub terrain_reads: TerrainReads,
    pub world_action: Option<OwnerWorldAction>,
}

/// World changes carried by the owner WAL record, not submitted as a second
/// action. Keep unrelated player/session fields out of the debug projection.
pub(in crate::server) struct OwnerWorldAction(pub CommitAction);

impl OwnerWorldAction {
    pub fn changes(&self) -> Vec<Change> {
        self.0
            .world_edits
            .iter()
            .filter(|edit| edit.changed)
            .map(|edit| {
                Change::new(
                    crate::server::durable::chunk_state_key(edit.key),
                    edit.before_snapshot.clone(),
                    edit.after_snapshot.clone(),
                )
            })
            .collect()
    }
}

impl std::fmt::Debug for OwnerWorldAction {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OwnerWorldAction")
            .field("chunks", &self.0.world_edits.len())
            .field("cells", &self.0.changed_cells.len())
            .finish()
    }
}

/// Wave-attached durable pieces for one owner commit.
///
/// State, deadline, durable wakes and cursor share one WAL record. Live wake
/// capacity is reserved during preparation but hints publish only on receipt,
/// tagged with their producing tick so later phase barriers cannot cascade.
pub(in crate::server) struct OwnerWaveDurables {
    pub prepared: PreparedOwnerWave,
    pub tick: TickId,
    pub wake_sets: PreparedWakeSets,
    pub durable_served: Vec<(SystemId, OwnerKey)>,
    pub cursor: Option<Change>,
    pub live_wakes: Vec<(SystemId, OwnerKey)>,
    pub terrain_reads: TerrainReads,
    pub world_action: Option<OwnerWorldAction>,
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
            live_wakes: Vec::new(),
            terrain_reads: TerrainReads::default(),
            world_action: None,
        }
    }

    pub fn with_live_wakes(mut self, wakes: Vec<(SystemId, OwnerKey)>) -> Self {
        self.live_wakes = wakes;
        self
    }

    pub fn with_terrain_reads(mut self, reads: TerrainReads) -> Self {
        self.terrain_reads = reads;
        self
    }

    pub fn with_world_action(mut self, action: Option<CommitAction>) -> Self {
        self.world_action = action.map(OwnerWorldAction);
        self
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
///
/// The live coordinator arbitrates each newly prepared wave against the
/// already-staged key sets ahead of it; the reservation check in
/// `stage_owner_wave` remains as the backstop for gameplay reservations.
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
        let due_tick = match patch.schedule() {
            OwnerSchedule::Active => None,
            OwnerSchedule::AtTick(tick) => Some(tick),
        };
        inputs.push((
            patch.owner(),
            patch.revisions().to_vec(),
            replacement,
            due_tick,
        ));
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
                for (owner, revisions, value, due_tick) in input_chunk {
                    writes.push(OwnerWrite {
                        owner: *owner,
                        reads: revisions
                            .iter()
                            .map(|stamp| (stamp.owner, stamp.revision))
                            .collect(),
                        value: value.clone(),
                        due_tick: *due_tick,
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
