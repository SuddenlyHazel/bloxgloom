//! WAL-backed owner runtime: the same append/fsync/replay path as entities.
//!
//! A prepared owner wave is submitted as one journal [`Transaction`]; the
//! commit applies to memory only after the worker's [`CommitReceipt`]
//! arrives. A crash between append and apply replays to the last complete
//! record on [`DurableOwnerRuntime::open`], matching the guarantees
//! `journal.rs` already gives everything else. Nothing becomes visible
//! before its receipt.
//!
//! Journal backpressure (`SubmitError::Full`) maps to `WouldBlock`: one wave
//! defers and the coordinator keeps running. Oversized waves are rejected up
//! front for the same reason; only journal validation failures keep their
//! `InvalidData` kind.

use super::super::journal::{CommitReceipt, Journal, JournalWriter, SubmitError, Transaction};
use super::super::parallel::{OwnerData, OwnerKey};
use super::super::registry::SystemId;
use super::owner_codec::OWNER_STATE_DOMAIN;
use super::owner_durable::{
    DurableOwnerStore, OwnerDurableError, OwnerSystemConfig, OwnerWalReceipt, OwnerWrite,
    PreparedOwnerWave,
};
use std::collections::BTreeMap;
use std::io::{self, ErrorKind};
use std::path::Path;
use std::sync::mpsc::Receiver;
use std::time::Duration;

/// Fail-closed bound for one owner-wave WAL record. Waves that would exceed
/// it defer with `WouldBlock` instead of pressing the journal's record
/// limit; genuine journal validation failures still report `InvalidData`.
pub(in crate::server) const MAX_OWNER_WAVE_BYTES: usize = 512 * 1024;

/// Pending WAL submission for one prepared wave. The receipt arrives only
/// after the complete record is synced; dropping the staged wave without
/// committing aborts it cleanly.
pub(in crate::server) struct StagedOwnerWave {
    prepared: PreparedOwnerWave,
    receiver: Receiver<io::Result<CommitReceipt>>,
}

impl std::fmt::Debug for StagedOwnerWave {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StagedOwnerWave")
            .field("writes", &self.prepared.changes().len())
            .finish()
    }
}

/// Durable owner state with a dedicated WAL tail. One runtime owns one
/// `owner.wal` file; commits stay coordinator-ordered through this handle.
pub(in crate::server) struct DurableOwnerRuntime {
    store: DurableOwnerStore,
    writer: JournalWriter,
    next_id: u128,
}

impl DurableOwnerRuntime {
    /// Opens (or creates) the owner WAL and replays the last complete
    /// record per key. Torn trailing frames are discarded by the journal;
    /// complete records replay exactly, so recovered state is the last
    /// acknowledged receipt's state — no more, no less.
    pub fn open(dir: &Path, configs: Vec<OwnerSystemConfig>) -> io::Result<Self> {
        let journal = Journal::open(dir.join("owner.wal"))?;
        let latest = journal.latest_values();
        let owner_latest: BTreeMap<_, _> = latest
            .into_iter()
            .filter(|(key, _)| key.domain == OWNER_STATE_DOMAIN)
            .collect();
        let store = DurableOwnerStore::recover(configs, &owner_latest)?;
        let next_id = journal.next_id()?;
        let writer = journal.into_writer(128, Duration::from_millis(3))?;
        Ok(Self {
            store,
            writer,
            next_id,
        })
    }

    pub fn revision(&self, system: &SystemId, owner: OwnerKey) -> Option<u64> {
        self.store.revision(system, owner)
    }

    pub fn snapshot(&self, system: &SystemId, owner: OwnerKey) -> Option<(u64, OwnerData)> {
        self.store.snapshot(system, owner)
    }

    pub fn cell_count(&self) -> usize {
        self.store.cell_count()
    }

    /// Inserts a new cell through the WAL: the cell is visible only after
    /// its receipt commits. Oversized values report `WouldBlock`.
    pub fn insert(
        &mut self,
        system: &SystemId,
        owner: OwnerKey,
        value: OwnerData,
        tick: u64,
    ) -> io::Result<()> {
        // Validate (including the byte bound) before spending a transaction
        // ID: a capacity rejection must not consume the ID space.
        let change = self
            .store
            .stage_insert(system, owner, &value)
            .map_err(OwnerDurableError::io)?;
        let id = self.claim_id()?;
        let receiver = self.submit(Transaction::new(id, tick, vec![change]))?;
        let receipt = receive(receiver)?;
        self.store
            .insert(system, owner, value)
            .map_err(OwnerDurableError::io)?;
        let _ = receipt.sequence;
        Ok(())
    }

    /// Prepares a wave and submits it without waiting: the caller must
    /// [`DurableOwnerRuntime::commit_staged`] the receipt before the writes
    /// are visible. Dropping the staged wave aborts it cleanly.
    pub fn stage(
        &mut self,
        system: &SystemId,
        writes: Vec<OwnerWrite>,
        tick: u64,
    ) -> io::Result<StagedOwnerWave> {
        let prepared = self
            .store
            .prepare(system, writes)
            .map_err(OwnerDurableError::io)?;
        let bytes: usize = prepared
            .changes()
            .iter()
            .map(|change| change.after.len())
            .sum();
        if bytes > MAX_OWNER_WAVE_BYTES {
            return Err(io::Error::new(
                ErrorKind::WouldBlock,
                format!("owner wave of {bytes} bytes exceeds {MAX_OWNER_WAVE_BYTES}"),
            ));
        }
        let id = self.claim_id()?;
        let transaction = Transaction::new(id, tick, prepared.changes().to_vec());
        let receiver = self.submit(transaction)?;
        Ok(StagedOwnerWave { prepared, receiver })
    }

    /// Waits for the staged wave's receipt, then commits it. The store
    /// rechecks every before-value, so a wave staged before a concurrent
    /// commit cannot half-apply.
    pub fn commit_staged(&mut self, staged: StagedOwnerWave) -> io::Result<usize> {
        let receipt = receive(staged.receiver)?;
        self.store
            .commit(
                staged.prepared,
                OwnerWalReceipt {
                    sequence: receipt.sequence,
                },
            )
            .map_err(OwnerDurableError::io)
    }

    /// One-step durable wave: stage, wait for the WAL receipt, commit.
    /// Nothing is visible before the receipt.
    pub fn commit_wave(
        &mut self,
        system: &SystemId,
        writes: Vec<OwnerWrite>,
        tick: u64,
    ) -> io::Result<usize> {
        let staged = self.stage(system, writes, tick)?;
        self.commit_staged(staged)
    }

    fn claim_id(&mut self) -> io::Result<u128> {
        let id = self.next_id;
        if id == 0 {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "owner transaction IDs exhausted",
            ));
        }
        self.next_id = id.checked_add(1).ok_or_else(|| {
            io::Error::new(ErrorKind::InvalidData, "owner transaction IDs exhausted")
        })?;
        Ok(id)
    }

    fn submit(&self, transaction: Transaction) -> io::Result<Receiver<io::Result<CommitReceipt>>> {
        self.writer
            .try_submit(transaction)
            .map_err(|error| match error {
                SubmitError::Full => io::Error::new(ErrorKind::WouldBlock, "owner journal is full"),
                SubmitError::Closed => io::Error::other("owner journal writer is closed"),
                SubmitError::Invalid(error) => error,
            })
    }
}

fn receive(receiver: Receiver<io::Result<CommitReceipt>>) -> io::Result<CommitReceipt> {
    receiver
        .recv()
        .map_err(|_| io::Error::other("owner journal worker stopped"))?
}

#[cfg(test)]
#[path = "owner_journal/tests.rs"]
mod tests;
