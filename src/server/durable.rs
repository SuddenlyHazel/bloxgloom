//! Server-owned durable transaction staging and per-key reservations.
//!
//! Gameplay code builds a complete `CommitAction`; this module serializes its
//! exact current/after values and submits it to the bounded journal worker.
//! The simulation coordinator applies the action only after polling a durable
//! receipt.

use super::checkpoint::{CheckpointReceipt, CheckpointSubmitError, CheckpointWriter};
use super::drops::{DropPlan, Drops};
use super::effects::CellCoord;
use super::journal::{
    CommitReceipt, JournalWriter, RotateError, RotationReceipt, StateKey, SubmitError, Transaction,
};
use super::simulation::TickId;
use crate::inventory::{Inventory, InventoryStore};
use crate::protocol::{ClientMessage, DroppedItem};
use crate::world::{Chunk, ChunkKey, PreparedEdit, World};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{self, ErrorKind};
use std::path::Path;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

#[path = "durable/actions.rs"]
pub(super) mod actions;
#[path = "durable/checkpoint.rs"]
mod checkpoint;
#[path = "durable/coordinator.rs"]
pub(super) mod coordinator;
#[path = "durable/publication.rs"]
mod publication;
#[path = "durable/receipt.rs"]
mod receipt;
#[path = "durable/recovery.rs"]
mod recovery;
#[path = "durable/rotation.rs"]
mod rotation;
#[path = "durable/state.rs"]
mod state;
pub(super) use checkpoint::remember_drops_checkpoint;
pub(super) use coordinator::{
    handle_live_message, process_durable_actions, queue_interaction_actions,
};
pub(super) use publication::publish_committed;
pub(super) use state::{
    action_changes, action_receipt_state_key, chunk_state_key, drops_checkpoint_key,
    encode_action_receipt, inventory_state_key, is_checkpoint_key,
};

pub(super) const MAX_PENDING_DURABLE_ACTIONS: usize = 256;
pub(super) const MAX_DEFERRED_DURABLE_ACTIONS: usize = 256;
pub(super) const MAX_DIRTY_CHECKPOINT_KEYS: usize = 1_024;
pub(super) const MAX_DIRTY_CHECKPOINT_BYTES: usize = 128 * 1024 * 1024;
// Receipt values remain replayable across restart; reject new client actions
// at this bound until a compact per-session receipt representation exists.
const MAX_ACTION_RECEIPTS: usize = 1_000_000;
const CHECKPOINT_QUEUE_CAPACITY: usize = 16;

fn action_receipt_limit_reached(committed: usize, pending: usize, limit: usize) -> bool {
    committed.saturating_add(pending) >= limit
}

pub(super) struct Durability {
    pub(super) writer: JournalWriter,
    pub(super) next_id: u128,
    pub(super) inventory_overlay: HashMap<u128, Inventory>,
    /// Last WAL-committed inventory revision per profile, retained after its
    /// BGIN checkpoint so delayed socket reads cannot authorize stale joins.
    pub(super) inventory_revisions: HashMap<u128, u64>,
    pub(super) pending: Vec<PendingCommit>,
    pub(super) reserved: HashSet<StateKey>,
    pub(super) queued: VecDeque<DurableRequest>,
    pub(super) retry_pickups: HashSet<u64>,
    pub(super) expire_queued: bool,
    pub(super) expire_again: bool,
    pub(super) publish_queue: Vec<PublishEffects>,
    pub(super) checkpoint_writer: CheckpointWriter,
    pub(super) dirty_checkpoints: HashMap<StateKey, DirtyCheckpoint>,
    pub(super) checkpoint_inflight: HashMap<StateKey, u64>,
    pub(super) next_checkpoint_revision: u64,
    pub(super) action_receipts: HashMap<(u128, u128), Vec<u8>>,
    pub(super) rotation_requested: bool,
    pub(super) rotation_snapshot_ready: bool,
    pub(super) rotation_receipt: Option<Receiver<io::Result<RotationReceipt>>>,
    pub(super) failed: bool,
}

pub(super) struct DirtyCheckpoint {
    pub(super) revision: u64,
    pub(super) snapshot: Vec<u8>,
    pub(super) retry_after: Instant,
}

pub(super) struct PendingCommit {
    pub(super) receiver: Receiver<io::Result<CommitReceipt>>,
    pub(super) submitted_at: Instant,
    pub(super) keys: Vec<StateKey>,
    checkpoint_sizes: HashMap<StateKey, usize>,
    pub(super) action: CommitAction,
}

#[derive(Clone)]
pub(super) struct CommitAction {
    pub(super) client_id: Option<u64>,
    pub(super) profile: Option<u128>,
    pub(super) action_id: Option<u128>,
    pub(super) receipt_value: Option<Vec<u8>>,
    pub(super) inventory_before: Option<Vec<u8>>,
    pub(super) inventory: Option<Inventory>,
    pub(super) world_edits: Vec<PreparedEdit>,
    pub(super) drops: DropPlan,
    pub(super) deltas: Vec<BlockDelta>,
    pub(super) changed_cells: Vec<CellCoord>,
    pub(super) pickups: Vec<DroppedItem>,
}

#[derive(Clone, Copy)]
pub(super) struct BlockDelta {
    pub(super) key: ChunkKey,
    pub(super) version: u64,
    pub(super) local: [u8; 3],
    pub(super) block: u8,
}

pub(super) struct PublishEffects {
    pub(super) client_id: Option<u64>,
    pub(super) profile: Option<u128>,
    pub(super) action_id: Option<u128>,
    pub(super) accepted: bool,
    pub(super) reason: String,
    pub(super) inventory: Option<Inventory>,
    pub(super) chunks: Vec<Chunk>,
    pub(super) deltas: Vec<BlockDelta>,
    pub(super) pickups: Vec<DroppedItem>,
}

#[derive(Clone)]
pub(super) enum DurableRequest {
    Command {
        id: u64,
        message: ClientMessage,
        queued_at: Instant,
    },
    Pickup {
        id: u64,
    },
    Expire,
}

#[derive(Debug)]
pub(super) enum StageError {
    Conflict,
    Full,
    ReceiptLimit,
    Closed,
    Invalid(io::Error),
    IdExhausted,
}

impl Durability {
    /// Opens and recovers all journal-backed after-values before the server can
    /// accept clients. Existing saves are decoded before any replay replacement.
    pub(super) fn open(
        root: &Path,
        world: &mut World,
        inventory_store: &InventoryStore,
        drops: &mut Drops,
    ) -> io::Result<Self> {
        recovery::open(root, world, inventory_store, drops)
    }

    /// Nonblocking stage: conflicts and queue pressure are explicit failures.
    /// Keys are reserved only after the writer accepts the complete transaction.
    pub(super) fn try_stage(
        &mut self,
        tick: TickId,
        action: &CommitAction,
        projected_drop_snapshot_size: Option<usize>,
    ) -> Result<bool, StageError> {
        let changes = action_changes(action).map_err(StageError::Invalid)?;
        if changes.is_empty() {
            return Ok(false);
        }
        if self.failed {
            return Err(StageError::Closed);
        }
        if self.rotation_requested {
            return Err(StageError::Full);
        }
        // Reserve capacity for every in-flight durable action before accepting
        // another receipt-bearing request. `pending.len()` is intentionally a
        // conservative O(1) upper bound; otherwise a full WAL batch could push
        // the persisted receipt count over the startup limit before receipts
        // are polled into `action_receipts`.
        if action.action_id.is_some()
            && action_receipt_limit_reached(
                self.action_receipts.len(),
                self.pending.len(),
                MAX_ACTION_RECEIPTS,
            )
        {
            return Err(StageError::ReceiptLimit);
        }
        if self.pending.len() >= MAX_PENDING_DURABLE_ACTIONS {
            return Err(StageError::Full);
        }
        let mut keys = Vec::with_capacity(changes.len());
        for change in &changes {
            if self.reserved.contains(&change.key) {
                return Err(StageError::Conflict);
            }
            keys.push(change.key.clone());
        }
        let drops_key = drops_checkpoint_key();
        let mut projected_checkpoint_keys: HashSet<StateKey> =
            self.dirty_checkpoints.keys().cloned().collect();
        let mut projected_checkpoint_bytes: HashMap<StateKey, usize> = self
            .dirty_checkpoints
            .iter()
            .map(|(key, checkpoint)| (key.clone(), checkpoint.snapshot.len()))
            .collect();
        for pending in &self.pending {
            for (key, size) in &pending.checkpoint_sizes {
                projected_checkpoint_keys.insert(key.clone());
                let entry = projected_checkpoint_bytes.entry(key.clone()).or_default();
                *entry = (*entry).max(*size);
            }
        }
        for change in &changes {
            if is_checkpoint_key(&change.key) {
                projected_checkpoint_keys.insert(change.key.clone());
                projected_checkpoint_bytes.insert(change.key.clone(), change.after.len());
            }
        }
        if let Some(size) = projected_drop_snapshot_size {
            projected_checkpoint_keys.insert(drops_key.clone());
            let entry = projected_checkpoint_bytes
                .entry(drops_key.clone())
                .or_default();
            *entry = (*entry).max(size);
        }
        if projected_checkpoint_keys.len() > MAX_DIRTY_CHECKPOINT_KEYS {
            return Err(StageError::Full);
        }
        let projected_bytes = projected_checkpoint_bytes
            .values()
            .copied()
            .fold(0usize, usize::saturating_add);
        if projected_bytes > MAX_DIRTY_CHECKPOINT_BYTES {
            return Err(StageError::Full);
        }

        let mut checkpoint_sizes: HashMap<StateKey, usize> = changes
            .iter()
            .filter(|change| is_checkpoint_key(&change.key))
            .map(|change| (change.key.clone(), change.after.len()))
            .collect();
        if let Some(size) = projected_drop_snapshot_size {
            checkpoint_sizes.insert(drops_key, size);
        }

        let id = self.next_id;
        let next_id = id.checked_add(1).ok_or(StageError::IdExhausted)?;
        let transaction = Transaction::new(id, tick.get(), changes);
        let receiver = self
            .writer
            .try_submit(transaction)
            .map_err(|error| match error {
                SubmitError::Full => StageError::Full,
                SubmitError::Closed => StageError::Closed,
                SubmitError::Invalid(error) => StageError::Invalid(error),
            })?;
        self.reserved.extend(keys.iter().cloned());
        self.next_id = next_id;
        self.pending.push(PendingCommit {
            receiver,
            submitted_at: Instant::now(),
            keys,
            checkpoint_sizes,
            action: action.clone(),
        });
        Ok(true)
    }

    pub(super) fn profile_reserved(&self, profile: u128) -> bool {
        self.reserved.contains(&inventory_state_key(profile))
    }

    pub(super) fn profile_pending(&self, profile: u128) -> bool {
        self.pending
            .iter()
            .any(|commit| commit.action.profile == Some(profile))
    }

    pub(super) fn action_receipt(&self, profile: u128, action_id: u128) -> Option<&[u8]> {
        self.action_receipts
            .get(&(profile, action_id))
            .map(Vec::as_slice)
    }

    pub(super) fn action_receipt_reserved(&self, profile: u128, action_id: u128) -> bool {
        self.reserved
            .contains(&action_receipt_state_key(profile, action_id))
    }

    pub(super) fn poll_checkpoints(&mut self) -> Vec<CheckpointReceipt> {
        let mut completed = Vec::new();
        while let Ok(receipt) = self.checkpoint_writer.try_recv() {
            self.checkpoint_inflight.remove(&receipt.key);
            if let (Some(dirty), Err(_)) = (
                self.dirty_checkpoints.get_mut(&receipt.key),
                &receipt.result,
            ) {
                dirty.retry_after = Instant::now() + Duration::from_secs(1);
            }
            completed.push(receipt);
        }
        completed
    }

    pub(super) fn checkpoint_committed(
        &mut self,
        key: &StateKey,
        revision: u64,
    ) -> Option<Vec<u8>> {
        if self
            .dirty_checkpoints
            .get(key)
            .is_some_and(|dirty| dirty.revision == revision)
        {
            self.dirty_checkpoints
                .remove(key)
                .map(|dirty| dirty.snapshot)
        } else {
            None
        }
    }

    pub(super) fn remember_checkpoint(&mut self, key: StateKey, snapshot: Vec<u8>) {
        let revision = self.next_checkpoint_revision;
        self.next_checkpoint_revision = self
            .next_checkpoint_revision
            .checked_add(1)
            .expect("checkpoint revision exhausted");
        self.dirty_checkpoints.insert(
            key,
            DirtyCheckpoint {
                revision,
                snapshot,
                retry_after: Instant::now(),
            },
        );
    }

    pub(super) fn submit_checkpoint(
        &mut self,
        key: StateKey,
        write: impl FnOnce(&[u8]) -> io::Result<()> + Send + 'static,
    ) -> Result<bool, CheckpointSubmitError> {
        if self.checkpoint_inflight.contains_key(&key) {
            return Ok(false);
        }
        let Some(dirty) = self.dirty_checkpoints.get(&key) else {
            return Ok(false);
        };
        if dirty.retry_after > Instant::now() {
            return Ok(false);
        }
        let revision = dirty.revision;
        let snapshot = dirty.snapshot.clone();
        self.checkpoint_writer
            .try_submit(key.clone(), revision, snapshot, write)?;
        self.checkpoint_inflight.insert(key, revision);
        Ok(true)
    }
}
