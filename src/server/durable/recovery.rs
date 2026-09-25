//! Startup validation and replay of the durable journal.

use super::state::{
    decode_chunk_key, decode_drop_key, decode_profile_key, invalid_data,
    valid_action_receipt_with_catalog,
};
use super::*;
use crate::server::entities::decode_checkpoint;
use crate::server::entities::{EntityStore, EntityTypeRegistry};
use crate::server::entity_checkpoint::EntityCheckpointMirror;
use crate::server::fire::{FireCheckpointStore, FireRecovered};
use crate::server::journal::Journal;
use std::sync::Arc;

pub(super) fn open(
    root: &Path,
    world: &mut World,
    inventory_store: &InventoryStore,
    drops: &mut Drops,
    entity_types: Arc<EntityTypeRegistry>,
) -> io::Result<(Durability, FireRecovered, EntityStore)> {
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
            "bloxgloom:drop_owner" => {
                let id = decode_drop_key(&key.bytes)?;
                journal.validate_snapshot(key, &drops.owner_snapshot(id))?;
            }
            "bloxgloom:drop_allocator" => {
                if !key.bytes.is_empty() || value.len() != 8 {
                    return Err(invalid_data("invalid journaled drop allocator"));
                }
                journal.validate_snapshot(key, &drops.allocator_snapshot())?;
            }
            "bloxgloom:drop_position" => {
                if decode_drop_key(&key.bytes)? == 0
                    || (!value.is_empty()
                        && (value.len() != 12
                            || value.chunks_exact(4).any(|bits| {
                                !f32::from_le_bytes(bits.try_into().unwrap()).is_finite()
                            })))
                {
                    return Err(invalid_data("invalid journaled drop position"));
                }
            }
            "bloxgloom:action_receipt" => {
                return Err(invalid_data(
                    "legacy action receipts are unsupported in this save format",
                ));
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
    let drop_owner_set_closed = journal.drop_owner_set_closed();
    // The drop store receives only its registered keys. The full journal map
    // was already checked above, so adding another persistent domain does not
    // require teaching drop recovery to ignore that domain by name.
    let drop_values = latest
        .iter()
        .filter(|(key, _)| {
            matches!(
                key.domain.as_str(),
                "bloxgloom:drop_owner" | "bloxgloom:drop_position" | "bloxgloom:drop_allocator"
            )
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    drops.validate_recovered_journal(&drop_values, drop_owner_set_closed)?;

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
    if drops.apply_recovered_journal(&drop_values, drop_owner_set_closed)? {
        drops.save()?;
    }
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
    ))
}
