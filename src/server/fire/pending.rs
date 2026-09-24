//! Source-scoped durable ignition mailboxes for resident or unloaded targets.

use super::codec::{checked_body, finish, invalid, key_bytes, read_key};
use crate::world::ChunkKey;
use std::collections::BTreeMap;
use std::io;

pub(super) const MAX_PENDING_IGNITIONS: usize = 4_096;

/// A stable producer identity independent of worker order and WAL retries.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct FireIgnitionId {
    pub(super) source_tick: u64,
    pub(super) source_cell: u16,
    pub(super) direction: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct FireIgnition {
    pub(super) id: FireIgnitionId,
    pub(super) target_cell: u16,
    pub(super) activate_at: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct FirePending {
    ignitions: BTreeMap<FireIgnitionId, FireIgnition>,
}

impl FirePending {
    pub(super) fn len(&self) -> usize {
        self.ignitions.len()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.ignitions.is_empty()
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = &FireIgnition> {
        self.ignitions.values()
    }

    pub(super) fn latest_tick(&self) -> u64 {
        self.iter()
            .map(|entry| entry.activate_at)
            .max()
            .unwrap_or(0)
    }

    /// No implicit overwrite: a stable ID naming different output is corrupt.
    pub(super) fn insert(&mut self, ignition: FireIgnition) -> io::Result<bool> {
        validate(ignition)?;
        if let Some(existing) = self.ignitions.get(&ignition.id) {
            if existing != &ignition {
                return Err(invalid("fire ignition identity collision"));
            }
            return Ok(false);
        }
        if self.ignitions.len() == MAX_PENDING_IGNITIONS {
            return Err(io::Error::other("fire destination mailbox full"));
        }
        self.ignitions.insert(ignition.id, ignition);
        Ok(true)
    }

    pub(super) fn remove(&mut self, id: FireIgnitionId) -> Option<FireIgnition> {
        self.ignitions.remove(&id)
    }

    pub(super) fn encode(&self) -> Vec<u8> {
        if self.is_empty() {
            return Vec::new();
        }
        let mut bytes = Vec::with_capacity(11 + self.len() * 21);
        bytes.extend(b"BGFP");
        bytes.push(1);
        bytes.extend((self.len() as u16).to_le_bytes());
        for entry in self.iter() {
            bytes.extend(entry.id.source_tick.to_le_bytes());
            bytes.extend(entry.id.source_cell.to_le_bytes());
            bytes.push(entry.id.direction);
            bytes.extend(entry.target_cell.to_le_bytes());
            bytes.extend(entry.activate_at.to_le_bytes());
        }
        finish(bytes)
    }

    pub(super) fn decode(bytes: &[u8]) -> io::Result<Self> {
        if bytes.is_empty() {
            return Ok(Self::default());
        }
        let body = checked_body(bytes, b"BGFP", 21)?;
        if body.len() / 21 > MAX_PENDING_IGNITIONS {
            return Err(invalid("fire mailbox exceeds bound"));
        }
        let mut pending = Self::default();
        let mut previous = None;
        for record in body.chunks_exact(21) {
            let ignition = FireIgnition {
                id: FireIgnitionId {
                    source_tick: u64::from_le_bytes(record[..8].try_into().unwrap()),
                    source_cell: u16::from_le_bytes(record[8..10].try_into().unwrap()),
                    direction: record[10],
                },
                target_cell: u16::from_le_bytes(record[11..13].try_into().unwrap()),
                activate_at: u64::from_le_bytes(record[13..21].try_into().unwrap()),
            };
            if previous.is_some_and(|id| id >= ignition.id) {
                return Err(invalid("unordered fire mailbox"));
            }
            validate(ignition)?;
            pending.ignitions.insert(ignition.id, ignition);
            previous = Some(ignition.id);
        }
        Ok(pending)
    }
}

fn validate(entry: FireIgnition) -> io::Result<()> {
    if entry.id.source_tick == 0
        || entry.id.source_cell >= 4_096
        || !(entry.id.direction < 6 || (8..14).contains(&entry.id.direction))
        || entry.target_cell >= 4_096
        || entry.activate_at <= entry.id.source_tick
    {
        return Err(invalid("invalid fire ignition"));
    }
    Ok(())
}

pub(super) fn pending_key(destination: ChunkKey, source: ChunkKey) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(24);
    bytes.extend(key_bytes(destination));
    bytes.extend(key_bytes(source));
    bytes
}

pub(super) fn decode_pending_key(bytes: &[u8]) -> io::Result<(ChunkKey, ChunkKey)> {
    if bytes.len() != 24 {
        return Err(invalid("invalid fire mailbox key"));
    }
    Ok((read_key(&bytes[..12])?, read_key(&bytes[12..])?))
}
