//! Durable pending-wake flags for owner-system effect destinations.
//!
//! THE RULE (see `owner_effects`): an effect may only cause work to happen
//! SOONER. The durable truth is therefore the schedule, not the effect: a
//! pending wake is a tiny bounded "this owner is due" flag keyed by
//! destination, carrying no effect payload. The destination still does its own
//! durable work through its normal handler when the wake is served, so a lost
//! wake costs latency and never state.
//!
//! Wake flags live in the main journal under [`OWNER_WAKE_DOMAIN`] — new
//! persisted state alongside `bloxgloom:owner_state`, with existing encodings
//! untouched. A present value means the destination is due; an empty `after`
//! value clears the flag. The producing tick rides in the envelope so replays
//! stay deterministic; it is informational only and never schedules anything
//! by itself.
//!
//! Corruption (bad magic, version, length, checksum) reports `InvalidData` so
//! the coordinator can stop. Capacity (too many pending wakes) is enforced by
//! the holder of the wake set, never here.

use super::super::journal::StateKey;
use super::super::parallel::OwnerKey;
use super::super::registry::SystemId;
use crate::world::ChunkKey;
use std::io::{self, ErrorKind};

/// Journal domain for durable pending owner wakes. New persisted state; no
/// existing encoding uses it.
pub(in crate::server) const OWNER_WAKE_DOMAIN: &str = "bloxgloom:owner_wake";

/// Magic for the wake-flag value envelope: `wake tick + checksum`. The
/// envelope is new persisted state.
const OWNER_WAKE_MAGIC: &[u8; 4] = b"BGWK";
const OWNER_WAKE_VERSION: u16 = 1;

/// Exact encoded length of one present wake flag: magic + version + tick +
/// checksum. Fixed and tiny by construction; there is nothing to truncate.
pub(in crate::server) const OWNER_WAKE_VALUE_LEN: usize = 4 + 2 + 8 + 4;

/// Stable journal address for one destination's pending wake. The byte layout
/// mirrors [`super::owner_codec::owner_state_key`] so the same destination
/// maps to the same suffix in either domain; the domains never collide.
pub(in crate::server) fn owner_wake_key(system: &SystemId, owner: OwnerKey) -> StateKey {
    let system_bytes = system.as_str().as_bytes();
    let mut bytes = Vec::with_capacity(2 + system_bytes.len() + 1 + 32);
    let system_len = u16::try_from(system_bytes.len()).expect("system id fits in u16");
    bytes.extend(system_len.to_le_bytes());
    bytes.extend(system_bytes);
    match owner {
        OwnerKey::Chunk(key) => {
            bytes.push(0);
            bytes.extend(key.x.to_le_bytes());
            bytes.extend(key.y.to_le_bytes());
            bytes.extend(key.z.to_le_bytes());
        }
        OwnerKey::Entity(id) => {
            bytes.push(1);
            bytes.extend(id.to_le_bytes());
        }
        OwnerKey::Profile(id) => {
            bytes.push(2);
            bytes.extend(id.to_le_bytes());
        }
    }
    StateKey::new(OWNER_WAKE_DOMAIN, bytes)
}

/// Decodes the system identity and owner from a wake-flag key. Returns `None`
/// for malformed keys so recovery can fail closed with `InvalidData`.
pub(in crate::server) fn decode_owner_wake_key(key: &StateKey) -> Option<(String, OwnerKey)> {
    if key.domain != OWNER_WAKE_DOMAIN || key.bytes.len() < 3 {
        return None;
    }
    let system_len = usize::from(u16::from_le_bytes(key.bytes[0..2].try_into().ok()?));
    let system_end = 2usize.checked_add(system_len)?;
    let tag_index = system_end;
    let system_bytes = key.bytes.get(2..system_end)?;
    let system = std::str::from_utf8(system_bytes).ok()?.to_owned();
    let tag = *key.bytes.get(tag_index)?;
    let rest = key.bytes.get(tag_index + 1..)?;
    let owner = match tag {
        0 => {
            if rest.len() != 12 {
                return None;
            }
            OwnerKey::Chunk(ChunkKey {
                x: i32::from_le_bytes(rest[0..4].try_into().ok()?),
                y: i32::from_le_bytes(rest[4..8].try_into().ok()?),
                z: i32::from_le_bytes(rest[8..12].try_into().ok()?),
            })
        }
        1 => {
            if rest.len() != 8 {
                return None;
            }
            OwnerKey::Entity(u64::from_le_bytes(rest.try_into().ok()?))
        }
        2 => {
            if rest.len() != 16 {
                return None;
            }
            OwnerKey::Profile(u128::from_le_bytes(rest.try_into().ok()?))
        }
        _ => return None,
    };
    SystemId::new(&system).ok()?;
    Some((system, owner))
}

/// Encodes a present wake flag carrying the producing tick.
pub(in crate::server) fn encode_wake_value(wake_tick: u64) -> Vec<u8> {
    let mut value = Vec::with_capacity(OWNER_WAKE_VALUE_LEN);
    value.extend(OWNER_WAKE_MAGIC);
    value.extend(OWNER_WAKE_VERSION.to_le_bytes());
    value.extend(wake_tick.to_le_bytes());
    let crc = crc32(&value);
    value.extend(crc.to_le_bytes());
    debug_assert_eq!(value.len(), OWNER_WAKE_VALUE_LEN);
    value
}

/// Decodes a present wake flag into its producing tick. An empty value is a
/// cleared flag, not a present one — callers treat absence as absent; this
/// reports it as `InvalidData` so a cleared flag can never be mistaken for a
/// due destination. Any structural problem is `InvalidData`: genuine
/// corruption that may stop the coordinator. Capacity pressure is never
/// reported through this path.
pub(in crate::server) fn decode_wake_value(value: &[u8]) -> io::Result<u64> {
    if value.len() != OWNER_WAKE_VALUE_LEN {
        return Err(invalid_data("owner wake value has a bad length"));
    }
    let (body, crc_bytes) = value.split_at(value.len() - 4);
    let expected = u32::from_le_bytes(crc_bytes.try_into().expect("crc is 4 bytes"));
    if crc32(body) != expected {
        return Err(invalid_data("owner wake value checksum mismatch"));
    }
    if &body[0..4] != OWNER_WAKE_MAGIC {
        return Err(invalid_data("owner wake value has a bad magic"));
    }
    if u16::from_le_bytes(body[4..6].try_into().expect("version is 2 bytes")) != OWNER_WAKE_VERSION
    {
        return Err(invalid_data("owner wake value has an unsupported version"));
    }
    Ok(u64::from_le_bytes(
        body[6..14].try_into().expect("wake tick is 8 bytes"),
    ))
}

fn invalid_data(message: &'static str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = crc & 1;
            crc >>= 1;
            if mask == 1 {
                crc ^= 0xEDB8_8320;
            }
        }
    }
    !crc
}

#[cfg(test)]
#[path = "owner_wake/tests.rs"]
mod tests;
