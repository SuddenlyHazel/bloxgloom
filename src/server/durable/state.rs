//! Journal key construction and action payload codecs.

use super::{CommitAction, StateKey};
use crate::inventory::{InventoryStore, SLOTS, STACK_LIMIT};
use crate::protocol::ClientMessage;
use crate::server::journal::Change;
use crate::world::ChunkKey;
use std::io::{self, ErrorKind};

pub(in crate::server) fn action_changes(action: &CommitAction) -> io::Result<Vec<Change>> {
    let mut changes = Vec::new();
    for edit in &action.world_edits {
        if edit.changed {
            changes.push(Change::new(
                chunk_state_key(edit.key),
                edit.before_snapshot.clone(),
                edit.after_snapshot.clone(),
            ));
        }
    }
    if let Some(inventory) = &action.inventory {
        let profile = action
            .profile
            .ok_or_else(|| invalid_data("inventory action has no profile"))?;
        let before = action
            .inventory_before
            .clone()
            .ok_or_else(|| invalid_data("inventory action has no before snapshot"))?;
        changes.push(Change::new(
            inventory_state_key(profile),
            before,
            InventoryStore::encode_snapshot(inventory)?,
        ));
    }
    for mutation in &action.drops.changes {
        changes.push(Change::new(
            drop_owner_state_key(mutation.id),
            mutation.before.clone(),
            mutation.after.clone(),
        ));
        if let Some(position) = mutation.initial_position {
            changes.push(Change::new(
                drop_position_state_key(mutation.id),
                Vec::new(),
                encode_position(position),
            ));
        }
    }
    if let Some((before, after)) = action.drops.allocator {
        changes.push(Change::new(
            drop_allocator_state_key(),
            before.to_le_bytes().to_vec(),
            after.to_le_bytes().to_vec(),
        ));
    }
    if let (Some(profile), Some(action_id), Some(value)) = (
        action.profile,
        action.action_id,
        action.receipt_value.clone(),
    ) {
        changes.push(Change::new(
            action_receipt_state_key(profile, action_id),
            Vec::new(),
            value,
        ));
    } else if action.action_id.is_some() {
        return Err(invalid_data("client durable action has no receipt payload"));
    }
    Ok(changes)
}

pub(in crate::server) fn chunk_state_key(key: ChunkKey) -> StateKey {
    let mut bytes = Vec::with_capacity(12);
    bytes.extend(key.x.to_le_bytes());
    bytes.extend(key.y.to_le_bytes());
    bytes.extend(key.z.to_le_bytes());
    StateKey::new("bloxgloom:chunk_snapshot", bytes)
}

pub(in crate::server) fn inventory_state_key(profile: u128) -> StateKey {
    StateKey::new("bloxgloom:inventory", profile.to_le_bytes().to_vec())
}

fn drop_owner_state_key(id: u64) -> StateKey {
    StateKey::new("bloxgloom:drop_owner", id.to_le_bytes().to_vec())
}

fn drop_position_state_key(id: u64) -> StateKey {
    StateKey::new("bloxgloom:drop_position", id.to_le_bytes().to_vec())
}

fn drop_allocator_state_key() -> StateKey {
    StateKey::new("bloxgloom:drop_allocator", Vec::new())
}

pub(in crate::server) fn action_receipt_state_key(profile: u128, action_id: u128) -> StateKey {
    let mut bytes = Vec::with_capacity(32);
    bytes.extend(profile.to_le_bytes());
    bytes.extend(action_id.to_le_bytes());
    StateKey::new("bloxgloom:action_receipt", bytes)
}

pub(in crate::server) fn drops_checkpoint_key() -> StateKey {
    StateKey::new("bloxgloom:drops_snapshot", Vec::new())
}

pub(in crate::server) fn encode_action_receipt(message: &ClientMessage) -> io::Result<Vec<u8>> {
    let mut value = vec![1];
    match message {
        ClientMessage::Edit {
            x,
            y,
            z,
            block,
            slot,
            ..
        } => {
            value.push(0);
            value.extend(x.to_le_bytes());
            value.extend(y.to_le_bytes());
            value.extend(z.to_le_bytes());
            value.push(*block);
            value.push(*slot);
        }
        ClientMessage::InventoryMove {
            from, to, count, ..
        } => {
            value.extend([1, *from, *to]);
            value.extend(count.to_le_bytes());
        }
        ClientMessage::DropStack { slot, count, .. } => {
            value.extend([2, *slot]);
            value.extend(count.to_le_bytes());
        }
        _ => return Err(invalid_data("cannot create receipt for this command")),
    }
    if !valid_action_receipt(&value) {
        return Err(invalid_data("invalid client durable action payload"));
    }
    Ok(value)
}

pub(in crate::server) fn is_checkpoint_key(key: &StateKey) -> bool {
    matches!(
        key.domain.as_str(),
        "bloxgloom:chunk_snapshot" | "bloxgloom:inventory" | "bloxgloom:drops_snapshot"
    )
}

pub(in crate::server::durable) fn valid_action_receipt(value: &[u8]) -> bool {
    match value {
        [
            1,
            0,
            _x0,
            _x1,
            _x2,
            _x3,
            _y0,
            _y1,
            _y2,
            _y3,
            _z0,
            _z1,
            _z2,
            _z3,
            block,
            slot,
        ] => crate::world::valid_block(*block) && usize::from(*slot) < SLOTS,
        [1, 1, from, to, count0, count1] => {
            usize::from(*from) < SLOTS
                && usize::from(*to) < SLOTS
                && (1..=STACK_LIMIT).contains(&u16::from_le_bytes([*count0, *count1]))
        }
        [1, 2, slot, count0, count1] => {
            usize::from(*slot) < SLOTS
                && (1..=STACK_LIMIT).contains(&u16::from_le_bytes([*count0, *count1]))
        }
        _ => false,
    }
}

fn encode_position(position: [f32; 3]) -> Vec<u8> {
    position.into_iter().flat_map(f32::to_le_bytes).collect()
}

pub(in crate::server::durable) fn decode_chunk_key(bytes: &[u8]) -> io::Result<ChunkKey> {
    if bytes.len() != 12 {
        return Err(invalid_data("invalid journal chunk key"));
    }
    Ok(ChunkKey {
        x: i32::from_le_bytes(bytes[0..4].try_into().unwrap()),
        y: i32::from_le_bytes(bytes[4..8].try_into().unwrap()),
        z: i32::from_le_bytes(bytes[8..12].try_into().unwrap()),
    })
}

pub(in crate::server::durable) fn decode_profile_key(bytes: &[u8]) -> io::Result<u128> {
    if bytes.len() != 16 {
        return Err(invalid_data("invalid journal profile key"));
    }
    let profile = u128::from_le_bytes(bytes.try_into().unwrap());
    if profile == 0 {
        return Err(invalid_data("invalid journal profile ID"));
    }
    Ok(profile)
}

pub(in crate::server::durable) fn decode_drop_key(bytes: &[u8]) -> io::Result<u64> {
    if bytes.len() != 8 {
        return Err(invalid_data("invalid journal drop key"));
    }
    let id = u64::from_le_bytes(bytes.try_into().unwrap());
    if id == 0 {
        return Err(invalid_data("invalid journal drop ID"));
    }
    Ok(id)
}

pub(in crate::server::durable) fn invalid_data(message: &'static str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}
