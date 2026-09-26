//! Startup validation and replay of the durable journal.
//!
//! JOURNAL DECISION (single journal): owner cells live in the main
//! `server.wal` under the `bloxgloom:owner_state` domain and recover from its
//! latest-values map alongside every other domain. A separate owner log was
//! rejected: a commit spanning entity state and owner state must be one WAL
//! record, and two tails would force recovery to reconcile them.
//!
//! Owner cells need no per-key checkpoint file. Rotation materializes the
//! full latest-value map — every domain — into the new base generation, so
//! owner state survives rotation with the base itself; the rotation gate
//! (empty pending, fenced entity checkpoint, drained per-key checkpoints)
//! already covers it. Registering dirty-checkpoint entries without a backing
//! file would wedge that gate forever, so owner commits deliberately do not
//! touch it: every owner wave is receipted before it is visible, hence
//! already durable at any rotation cut.

use super::state::{
    decode_chunk_key, decode_profile_key, invalid_data, valid_action_receipt_with_catalog,
};
use super::*;
use crate::server::entities::decode_checkpoint;
use crate::server::entities::{EntityStore, EntityTypeRegistry};
use crate::server::entity_checkpoint::EntityCheckpointMirror;
use crate::server::fire::{FireCheckpointStore, FireRecovered};
use crate::server::journal::Journal;
use crate::server::runtime::owner_codec::OWNER_STATE_DOMAIN;
use crate::server::runtime::owner_codec::{
    OWNER_CURSOR_DOMAIN, decode_cursor_value, decode_owner_cursor_key,
};
use crate::server::runtime::owner_wake::{OWNER_WAKE_DOMAIN, PendingWakeStore};
use std::collections::BTreeMap;
use std::sync::Arc;

pub(super) fn open(
    root: &Path,
    world: &mut World,
    inventory_store: &InventoryStore,
    entity_types: Arc<EntityTypeRegistry>,
    owner_configs: Vec<OwnerSystemConfig>,
) -> io::Result<(
    Durability,
    FireRecovered,
    EntityStore,
    DurableOwnerStore,
    PendingWakeStore,
    BTreeMap<SystemId, OwnerKey>,
)> {
    let journal = Journal::open(root.join("server.wal"))?;
    let latest = journal.latest_values();
    let recovered_entities =
        super::entity_recovery::prepare(root, &journal, &latest, Arc::clone(&entity_types))?;
    let receipt_store = receipts::ReceiptStore::new(root)?;
    let fire_store = FireCheckpointStore::new(root)?;
    let mut fire_recovered = FireRecovered::default();
    let mut fire_replay = Vec::new();
    let mut receipt_ledgers = HashMap::new();
    let mut receipt_replay = Vec::new();
    let mut inventory_revisions = HashMap::new();
    let mut owner_latest = BTreeMap::new();
    let mut wake_latest = BTreeMap::new();
    let mut cursor_latest = BTreeMap::new();
    let storage = world.storage_handle();
    let mut chunk_replay = Vec::new();
    let mut inventory_replay = Vec::new();

    // Validate every candidate and current checkpoint first. Recovery is a
    // second pass so a corrupt later participant cannot leave earlier save
    // files partially overwritten before startup rejects the world.
    for (key, value) in &latest {
        match key.domain.as_str() {
            "bloxgloom:chunk_snapshot" => {
                let chunk = decode_chunk_key(&key.bytes)?;
                let current_opt = world.read_chunk_snapshot(chunk)?;
                if let Some(existing) = current_opt.as_deref() {
                    storage.decode_snapshot(Some(existing))?;
                }
                storage.decode_snapshot((!value.is_empty()).then_some(value.as_slice()))?;
                let current = current_opt.unwrap_or_default();
                journal.validate_snapshot(key, &current)?;
                if current != *value {
                    chunk_replay.push((chunk, value.clone()));
                }
            }
            "bloxgloom:inventory" => {
                let profile = decode_profile_key(&key.bytes)?;
                let current_opt = inventory_store.read_snapshot(profile)?;
                let current = match current_opt.as_ref() {
                    Some(existing) => {
                        InventoryStore::decode_snapshot_with_catalog(existing, world.catalog())?;
                        existing.clone()
                    }
                    None => InventoryStore::encode_snapshot_with_catalog(
                        &Inventory::default(),
                        world.catalog(),
                    )?,
                };
                journal.validate_snapshot(key, &current)?;
                let recovered =
                    InventoryStore::decode_snapshot_with_catalog(value, world.catalog())?;
                inventory_revisions.insert(profile, recovered.revision);
                if current != *value {
                    inventory_replay.push((profile, value.clone()));
                }
            }
            "bloxgloom:action_ledger" => {
                let profile = decode_profile_key(&key.bytes)?;
                let current = receipt_store.read(profile)?.unwrap_or_default();
                if !current.is_empty() {
                    let checkpoint = receipts::ReceiptLedger::decode(&current)?;
                    for record in &checkpoint.results {
                        if !valid_action_receipt_with_catalog(&record.payload, world.catalog()) {
                            return Err(invalid_data("invalid checkpointed action result payload"));
                        }
                    }
                }
                journal.validate_snapshot(key, &current)?;
                let ledger = receipts::ReceiptLedger::decode(value)?;
                for record in &ledger.results {
                    if !valid_action_receipt_with_catalog(&record.payload, world.catalog()) {
                        return Err(invalid_data("invalid durable action result payload"));
                    }
                }
                receipt_ledgers.insert(profile, ledger);
                if current != *value {
                    receipt_replay.push((profile, value.clone()));
                }
            }
            "bloxgloom:drops_snapshot" => {
                return Err(invalid_data(
                    "aggregate drop snapshots are not journal keys",
                ));
            }
            domain if domain.starts_with("bloxgloom:fire_") => {
                fire_recovered.apply_value(key, value)?;
                let current = fire_store.read(key)?.unwrap_or_default();
                journal.validate_snapshot(key, &current)?;
                if current != *value {
                    fire_replay.push((key.clone(), value.clone()));
                }
            }
            domain if domain.starts_with("bloxgloom:entity") => {
                // Entity codecs, checkpoint reachability, and derived indexes
                // were validated as one aggregate before this replay pass.
            }
            domain if domain == OWNER_STATE_DOMAIN => {
                // The journal base+tail is the primary store for owner
                // cells; there is no per-key file to validate against.
                // Collected here, decoded by `DurableOwnerStore::recover`
                // before any replay write below: malformed keys, unknown
                // systems, bad envelopes, and undecodable payloads fail
                // closed with `InvalidData` while the save is untouched.
                owner_latest.insert(key.clone(), value.clone());
            }
            domain if domain == OWNER_WAKE_DOMAIN => {
                // Pending owner wakes ride the same base+tail rotation as
                // owner cells: no per-key file, decoded by
                // `PendingWakeStore::recover` before any replay write below.
                // Malformed keys and undecodable flags fail closed with
                // `InvalidData` while the save is untouched.
                wake_latest.insert(key.clone(), value.clone());
            }
            domain if domain == OWNER_CURSOR_DOMAIN => {
                // Per-system round-robin cursors ride the same base+tail
                // rotation as owner cells: no per-key file, decoded below
                // before any replay write. Malformed keys, cursors for
                // unregistered systems, and undecodable values fail closed
                // with `InvalidData` while the save is untouched.
                cursor_latest.insert(key.clone(), value.clone());
            }
            domain => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unsupported journal key domain: {domain}"),
                ));
            }
        }
    }
    receipt_store.validate_no_orphans(&latest)?;
    fire_store.validate_no_orphans(&latest)?;

    // Fail closed before the first replay write below: a corrupt owner
    // record must reject the world without touching any save file.
    let owner_store = DurableOwnerStore::recover(owner_configs, &owner_latest)?;
    let wake_store = PendingWakeStore::recover(&wake_latest)?;
    let mut cursors = BTreeMap::new();
    for (key, value) in &cursor_latest {
        let Some(system_name) = decode_owner_cursor_key(key) else {
            return Err(invalid_data("owner cursor key is malformed"));
        };
        let system = SystemId::new(system_name)
            .map_err(|_| invalid_data("owner cursor key has a bad system id"))?;
        if !owner_store.is_registered(&system) {
            return Err(invalid_data("owner cursor names an unregistered system"));
        }
        let owner = decode_cursor_value(value)?;
        if cursors.insert(system, owner).is_some() {
            return Err(invalid_data("duplicate owner cursor key"));
        }
    }

    for (chunk, value) in chunk_replay {
        world.restore_snapshot(chunk, &value)?;
    }
    for (profile, value) in inventory_replay {
        inventory_store.checkpoint_snapshot(profile, &value)?;
    }
    for (profile, value) in receipt_replay {
        receipt_store.write(profile, &value)?;
    }
    if !fire_replay.is_empty() {
        fire_store.write_batch(&fire_replay)?;
    }
    fire_store.cleanup_interrupted_temps()?;
    recovered_entities.publish_replay()?;
    // Give the checkpoint worker an independently decoded authoritative
    // baseline. It never borrows or serializes the live entity store on a
    // simulation tick; every later change is delivered in commit order.
    let mirror_baseline = match recovered_entities.checkpoint_store.read()? {
        Some(bytes) => decode_checkpoint(&bytes, entity_types)
            .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))?,
        None => EntityStore::new(entity_types),
    };
    if mirror_baseline.durable_sequence() != recovered_entities.entities.durable_sequence()
        || mirror_baseline.revision() != recovered_entities.entities.revision()
        || mirror_baseline.len() != recovered_entities.entities.len()
    {
        return Err(invalid_data(
            "entity checkpoint mirror baseline differs from recovered store",
        ));
    }
    let entity_mirror = EntityCheckpointMirror::start(
        mirror_baseline,
        recovered_entities.checkpoint_store.clone(),
        MAX_PENDING_DURABLE_ACTIONS,
    )?;
    let next_id = journal.next_id()?;
    let writer = journal.into_writer(128, Duration::from_millis(3))?;
    Ok((
        Durability {
            catalog: world.catalog_arc(),
            writer,
            next_id,
            inventory_overlay: HashMap::new(),
            inventory_revisions,
            pending: Vec::new(),
            reserved: HashSet::new(),
            queued: VecDeque::new(),
            entity_tick_cursor: None,
            entity_sleep_cursor: None,
            entity_admission_turn: 0,
            oversized_entity_retry: BTreeMap::new(),
            pending_wakes: Vec::new(),
            retry_pickups: HashSet::new(),
            expire_queued: false,
            expire_again: false,
            publish_queue: Vec::new(),
            next_publish_commit_id: 1,
            checkpoint_writer: CheckpointWriter::new_with_workers(
                CHECKPOINT_QUEUE_CAPACITY,
                CHECKPOINT_WORKERS,
            ),
            dirty_checkpoints: HashMap::new(),
            checkpoint_inflight: HashMap::new(),
            fire_checkpoint_batch: None,
            next_checkpoint_revision: 1,
            receipt_store,
            fire_store,
            entity_store: recovered_entities.checkpoint_store,
            entity_mirror,
            entity_checkpoint_ticket: None,
            receipt_ledgers,
            pending_grants: HashSet::new(),
            ready_grants: HashMap::new(),
            pending_acks: HashMap::new(),
            rotation_requested: false,
            rotation_snapshot_ready: false,
            rotation_receipt: None,
            force_rotation_at_sequence: None,
            completed_rotations: 0,
            failed: false,
        },
        fire_recovered,
        recovered_entities.entities,
        owner_store,
        wake_store,
        cursors,
    ))
}
