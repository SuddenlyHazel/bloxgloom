//! Post-WAL owner-local installation of already prepared chunk snapshots.
//!
//! The coordinator validates a whole disjoint batch before launching workers.
//! Each worker then checks and replaces only its own resident owner slot. No
//! worker touches cache LRU, load epochs, checkpoint overlays, or another
//! chunk's state; those metadata updates follow the complete worker barrier.

use super::cache::OwnerState;
use super::{ChunkKey, PreparedEdit, World};
use std::collections::HashSet;
use std::io;
use std::sync::atomic::Ordering;
use std::sync::{Arc, RwLock};

pub(crate) struct OwnerApplyTask {
    slot: Arc<RwLock<OwnerState>>,
    edit: PreparedEdit,
}

impl OwnerApplyTask {
    pub(crate) fn key(&self) -> ChunkKey {
        self.edit.key
    }

    pub(crate) fn expected_version(&self) -> u64 {
        self.edit.expected_version
    }

    /// Invoked by a fixed owner worker only after the matching WAL receipt.
    /// The pre-WAL worker already encoded the immutable `after_chunk`; this
    /// phase performs a checked owner-local swap, not duplicate cell work.
    pub(crate) fn run(self) -> io::Result<OwnerApplyReceipt> {
        let PreparedEdit {
            key,
            expected_version,
            new_version,
            after_snapshot,
            changed,
            expected_revision,
            revision,
            before_edits,
            after_chunk,
            after_edits,
            ..
        } = self.edit;
        let mut state = self
            .slot
            .write()
            .map_err(|_| io::Error::other("authoritative owner lock poisoned"))?;
        if state.chunk.version != expected_version
            || state.edits != before_edits
            || revision.load(Ordering::Acquire) != expected_revision
        {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "post-WAL owner no longer matches prepared edit",
            ));
        }
        if after_chunk.key != key || after_chunk.version != new_version {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "prepared owner after-chunk has wrong identity or version",
            ));
        }
        if changed {
            *state = OwnerState {
                chunk: Arc::new(after_chunk),
                edits: after_edits,
            };
        } else if new_version != expected_version || after_edits != before_edits {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unchanged prepared owner has changed after-state",
            ));
        }
        drop(state);
        Ok(OwnerApplyReceipt {
            key,
            new_version,
            after_snapshot,
            changed,
            slot: self.slot,
        })
    }
}

pub(crate) struct OwnerApplyReceipt {
    key: ChunkKey,
    new_version: u64,
    after_snapshot: Vec<u8>,
    changed: bool,
    slot: Arc<RwLock<OwnerState>>,
}

impl OwnerApplyReceipt {
    pub(crate) fn key(&self) -> ChunkKey {
        self.key
    }
}

impl World {
    pub(crate) fn owner_apply_capacity(&self) -> usize {
        self.cache.capacity()
    }

    /// Validate the complete disjoint write set against one pre-apply world
    /// snapshot, before any worker of a possibly partitioned batch runs.
    pub(crate) fn validate_owner_apply_batch(&self, edits: &[PreparedEdit]) -> io::Result<()> {
        if edits.len() > self.cache.capacity() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "post-WAL owner batch exceeds resident cache capacity",
            ));
        }
        let mut keys = HashSet::with_capacity(edits.len());
        for edit in edits {
            if !keys.insert(edit.key) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "duplicate owner in post-WAL apply batch",
                ));
            }
            self.validate_prepared_edit(edit)?;
            if edit.changed
                && self.in_flight_by_key.contains_key(&edit.key)
                && self.edit_epoch(edit.key) == u64::MAX
            {
                return Err(io::Error::other("chunk load epoch exhausted"));
            }
            if edit.changed && edit.revision.load(Ordering::Acquire) == u64::MAX {
                return Err(io::Error::other("prepared edit revision exhausted"));
            }
            if edit.before_chunk.key != edit.key
                || edit.before_chunk.version != edit.expected_version
                || edit.after_chunk.key != edit.key
                || edit.after_chunk.version != edit.new_version
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "prepared owner chunk identity or version mismatch",
                ));
            }
        }
        Ok(())
    }

    /// Validates every owner and captures disjoint resident slots before any
    /// post-WAL worker can mutate one. Missing cached owners are reconstructed
    /// from the immutable before-chunk retained by the prepared edit; their
    /// WAL record is already synced, but they stay at the old version until
    /// workers install the new state. Cache admission is bounded and touched
    /// owners cannot evict one another within a batch of at most capacity.
    pub(crate) fn prepare_owner_apply_batch(
        &mut self,
        edits: Vec<PreparedEdit>,
    ) -> io::Result<Vec<OwnerApplyTask>> {
        if edits.len() > self.cache.capacity() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "post-WAL owner batch exceeds resident cache capacity",
            ));
        }
        self.validate_owner_apply_batch(&edits)?;

        let mut tasks = Vec::with_capacity(edits.len());
        for edit in edits {
            if !self.cache.contains_key(&edit.key) {
                self.cache.insert(
                    edit.key,
                    Arc::clone(&edit.before_chunk),
                    edit.before_edits.clone(),
                );
            }
            let slot = Arc::clone(
                &self
                    .cache
                    .get_mut_and_touch(edit.key)
                    .expect("post-WAL owner was just admitted")
                    .state,
            );
            tasks.push(OwnerApplyTask { slot, edit });
        }
        if tasks.iter().any(|task| {
            self.cache
                .get(&task.key())
                .is_none_or(|entry| !Arc::ptr_eq(&entry.state, &task.slot))
        }) {
            return Err(io::Error::other(
                "post-WAL owner batch could not retain all resident slots",
            ));
        }
        Ok(tasks)
    }

    /// Called after every accepted owner worker reached its barrier. The
    /// revisions and checkpoint overlays then become visible to subsequent
    /// server phases, before any ordered gameplay effects are published.
    pub(crate) fn finish_owner_apply_batch(
        &mut self,
        receipts: Vec<OwnerApplyReceipt>,
    ) -> io::Result<Vec<(ChunkKey, u64)>> {
        let mut keys = HashSet::with_capacity(receipts.len());
        for receipt in &receipts {
            if !keys.insert(receipt.key)
                || self
                    .cache
                    .get(&receipt.key)
                    .is_none_or(|entry| !Arc::ptr_eq(&entry.state, &receipt.slot))
                || self.cached_version(receipt.key) != Some(receipt.new_version)
            {
                return Err(io::Error::other(
                    "post-WAL owner receipt is missing, duplicate, or stale",
                ));
            }
        }
        let mut versions = Vec::with_capacity(receipts.len());
        for receipt in receipts {
            versions.push((receipt.key, receipt.new_version));
            if receipt.changed {
                let edited = !self
                    .cache
                    .get(&receipt.key)
                    .unwrap()
                    .read()
                    .edits
                    .is_empty();
                self.sky_ceiling.update(receipt.key, edited);
                self.bump_edit_epoch(receipt.key)?;
                self.advance_prepared_revision(receipt.key)?;
                self.pending_snapshots.insert(
                    receipt.key,
                    (!receipt.after_snapshot.is_empty()).then_some(receipt.after_snapshot),
                );
            }
        }
        Ok(versions)
    }
}
