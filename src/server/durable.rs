//! Server-owned durable transaction staging and per-key reservations.
//!
//! Gameplay code builds a complete `CommitAction`; this module serializes its
//! exact current/after values and submits it to the bounded journal worker.
//! The simulation coordinator applies the action only after polling a durable
//! receipt.

use super::checkpoint::{CheckpointReceipt, CheckpointSubmitError, CheckpointWriter};
use super::effects::CellCoord;
use super::entities::{
    EntityCommit, EntityId, EntityStore, EntityTypeRegistry, PreparedEntityBatch,
};
use super::entity_checkpoint::{CheckpointTicket, EntityCheckpointMirror, MirrorPermit};
use super::fire::{FireCheckpointStore, FireRecovered, FireSeed, FireTransaction};
use super::journal::{
    CommitReceipt, JournalWriter, RotateError, RotationReceipt, StateKey, SubmitError, Transaction,
};
use super::parallel::OwnerKey;
use super::registry::SystemId;
use super::runtime::owner_durable::{DurableOwnerStore, OwnerSystemConfig};
use super::runtime::owner_wake::PendingWakeStore;
use super::simulation::TickId;
use crate::inventory::{Inventory, InventoryStore};
use crate::protocol::{ClientMessage, DroppedItem};
use crate::world::{BlockId, ChunkKey, PreparedEdit, World};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::io::{self, ErrorKind};
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

#[path = "durable/actions/mod.rs"]
pub(super) mod actions;
mod admission;
mod terrain_reads;
pub(in crate::server) use terrain_reads::TerrainReads;
#[path = "durable/checkpoint.rs"]
mod checkpoint;
#[path = "durable/coordinator.rs"]
pub(super) mod coordinator;
mod entity_dispatch;
#[path = "durable/entity_recovery.rs"]
mod entity_recovery;
#[path = "durable/fire.rs"]
pub(super) mod fire;
mod owner;
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
pub(super) use coordinator::{
    handle_live_message, process_durable_actions, queue_interaction_actions,
};
pub(super) use publication::publish_committed;
#[cfg(test)]
pub(super) use receipt::poll_journal_receipts;
pub(super) use receipt::{CommitBarrier, complete_barrier};
#[cfg(test)]
pub(super) use state::encode_action_receipt;
pub(super) use state::{action_changes, chunk_state_key, inventory_state_key, is_checkpoint_key};

pub(super) const MAX_PENDING_DURABLE_ACTIONS: usize = 256;
pub(super) const MAX_DEFERRED_DURABLE_ACTIONS: usize = 256;
pub(super) const MAX_DURABLE_LANE_ACTIONS: usize = MAX_DEFERRED_DURABLE_ACTIONS / 2;
pub(super) const MAX_DIRTY_CHECKPOINT_KEYS: usize = 1_024;
pub(super) const MAX_DIRTY_CHECKPOINT_BYTES: usize = 128 * 1024 * 1024;
const CHECKPOINT_QUEUE_CAPACITY: usize = 16;
const CHECKPOINT_WORKERS: usize = 4;

pub(super) type RecoveredDurability = (
    Durability,
    FireRecovered,
    EntityStore,
    DurableOwnerStore,
    PendingWakeStore,
    BTreeMap<SystemId, OwnerKey>,
);

pub(super) struct Durability {
    pub(super) recovered_tick: u64,
    catalog: Arc<crate::content::Catalog>,
    pub(super) writer: JournalWriter,
    pub(super) next_id: u128,
    pub(super) inventory_overlay: HashMap<u128, Inventory>,
    /// Last WAL-committed inventory revision per profile, retained after its
    /// BGIN checkpoint so delayed socket reads cannot authorize stale joins.
    pub(super) inventory_revisions: HashMap<u128, u64>,
    /// Every accepted producer lives here until ordered apply. A fatal error
    /// quarantines the unapplied suffix and its permits; it cannot be polled
    /// again. On shutdown `writer` drops/joins before this queue is destroyed,
    /// completing or failing accepted writes on the one WAL. Recovery owns
    /// the valid tail, including records not applied in memory; no shutdown
    /// checkpoint fabricates visibility.
    pub(super) pending: Vec<PendingCommit>,
    pub(super) reserved: HashSet<StateKey>,
    /// Shared read reservations are included in `reserved`, so all existing
    /// write admission paths (including owner waves) still fence them.
    shared_reads: HashMap<StateKey, usize>,
    /// Last admitted entity publication watermark, including unapplied WAL
    /// records. This is ordering metadata, never speculative world state.
    entity_publication_frontier: (u64, u64),
    pub(super) queued: VecDeque<DurableRequest>,
    /// In-memory round-robin cursor; resets on restart, while entity due
    /// times remain WAL-owned on each record.
    pub(super) entity_tick_cursor: Option<(u64, EntityId)>,
    /// Circular suspended-record rechecks. Resets to the first live record on
    /// restart; eligibility itself is reconstructed by the entity indexes.
    pub(super) entity_sleep_cursor: Option<(EntityId, EntityId)>,
    /// Rotates scarce admission turns independently of tick parity, so even
    /// intermittent free capacity cannot repeatedly select the same lane.
    pub(super) entity_admission_turn: usize,
    /// Fixed 3:1 player/simulation first-turn policy for mixed admission waves.
    pub(super) response_turn: u8,
    pub(super) pickup_cursor: u64,
    /// Transient retry deadlines for views rejected at their fixed capture
    /// bound. The persisted due entry remains authoritative across restart.
    pub(super) oversized_entity_retry: BTreeMap<EntityId, u64>,
    /// Committed wake destinations waiting for the interaction/commit
    /// barrier. Transient scheduling state: dropping entries only delays the
    /// destination's own durable work, discoverable in the due/suspended index.
    pub(super) pending_wakes: Vec<EntityId>,
    pub(super) retry_pickups: HashSet<u64>,
    pub(super) expire_queued: bool,
    pub(super) expire_again: bool,
    pub(super) publish_queue: Vec<PublishEffects>,
    pub(super) next_publish_commit_id: u64,
    pub(super) checkpoint_writer: CheckpointWriter,
    pub(super) dirty_checkpoints: BTreeMap<StateKey, DirtyCheckpoint>,
    /// Transient round-robin traversal; restart reconstructs dirty state from
    /// WAL and starts at the first key. No persistence ordering depends on it.
    pub(super) checkpoint_cursor: Option<StateKey>,
    pub(super) checkpoint_inflight: HashMap<StateKey, u64>,
    pub(super) fire_checkpoint_batch: Option<FireCheckpointBatch>,
    pub(super) next_checkpoint_revision: u64,
    pub(super) receipt_store: receipts::ReceiptStore,
    pub(super) fire_store: FireCheckpointStore,
    #[cfg(test)]
    pub(super) entity_store: super::entities::EntityCheckpointStore,
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

impl Durability {
    /// Best-effort latency hint, never the source of entity work eligibility.
    /// The due/suspended indexes reconstruct necessary work after overflow,
    /// rejection, unload, or restart. Coalescing also bounds blocked retries.
    pub(super) fn hint_entity_wake(&mut self, id: EntityId) {
        if self.pending_wakes.len() < MAX_DEFERRED_DURABLE_ACTIONS
            && !self.pending_wakes.contains(&id)
        {
            self.pending_wakes.push(id);
        }
    }
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
    pub(super) id: u128,
    pub(super) receiver: Receiver<io::Result<CommitReceipt>>,
    pub(super) submitted_at: Instant,
    pub(super) keys: Vec<StateKey>,
    shared_read_keys: Vec<StateKey>,
    checkpoint_sizes: HashMap<StateKey, usize>,
    pub(super) payload: PendingPayload,
    pub(super) entity_permit: Option<MirrorPermit>,
}

#[allow(
    clippy::large_enum_variant,
    reason = "Pending commits are admission-bounded; preserve inline transaction ownership without adding per-action allocation."
)]
pub(super) enum PendingPayload {
    FireAction(crate::server::fire::FireTransaction, CommitAction),
    Action(CommitAction),
    Fire(FireTransaction),
    Owner(super::runtime::owner_commit::OwnerCommit),
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
    pub(super) terrain_reads: TerrainReads,
    pub(super) deltas: Vec<BlockDelta>,
    pub(super) changed_cells: Vec<CellCoord>,
    pub(super) pickups: Vec<DroppedItem>,
    pub(super) fire_seed: Option<FireSeed>,
    pub(super) clock_change: Option<crate::server::journal::Change>,
    pub(super) weather_change: Option<crate::server::journal::Change>,
    pub(super) entities: Option<PreparedEntityBatch>,
    /// Routed wake destinations declared by an entity plan. Transient
    /// scheduling only: never part of the WAL change set, delivered as
    /// tick attempts at the commit barrier when this action applies.
    pub(super) entity_wakes: Vec<EntityId>,
    /// Related owner cells and explicitly prepared profile inventory participants.
    pub(super) owner_changes: Vec<crate::server::journal::Change>,
    pub(super) player_publication: Option<super::players::Published>,
}

impl CommitAction {
    fn receipt_only(transition: receipts::ReceiptTransition) -> Self {
        Self {
            client_id: None,
            profile: Some(transition.profile),
            action_id: None,
            receipt_value: None,
            receipt_transition: Some(transition),
            terrain_reads: Default::default(),
            inventory_before: None,
            inventory: None,
            world_edits: Vec::new(),
            deltas: Vec::new(),
            changed_cells: Vec::new(),
            pickups: Vec::new(),
            fire_seed: None,
            clock_change: None,
            weather_change: None,
            entities: None,
            entity_wakes: Vec::new(),
            owner_changes: vec![],
            player_publication: None,
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
    pub(super) spawned: Vec<crate::protocol::SpawnReceipt>,
    pub(super) client_id: Option<u64>,
    pub(super) profile: Option<u128>,
    pub(super) action_id: Option<u128>,
    pub(super) accepted: bool,
    pub(super) reason: String,
    pub(super) inventory: Option<Inventory>,
    pub(super) deltas: Vec<BlockDelta>,
    pub(super) entity_commit: Option<EntityCommit>,
    pub(super) pickups: Vec<DroppedItem>,
    pub(super) fire_bursts: Vec<[i32; 3]>,
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
        match self.try_stage(tick, &CommitAction::receipt_only(transition), None)? {
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
        self.try_stage(tick, &CommitAction::receipt_only(transition), None)
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
        entity_types: Arc<EntityTypeRegistry>,
        owner_configs: Vec<OwnerSystemConfig>,
    ) -> io::Result<RecoveredDurability> {
        recovery::open(root, world, inventory_store, entity_types, owner_configs)
    }

    /// Nonblocking stage: conflicts and queue pressure are explicit failures.
    /// Keys are reserved only after the writer accepts the complete transaction.
    pub(super) fn try_stage(
        &mut self,
        tick: TickId,
        action: &CommitAction,
        entity_permit: Option<MirrorPermit>,
    ) -> Result<bool, StageError> {
        if action.entities.is_some() != entity_permit.is_some() {
            return Err(StageError::Invalid(io::Error::new(
                ErrorKind::InvalidInput,
                "entity WAL admission requires one mirror reservation",
            )));
        }
        if !action.terrain_reads.is_current() {
            return Err(StageError::Conflict);
        }
        let mut action = action.clone();
        if let Some(entities) = &mut action.entities {
            entities
                .assign_publication(self.entity_publication_frontier)
                .map_err(|error| StageError::Invalid(io::Error::other(error)))?;
        }
        let changes = action_changes(&action, &self.catalog).map_err(StageError::Invalid)?;
        let mut read_keys: Vec<_> = action
            .entities
            .as_ref()
            .map(|entities| entities.read_keys().cloned().collect())
            .unwrap_or_default();
        read_keys.extend(action.terrain_reads.keys());
        // Generic block edits also read anchored occupancy, including absence.
        // This protects them against pending footprint changes even if no
        // entity mutation belongs to this action.
        read_keys.extend(action.changed_cells.iter().map(|cell| {
            super::entities::cell_state_key(super::entities::CellCoord::new(cell.x, cell.y, cell.z))
        }));
        let frontier = action
            .entities
            .as_ref()
            .map(|entities| entities.publication_frontier())
            .transpose()
            .map_err(|error| StageError::Invalid(io::Error::other(error)))?;
        let staged = self.try_stage_changes(
            tick,
            changes,
            read_keys,
            &mut Some(PendingPayload::Action(action)),
            entity_permit,
        )?;
        if staged && let Some(frontier) = frontier {
            self.entity_publication_frontier = frontier;
        }
        Ok(staged)
    }

    pub(super) fn pending_profile_inserts(&self) -> usize {
        self.pending
            .iter()
            .map(|pending| match &pending.payload {
                PendingPayload::Action(action) | PendingPayload::FireAction(_, action) => action
                    .owner_changes
                    .iter()
                    .filter(|change| {
                        change.key.domain == super::runtime::owner_codec::OWNER_STATE_DOMAIN
                            && change.before.is_empty()
                    })
                    .count(),
                _ => 0,
            })
            .sum()
    }

    pub(super) fn profile_reserved(&self, profile: u128) -> bool {
        self.reserved.contains(&inventory_state_key(profile))
            || self.catalog.player_lifecycles().any(|reg| {
                crate::server::registry::SystemId::new(&reg.key).is_ok_and(|system| {
                    self.reserved
                        .contains(&crate::server::runtime::owner_codec::owner_state_key(
                            &system,
                            crate::server::parallel::OwnerKey::Profile(profile),
                        ))
                })
            })
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
        if self.checkpoint_writer.is_full() {
            return Err(CheckpointSubmitError::Full);
        }
        let revision = dirty.revision;
        let snapshot = dirty.snapshot.clone();
        self.checkpoint_writer
            .try_submit(key.clone(), revision, snapshot, write)?;
        self.checkpoint_inflight.insert(key, revision);
        Ok(true)
    }
}
