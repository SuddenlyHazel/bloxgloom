//! Codec contract and journal addressing for durable owner cells.
//!
//! Owner values live decoded in memory (`OwnerData`) and are serialized only
//! for WAL `Change` values addressed by [`StateKey`]. The shape mirrors
//! `EntityPayloadCodec`: `encode` / `decode` / `migrate` with a declared
//! per-system byte bound enforced at insert and at patch time. Fail closed:
//! oversized writes are rejected, never truncated.
//!
//! The [`OWNER_STATE_DOMAIN`] domain is new persisted state. Existing
//! encodings are untouched, so no world-folder version bump is needed.

use super::super::journal::StateKey;
use super::super::parallel::{OwnerData, OwnerKey};
use super::super::registry::SystemId;
use crate::world::ChunkKey;
use std::io::{self, ErrorKind};

/// Hard ceiling for one owner value, mirroring `MAX_ENTITY_PAYLOAD_BYTES`.
/// Systems declare their own bound at or below this ceiling.
pub(in crate::server) const MAX_OWNER_VALUE_BYTES: usize = 64 * 1024;

/// Journal domain for durable owner cells. New in this change; no existing
/// encoding uses it.
pub(in crate::server) const OWNER_STATE_DOMAIN: &str = "bloxgloom:owner_state";

/// Magic for the owner-cell value envelope: `revision + codec version +
/// payload`. The envelope is new persisted state; the payload itself is
/// codec-defined.
const OWNER_CELL_MAGIC: &[u8; 4] = b"BGOW";
const OWNER_CELL_VERSION: u16 = 1;

/// Fail-closed codec failure, mirroring `EntityCodecError`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::server) enum OwnerCodecError {
    InvalidData,
    UnsupportedVersion,
}

/// Type-specific owner-value encoding for one registered system.
///
/// Values stay decoded as [`OwnerData`] in the live store; codecs serialize
/// only for WAL change values. `migrate` upgrades a stored payload from one
/// codec version to another; the default accepts identity only.
pub(in crate::server) trait OwnerValueCodec: Send + Sync + 'static {
    fn decode(&self, payload: &[u8]) -> Result<OwnerData, OwnerCodecError>;

    fn encode(&self, value: &OwnerData) -> Result<Vec<u8>, OwnerCodecError>;

    fn migrate(
        &self,
        from_version: u16,
        to_version: u16,
        payload: &[u8],
    ) -> Result<Vec<u8>, OwnerCodecError> {
        if from_version == to_version {
            Ok(payload.to_vec())
        } else {
            Err(OwnerCodecError::UnsupportedVersion)
        }
    }
}

/// Stable journal address for one owner cell of one system.
pub(in crate::server) fn owner_state_key(system: &SystemId, owner: OwnerKey) -> StateKey {
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
    StateKey::new(OWNER_STATE_DOMAIN, bytes)
}

/// Decodes the system identity and owner from an owner-state key. Returns
/// `None` for malformed keys so recovery can fail closed with `InvalidData`.
pub(in crate::server) fn decode_owner_state_key(key: &StateKey) -> Option<(String, OwnerKey)> {
    if key.domain != OWNER_STATE_DOMAIN || key.bytes.len() < 3 {
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

/// Encodes a durable cell value: envelope plus codec payload.
pub(in crate::server) fn encode_cell_value(
    revision: u64,
    codec_version: u16,
    payload: &[u8],
) -> Vec<u8> {
    let mut value = Vec::with_capacity(4 + 2 + 8 + 2 + payload.len() + 4);
    value.extend(OWNER_CELL_MAGIC);
    value.extend(OWNER_CELL_VERSION.to_le_bytes());
    value.extend(revision.to_le_bytes());
    value.extend(codec_version.to_le_bytes());
    value.extend(payload);
    let crc = crc32(&value);
    value.extend(crc.to_le_bytes());
    value
}

/// Decodes a durable cell value into `(revision, codec_version, payload)`.
/// Any structural problem is `InvalidData`: genuine corruption that may stop
/// the coordinator. Capacity pressure is never reported through this path.
pub(in crate::server) fn decode_cell_value(value: &[u8]) -> io::Result<(u64, u16, Vec<u8>)> {
    if value.len() < 4 + 2 + 8 + 2 + 4 {
        return Err(invalid_data("owner cell value is truncated"));
    }
    let (body, crc_bytes) = value.split_at(value.len() - 4);
    let expected = u32::from_le_bytes(crc_bytes.try_into().expect("crc is 4 bytes"));
    if crc32(body) != expected {
        return Err(invalid_data("owner cell value checksum mismatch"));
    }
    if &body[0..4] != OWNER_CELL_MAGIC {
        return Err(invalid_data("owner cell value has a bad magic"));
    }
    if u16::from_le_bytes(body[4..6].try_into().expect("version is 2 bytes")) != OWNER_CELL_VERSION
    {
        return Err(invalid_data("owner cell value has an unsupported version"));
    }
    let revision = u64::from_le_bytes(body[6..14].try_into().expect("revision is 8 bytes"));
    let codec_version = u16::from_le_bytes(body[14..16].try_into().expect("codec is 2 bytes"));
    Ok((revision, codec_version, body[16..].to_vec()))
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
#[path = "owner_codec/tests.rs"]
mod tests;
