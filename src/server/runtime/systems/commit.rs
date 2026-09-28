//! Owner-specific payload assembly and confirmed apply. Admission, receipt
//! ownership, reservation release and fatal gating belong to `durable`.
use super::*;
use crate::server::durable::StageError;
use crate::server::journal::CommitReceipt;
use crate::server::runtime::owner_commit::OwnerCommit;
use crate::server::runtime::owner_durable::{MAX_OWNER_WAVE_BYTES, OwnerWalReceipt};

impl SystemRuntime {
    /// Nonblocking submission. Every pre-admission rejection withdraws the
    /// wave's staged wake capacity so retry starts from confirmed state.
    pub(in crate::server) fn stage_owner_wave(
        &mut self,
        durables: OwnerWaveDurables,
        durability: &mut Durability,
    ) -> io::Result<StagedOwnerCommit> {
        let OwnerWaveDurables {
            prepared,
            tick,
            wake_sets,
            durable_served,
            cursor,
            live_wakes,
            terrain_reads,
            world_action,
        } = durables;
        let mut changes = prepared.changes().to_vec();
        changes.extend(wake_sets.changes().iter().cloned());
        changes.extend(self.durable_wakes.stage_clears(&durable_served));
        changes.extend(cursor.iter().cloned());
        if let Some(action) = &world_action {
            changes.extend(action.changes());
        }
        let commit = OwnerCommit {
            prepared,
            wake_sets,
            durable_served,
            cursor,
            live_wakes,
            tick,
            terrain_reads,
            world_action,
        };
        let bytes: usize = changes.iter().map(|change| change.after.len()).sum();
        if bytes > MAX_OWNER_WAVE_BYTES {
            self.cancel_owner_commit(commit);
            return Err(io::Error::new(
                ErrorKind::WouldBlock,
                format!("owner wave of {bytes} bytes exceeds {MAX_OWNER_WAVE_BYTES}"),
            ));
        }
        let keys = canonical_key_set(changes.iter());
        match durability.try_stage_owner(tick, changes, commit) {
            Ok(barrier) => Ok(StagedOwnerCommit { keys, barrier }),
            Err(rejected) => {
                let (error, commit) = *rejected;
                self.cancel_owner_commit(commit);
                match error {
                    StageError::Full | StageError::Conflict => Err(io::Error::new(
                        ErrorKind::WouldBlock,
                        "owner wave admission deferred",
                    )),
                    error => {
                        durability.failed = true;
                        Err(io::Error::other(format!(
                            "owner WAL admission failed: {error:?}"
                        )))
                    }
                }
            }
        }
    }

    /// Called only by the shared receipt gate. A failed apply is fatal until
    /// replay; no capacity check or retry may follow a WAL-confirmed mutation.
    pub(in crate::server) fn apply_receipted_owner_wave(
        &mut self,
        commit: OwnerCommit,
        receipt: CommitReceipt,
    ) -> io::Result<usize> {
        let OwnerCommit {
            wake_sets,
            durable_served,
            prepared,
            cursor,
            live_wakes,
            tick,
            terrain_reads: _,
            world_action: _,
        } = commit;
        self.staged_live_wakes -= live_wakes.len();
        let applied = match self.durable.commit(
            prepared,
            OwnerWalReceipt {
                sequence: receipt.sequence,
            },
        ) {
            Ok(applied) => applied,
            Err(error) => {
                self.durable_wakes.cancel_sets(wake_sets);
                return Err(error.io());
            }
        };
        self.durable_wakes.commit_sets(wake_sets);
        self.durable_wakes.commit_clears(&durable_served);
        if let Some(cursor) = cursor {
            self.apply_replayed_owner_changes(&[cursor])?;
        }
        for (system, owner) in live_wakes {
            self.pending_wakes
                .entry(system)
                .or_default()
                .entry(owner)
                .and_modify(|produced| *produced = (*produced).min(tick.get()))
                .or_insert(tick.get());
        }
        Ok(applied)
    }

    fn cancel_owner_commit(&mut self, commit: OwnerCommit) {
        self.staged_live_wakes -= commit.live_wakes.len();
        self.durable_wakes.cancel_sets(commit.wake_sets);
    }
}
