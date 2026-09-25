//! Server-owned durable transaction staging and per-key reservations.
//!
//! Gameplay code builds a complete `CommitAction`; this module serializes its
//! exact current/after values and submits it to the bounded journal worker.
//! The simulation coordinator applies the action only after polling a durable
//! receipt.

use super::checkpoint::{CheckpointReceipt, CheckpointSubmitError, CheckpointWriter};
use super::drops::{DropPlan, Drops};
use super::effects::CellCoord;
use super::entities::{
    EntityCheckpointStore, EntityCommit, EntityId, EntityStore, EntityTypeRegistry,
    PreparedEntityBatch,
};
use super::entity_checkpoint::{CheckpointTicket, EntityCheckpointMirror, MirrorPermit};
use super::fire::{FireCheckpointStore, FireRecovered, FireSeed, FireTransaction};
use super::journal::{
    CommitReceipt, JournalWriter, RotateError, RotationReceipt, StateKey, SubmitError, Transaction,
};
use super::runtime::owner_durable::{DurableOwnerStore, OwnerSystemConfig};
use super::simulation::TickId;
use crate::inventory::{Inventory, InventoryStore};
use crate::protocol::{ClientMessage, DroppedItem};
use crate::world::{BlockId, ChunkKey, PreparedEdit, World};
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::io::{self, ErrorKind};
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

#[path = "durable/actions/mod.rs"]
pub(super) mod actions;
#[path = "durable/checkpoint.rs"]
mod checkpoint;
#[path = "durable/coordinator.rs"]
pub(super) mod coordinator;
#[path = "durable/entity_recovery.rs"]
mod entity_recovery;
#[path = "durable/fire.rs"]
pub(super) mod fire;
#[path = "durable/publication.rs"]
mod publication;
#[path = "durable/receipt.rs"]
mod receipt;
#[path = "durable/receipts.rs"]
mod receipts;
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
#[cfg(test)]
pub(super) use state::encode_action_receipt;
pub(super) use state::{
    action_changes, chunk_state_key, drops_checkpoint_key, inventory_state_key, is_checkpoint_key,
};

pub(super) const MAX_PENDING_DURABLE_ACTIONS: usize = 256;
pub(super) const MAX_DEFERRED_DURABLE_ACTIONS: usize = 256;
pub(super) const MAX_DIRTY_CHECKPOINT_KEYS: usize = 1_024;
pub(super) const MAX_DIRTY_CHECKPOINT_BYTES: usize = 128 * 1024 * 1024;
const CHECKPOINT_QUEUE_CAPACITY: usize = 16;
const CHECKPOINT_WORKERS: usize = 4;

pub(super) struct Durability {
    catalog: Arc<crate::content::Catalog>,
    pub(super) writer: JournalWriter,
    pub(super) next_id: u128,
    pub(super) inventory_overlay: HashMap<u128, Inventory>,
    /// Last WAL-committed inventory revision per profile, retained after its
    /// BGIN checkpoint so delayed socket reads cannot authorize stale joins.
    pub(super) inventory_revisions: HashMap<u128, u64>,
    pub(super) pending: Vec<PendingCommit>,
    pub(super) reserved: HashSet<StateKey>,
    pub(super) queued: VecDeque<DurableRequest>,
    /// In-memory round-robin cursor; resets on restart, while entity due
    /// times remain WAL-owned on each record.
    pub(super) entity_tick_cursor: Option<(u64, EntityId)>,
    /// Committed wake destinations waiting for the interaction/commit
    /// barrier. Transient scheduling state: dropping entries only delays the
    /// destination's own durable work, which stays on its persisted schedule.
    pub(super) pending_wakes: Vec<EntityId>,
    pub(super) retry_pickups: HashSet<u64>,
    pub(super) expire_queued: bool,
    pub(super) expire_again: bool,
    pub(super) publish_queue: Vec<PublishEffects>,
    pub(super) next_publish_commit_id: u64,
    pub(super) checkpoint_writer: CheckpointWriter,
    pub(super) dirty_checkpoints: HashMap<StateKey, DirtyCheckpoint>,
    pub(super) checkpoint_inflight: HashMap<StateKey, u64>,
    pub(super) fire_checkpoint_batch: Option<FireCheckpointBatch>,
    pub(super) next_checkpoint_revision: u64,
    pub(super) receipt_store: receipts::ReceiptStore,
    pub(super) fire_store: FireCheckpointStore,
    pub(super) entity_store: EntityCheckpointStore,
    pub(super) entity_mirror: EntityCheckpointMirror,
    pub(super) entity_checkpoint_ticket: Option<CheckpointTicket>,
    pub(super) receipt_ledgers: HashMap<u128, receipts::ReceiptLedger>,
    pub(super) pending_grants: HashSet<u128>,
    pub(super) ready_grants: HashMap<u128, u64>,
    /// One coalesced contiguous ACK per connected profile, independent of the
    /// gameplay command queue so a full window can always make progress.
    pub(super) pending_acks: HashMap<u128, (u64, u64)>,
    pub(super) rotation_requested: bool,
    pub(super) rotation_snapshot_ready: bool,
    pub(super) rotation_receipt: Option<Receiver<io::Result<RotationReceipt>>>,
    /// Isolated benchmark-only one-shot trigger; production leaves this None.
    pub(super) force_rotation_at_sequence: Option<u64>,
    pub(super) completed_rotations: u64,
    pub(super) failed: bool,
}

pub(super) struct DirtyCheckpoint {
    pub(super) revision: u64,
    pub(super) snapshot: Vec<u8>,
    pub(super) retry_after: Instant,
}

pub(super) struct FireCheckpointBatch {
    pub(super) revision: u64,
    pub(super) covered: Vec<(StateKey, u64)>,
}

pub(super) struct PendingCommit {
    pub(super) receiver: Receiver<io::Result<CommitReceipt>>,
    pub(super) submitted_at: Instant,
    pub(super) keys: Vec<StateKey>,
    checkpoint_sizes: HashMap<StateKey, usize>,
    pub(super) payload: PendingPayload,
    pub(super) entity_permit: Option<MirrorPermit>,
}

pub(super) enum PendingPayload {
    Action(CommitAction),
    Fire(FireTransaction),
}

#[derive(Clone)]
pub(super) struct CommitAction {
    pub(super) client_id: Option<u64>,
    pub(super) profile: Option<u128>,
    pub(super) action_id: Option<u128>,
    pub(super) receipt_value: Option<Vec<u8>>,
    pub(super) receipt_transition: Option<receipts::ReceiptTransition>,
    pub(super) inventory_before: Option<Vec<u8>>,
    pub(super) inventory: Option<Inventory>,
    pub(super) world_edits: Vec<PreparedEdit>,
    pub(super) drops: DropPlan,
    pub(super) deltas: Vec<BlockDelta>,
    pub(super) changed_cells: Vec<CellCoord>,
    pub(super) pickups: Vec<DroppedItem>,
    pub(super) fire_seed: Option<FireSeed>,
    pub(super) entities: Option<PreparedEntityBatch>,
    /// Routed wake destinations declared by an entity plan. Transient
    /// scheduling only: never part of the WAL change set, delivered as
    /// tick attempts at the commit barrier when this action applies.
    pub(super) entity_wakes: Vec<EntityId>,
}

impl CommitAction {
    fn receipt_only(transition: receipts::ReceiptTransition) -> Self {
        Self {
            client_id: None,
            profile: Some(transition.profile),
            action_id: None,
            receipt_value: None,
            receipt_transition: Some(transition),
            inventory_before: None,
            inventory: None,
            world_edits: Vec::new(),
            drops: Default::default(),
            deltas: Vec::new(),
            changed_cells: Vec::new(),
            pickups: Vec::new(),
            fire_seed: None,
            entities: None,
            entity_wakes: Vec::new(),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct BlockDelta {
    pub(super) key: ChunkKey,
    pub(super) version: u64,
    pub(super) local: [u8; 3],
    pub(super) block: BlockId,
}

pub(super) struct PublishEffects {
    pub(super) client_id: Option<u64>,
    pub(super) profile: Option<u128>,
    pub(super) action_id: Option<u128>,
    pub(super) accepted: bool,
    pub(super) reason: String,
    pub(super) inventory: Option<Inventory>,
    pub(super) deltas: Vec<BlockDelta>,
    pub(super) entity_commit: Option<EntityCommit>,
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
    EntityTick {
        id: EntityId,
    },
    /// Transient wake: run the destination's tick planner early. Unlike the
    /// due-scan `EntityTick`, a woken attempt runs even before the entity's
    /// persisted due time; the planner's durable work still commits through
    /// the normal path, so a dropped wake only delays that work.
    EntityWake {
        id: EntityId,
    },
}

#[derive(Debug)]
pub(super) enum StageError {
    Conflict,
    Full,
    Closed,
    Invalid(io::Error),
    IdExhausted,
}

impl Durability {
    /// Begin a new server-issued session, or return its WAL-synced epoch.
    /// The caller retries on later ticks while this returns `Ok(None)`.
    pub(super) fn request_epoch_grant(
        &mut self,
        profile: u128,
        tick: TickId,
    ) -> Result<Option<u64>, StageError> {
        if profile == 0 {
            return Err(StageError::Invalid(io::Error::new(
                ErrorKind::InvalidInput,
                "missing profile",
            )));
        }
        if let Some(epoch) = self.ready_grants.get(&profile) {
            return Ok(Some(*epoch));
        }
        if self.pending_grants.contains(&profile) || self.profile_pending(profile) {
            return Ok(None);
        }
        let before = self.receipt_ledger(profile);
        let after = before.grant_next_epoch().map_err(StageError::Invalid)?;
        let transition = receipts::ReceiptTransition::new(
            profile,
            &before,
            after,
            receipts::ReceiptEvent::EpochGrant,
        )
        .map_err(StageError::Invalid)?;
        match self.try_stage(tick, &CommitAction::receipt_only(transition), None, None)? {
            true => {
                self.pending_grants.insert(profile);
                Ok(None)
            }
            false => Err(StageError::Invalid(io::Error::other("empty epoch grant"))),
        }
    }

    pub(super) fn claim_epoch_grant(&mut self, profile: u128) -> Option<u64> {
        self.ready_grants.remove(&profile)
    }

    pub(super) fn stage_action_ack(
        &mut self,
        profile: u128,
        epoch: u64,
        through_seq: u64,
        tick: TickId,
    ) -> Result<bool, StageError> {
        if self.profile_pending(profile) {
            return Err(StageError::Conflict);
        }
        let before = self.receipt_ledger(profile);
        let Some(after) = before
            .acknowledge(epoch, through_seq)
            .map_err(StageError::Invalid)?
        else {
            return Ok(false);
        };
        let transition =
            receipts::ReceiptTransition::new(profile, &before, after, receipts::ReceiptEvent::Ack)
                .map_err(StageError::Invalid)?;
        self.try_stage(tick, &CommitAction::receipt_only(transition), None, None)
    }
    /// Opens and recovers all journal-backed after-values before the server can
    /// accept clients. Existing saves are decoded before any replay replacement.
    ///
    /// Owner cells recover from the same `server.wal` latest-values map as
    /// every other domain — one journal, one tail — through the descriptors
    /// in `owner_configs`. See `recovery` for why owner state needs no
    /// per-key checkpoint file.
    pub(super) fn open(
        root: &Path,
        world: &mut World,
        inventory_store: &InventoryStore,
        drops: &mut Drops,
        entity_types: Arc<EntityTypeRegistry>,
        owner_configs: Vec<OwnerSystemConfig>,
    ) -> io::Result<(Self, FireRecovered, EntityStore, DurableOwnerStore)> {
        recovery::open(
            root,
            world,
            inventory_store,
            drops,
            entity_types,
            owner_configs,
        )
    }

    /// Nonblocking stage: conflicts and queue pressure are explicit failures.
    /// Keys are reserved only after the writer accepts the complete transaction.
    pub(super) fn try_stage(
        &mut self,
        tick: TickId,
        action: &CommitAction,
        projected_drop_snapshot_size: Option<usize>,
        entity_permit: Option<MirrorPermit>,
    ) -> Result<bool, StageError> {
        if action.entities.is_some() != entity_permit.is_some() {
            return Err(StageError::Invalid(io::Error::new(
                ErrorKind::InvalidInput,
                "entity WAL admission requires one mirror reservation",
            )));
        }
        let changes = action_changes(action, &self.catalog).map_err(StageError::Invalid)?;
        let read_keys = action
            .entities
            .as_ref()
            .map(|entities| entities.read_keys().cloned().collect())
            .unwrap_or_default();
        self.try_stage_changes(
            tick,
            changes,
            read_keys,
            PendingPayload::Action(action.clone()),
            projected_drop_snapshot_size,
            entity_permit,
        )
    }

    fn try_stage_changes(
        &mut self,
        tick: TickId,
        changes: Vec<super::journal::Change>,
        read_keys: Vec<StateKey>,
        payload: PendingPayload,
        projected_drop_snapshot_size: Option<usize>,
        mut entity_permit: Option<MirrorPermit>,
    ) -> Result<bool, StageError> {
        if changes.is_empty() {
            return Ok(false);
        }
        if self.failed {
            return Err(StageError::Closed);
        }
        if self.rotation_requested {
            return Err(StageError::Full);
        }
        if self.pending.len() >= MAX_PENDING_DURABLE_ACTIONS {
            return Err(StageError::Full);
        }
        let mut keys = BTreeSet::new();
        for change in &changes {
            if self.reserved.contains(&change.key) {
                return Err(StageError::Conflict);
            }
            keys.insert(change.key.clone());
        }
        for key in read_keys {
            if self.reserved.contains(&key) {
                return Err(StageError::Conflict);
            }
            keys.insert(key);
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
        if let Some(permit) = &mut entity_permit {
            if let Err(error) = permit.mark_authoritative_change() {
                self.failed = true;
                return Err(StageError::Invalid(error));
            }
        }
        self.reserved.extend(keys.iter().cloned());
        self.next_id = next_id;
        self.pending.push(PendingCommit {
            receiver,
            submitted_at: Instant::now(),
            keys: keys.into_iter().collect(),
            checkpoint_sizes,
            payload,
            entity_permit,
        });
        Ok(true)
    }

    pub(super) fn profile_reserved(&self, profile: u128) -> bool {
        self.reserved.contains(&inventory_state_key(profile))
    }

    pub(super) fn profile_pending(&self, profile: u128) -> bool {
        self.pending
            .iter()
            .any(|commit| matches!(&commit.payload, PendingPayload::Action(action) if action.profile == Some(profile)))
    }

    pub(super) fn receipt_ledger(&self, profile: u128) -> receipts::ReceiptLedger {
        self.receipt_ledgers
            .get(&profile)
            .cloned()
            .unwrap_or_default()
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
