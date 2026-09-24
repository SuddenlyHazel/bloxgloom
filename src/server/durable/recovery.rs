//! Startup validation and replay of the durable journal.

use super::state::{
    decode_chunk_key, decode_drop_key, decode_profile_key, invalid_data, valid_action_receipt,
};
use super::*;
use crate::server::journal::Journal;

pub(super) fn open(
    root: &Path,
    world: &mut World,
    inventory_store: &InventoryStore,
    drops: &mut Drops,
) -> io::Result<Durability> {
    let journal = Journal::open(root.join("server.wal"))?;
    let latest = journal.latest_values();
    let mut action_receipts = HashMap::new();
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
                        InventoryStore::decode_snapshot(existing)?;
                        existing.clone()
                    }
                    None => InventoryStore::encode_snapshot(&Inventory::default())?,
                };
                journal.validate_snapshot(key, &current)?;
                let recovered = InventoryStore::decode_snapshot(value)?;
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
                if key.bytes.len() != 32 || !valid_action_receipt(value) {
                    return Err(invalid_data("invalid durable action receipt"));
                }
                let profile = u128::from_le_bytes(key.bytes[..16].try_into().unwrap());
                let action_id = u128::from_le_bytes(key.bytes[16..].try_into().unwrap());
                if profile == 0 || action_id == 0 {
                    return Err(invalid_data("invalid durable action receipt key"));
                }
                if action_receipts.len() >= MAX_ACTION_RECEIPTS {
                    return Err(invalid_data("durable action receipt limit exceeded"));
                }
                action_receipts.insert((profile, action_id), value.clone());
            }
            "bloxgloom:drops_snapshot" => {
                return Err(invalid_data(
                    "aggregate drop snapshots are not journal keys",
                ));
            }
            domain => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unsupported journal key domain: {domain}"),
                ));
            }
        }
    }
    let drop_owner_set_closed = journal.drop_owner_set_closed();
    drops.validate_recovered_journal(&latest, drop_owner_set_closed)?;

    for (chunk, value) in chunk_replay {
        world.restore_snapshot(chunk, &value)?;
    }
    for (profile, value) in inventory_replay {
        inventory_store.checkpoint_snapshot(profile, &value)?;
    }
    if drops.apply_recovered_journal(&latest, drop_owner_set_closed)? {
        drops.save()?;
    }
    let next_id = journal.next_id()?;
    let writer = journal.into_writer(128, Duration::from_millis(3))?;
    Ok(Durability {
        writer,
        next_id,
        inventory_overlay: HashMap::new(),
        inventory_revisions,
        pending: Vec::new(),
        reserved: HashSet::new(),
        queued: VecDeque::new(),
        retry_pickups: HashSet::new(),
        expire_queued: false,
        expire_again: false,
        publish_queue: Vec::new(),
        checkpoint_writer: CheckpointWriter::new(CHECKPOINT_QUEUE_CAPACITY),
        dirty_checkpoints: HashMap::new(),
        checkpoint_inflight: HashMap::new(),
        next_checkpoint_revision: 1,
        action_receipts,
        rotation_requested: false,
        rotation_snapshot_ready: false,
        rotation_receipt: None,
        failed: false,
    })
}
