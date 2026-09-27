//! BGCT v1: independent bounded container slots, without player revision/layout.
//! Journal/checkpoint frames supply integrity checks. Exact length and canonical
//! empty/component representations are checked on decode.
use super::{ComponentPayload, MAX_COMPONENT_BYTES, Stack};
use crate::content::Catalog;
use crate::items::ItemId;
use bloxgloom_host_api::inventory::MAX_SLOTS;
use std::{io, sync::Arc};

pub fn max_bytes(slots: usize) -> usize {
    6 + slots * (10 + MAX_COMPONENT_BYTES)
}
fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid container snapshot")
}

pub fn encode(slots: &[Option<Stack>], catalog: &Catalog) -> io::Result<Vec<u8>> {
    if slots.is_empty() || slots.len() > MAX_SLOTS {
        return Err(invalid());
    }
    let mut bytes = b"BGCT\x01".to_vec();
    bytes.push(slots.len() as u8);
    for slot in slots {
        if let Some(stack) = slot {
            if !stack.valid_in(catalog) {
                return Err(invalid());
            }
            bytes.extend(stack.item.0.to_le_bytes());
            bytes.extend(stack.count.to_le_bytes());
            if let Some(components) = &stack.components {
                bytes.extend(components.version.to_le_bytes());
                bytes.extend((components.bytes.len() as u16).to_le_bytes());
                bytes.extend(&components.bytes);
            } else {
                bytes.extend([0; 4]);
            }
        } else {
            bytes.extend([0; 10]);
        }
    }
    Ok(bytes)
}
pub fn decode(bytes: &[u8], expected: usize, catalog: &Catalog) -> io::Result<Vec<Option<Stack>>> {
    if expected == 0
        || expected > MAX_SLOTS
        || bytes.len() < 6
        || bytes.len() > max_bytes(expected)
        || &bytes[..5] != b"BGCT\x01"
        || usize::from(bytes[5]) != expected
    {
        return Err(invalid());
    }
    let mut at = 6;
    let mut slots = Vec::with_capacity(expected);
    for _ in 0..expected {
        let entry = bytes.get(at..at + 10).ok_or_else(invalid)?;
        at += 10;
        let item = ItemId(u32::from_le_bytes(entry[..4].try_into().unwrap()));
        let count = u16::from_le_bytes(entry[4..6].try_into().unwrap());
        let version = u16::from_le_bytes(entry[6..8].try_into().unwrap());
        let length = usize::from(u16::from_le_bytes(entry[8..].try_into().unwrap()));
        if item.0 == 0 {
            if entry.iter().any(|v| *v != 0) {
                return Err(invalid());
            }
            slots.push(None);
            continue;
        }
        let components = if length == 0 {
            if version != 0 {
                return Err(invalid());
            }
            None
        } else {
            if length > MAX_COMPONENT_BYTES {
                return Err(invalid());
            }
            let raw = bytes.get(at..at + length).ok_or_else(invalid)?;
            at += length;
            Some(Arc::new(
                ComponentPayload::new(version, raw.to_vec()).ok_or_else(invalid)?,
            ))
        };
        let stack = Stack {
            item,
            count,
            components,
        };
        if !stack.valid_in(catalog) {
            return Err(invalid());
        }
        slots.push(Some(stack));
    }
    if at != bytes.len() {
        return Err(invalid());
    }
    Ok(slots)
}
#[cfg(test)]
mod tests;
