//! Isolated v4 drop and action codecs. No production v5 path accepts them.

use std::collections::BTreeMap;
use std::io;

use crate::content::ItemId;
use crate::inventory::{SLOTS, STACK_LIMIT};
use crate::storage::legacy::LegacyIdMap;

const OLD_DROP_HEADER: usize = 26;
const OLD_DROP_RECORD: usize = 33;
const NEW_DROP_RECORD: usize = 40;
const MAX_DROPS: usize = 1_000_000;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct LegacyDrop {
    pub id: u64,
    pub item: ItemId,
    pub count: u16,
    pub position: [f32; 3],
    pub born_ms: u64,
    pub pickup_delay_ms: u16,
    old_item: u8,
}

#[derive(Clone, Debug)]
pub(super) struct LegacyDrops {
    pub revision: u64,
    pub next_id: u64,
    pub entries: BTreeMap<u64, LegacyDrop>,
}

impl LegacyDrops {
    pub fn decode(bytes: Option<&[u8]>, ids: &LegacyIdMap) -> io::Result<Self> {
        let Some(bytes) = bytes else {
            return Ok(Self {
                revision: 0,
                next_id: 1,
                entries: BTreeMap::new(),
            });
        };
        if bytes.len() < OLD_DROP_HEADER + 4
            || &bytes[..4] != b"BGDP"
            || u16::from_le_bytes(bytes[4..6].try_into().unwrap()) != 1
        {
            return Err(invalid("unsupported legacy drops format"));
        }
        let count = u32::from_le_bytes(bytes[22..26].try_into().unwrap()) as usize;
        if count > MAX_DROPS || bytes.len() != OLD_DROP_HEADER + count * OLD_DROP_RECORD + 4 {
            return Err(invalid("invalid legacy drops length"));
        }
        let checksum_at = bytes.len() - 4;
        if u32::from_le_bytes(bytes[checksum_at..].try_into().unwrap())
            != checksum(&bytes[..checksum_at])
        {
            return Err(invalid("legacy drops checksum mismatch"));
        }
        let revision = u64::from_le_bytes(bytes[6..14].try_into().unwrap());
        let next_id = u64::from_le_bytes(bytes[14..22].try_into().unwrap());
        if next_id == 0 {
            return Err(invalid("invalid legacy drop allocator"));
        }
        let mut entries = BTreeMap::new();
        for record in bytes[OLD_DROP_HEADER..checksum_at].chunks_exact(OLD_DROP_RECORD) {
            let id = u64::from_le_bytes(record[0..8].try_into().unwrap());
            let old_item = record[8];
            let item = ids
                .item(old_item)
                .ok_or_else(|| invalid("unknown legacy drop item"))?;
            let count = u16::from_le_bytes(record[9..11].try_into().unwrap());
            let position = [
                f32::from_le_bytes(record[11..15].try_into().unwrap()),
                f32::from_le_bytes(record[15..19].try_into().unwrap()),
                f32::from_le_bytes(record[19..23].try_into().unwrap()),
            ];
            let born_ms = u64::from_le_bytes(record[23..31].try_into().unwrap());
            let pickup_delay_ms = u16::from_le_bytes(record[31..33].try_into().unwrap());
            if id == 0
                || id >= next_id
                || !(1..=STACK_LIMIT).contains(&count)
                || position.iter().any(|n| !n.is_finite())
            {
                return Err(invalid("invalid legacy dropped item"));
            }
            let drop = LegacyDrop {
                id,
                item,
                count,
                position,
                born_ms,
                pickup_delay_ms,
                old_item,
            };
            if entries.insert(id, drop).is_some() {
                return Err(invalid("duplicate legacy drop ID"));
            }
        }
        Ok(Self {
            revision,
            next_id,
            entries,
        })
    }

    pub fn old_owner_snapshot(&self, id: u64) -> Vec<u8> {
        let Some(entry) = self.entries.get(&id) else {
            return Vec::new();
        };
        let mut bytes = Vec::with_capacity(14);
        bytes.push(1);
        bytes.push(entry.old_item);
        bytes.extend(entry.count.to_le_bytes());
        bytes.extend(entry.born_ms.to_le_bytes());
        bytes.extend(entry.pickup_delay_ms.to_le_bytes());
        bytes
    }

    pub fn encode_v5_checkpoint(&self) -> Vec<u8> {
        let mut bytes =
            Vec::with_capacity(OLD_DROP_HEADER + self.entries.len() * NEW_DROP_RECORD + 4);
        bytes.extend(b"BGDP");
        bytes.extend(3u16.to_le_bytes());
        bytes.extend(self.revision.to_le_bytes());
        bytes.extend(self.next_id.to_le_bytes());
        bytes.extend((self.entries.len() as u32).to_le_bytes());
        for entry in self.entries.values() {
            bytes.extend(entry.id.to_le_bytes());
            bytes.extend(entry.item.get().to_le_bytes());
            bytes.extend(entry.count.to_le_bytes());
            for coordinate in entry.position {
                bytes.extend(coordinate.to_le_bytes());
            }
            bytes.extend(entry.born_ms.to_le_bytes());
            bytes.extend(entry.pickup_delay_ms.to_le_bytes());
            bytes.extend(0u16.to_le_bytes());
            bytes.extend(0u16.to_le_bytes());
        }
        bytes.extend(checksum(&bytes).to_le_bytes());
        bytes
    }
}

pub(super) fn convert_owner(id: u64, bytes: &[u8], ids: &LegacyIdMap) -> io::Result<Vec<u8>> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    if id == 0 || bytes.len() != 14 || bytes[0] != 1 {
        return Err(invalid("invalid legacy WAL drop owner"));
    }
    let item = ids
        .item(bytes[1])
        .ok_or_else(|| invalid("unknown legacy WAL item"))?;
    let count = u16::from_le_bytes(bytes[2..4].try_into().unwrap());
    if !(1..=STACK_LIMIT).contains(&count) {
        return Err(invalid("invalid legacy WAL drop stack"));
    }
    let mut converted = Vec::with_capacity(21);
    converted.push(3);
    converted.extend(item.get().to_le_bytes());
    converted.extend(&bytes[2..14]);
    converted.extend(0u16.to_le_bytes());
    converted.extend(0u16.to_le_bytes());
    Ok(converted)
}

pub(super) fn validate_position(bytes: &[u8]) -> io::Result<[f32; 3]> {
    if bytes.len() != 12 {
        return Err(invalid("invalid legacy WAL drop position"));
    }
    let position = [
        f32::from_le_bytes(bytes[0..4].try_into().unwrap()),
        f32::from_le_bytes(bytes[4..8].try_into().unwrap()),
        f32::from_le_bytes(bytes[8..12].try_into().unwrap()),
    ];
    if position.iter().any(|n| !n.is_finite()) {
        return Err(invalid("non-finite legacy WAL drop position"));
    }
    Ok(position)
}

pub(super) fn validate_action_receipt(bytes: &[u8], ids: &LegacyIdMap) -> io::Result<()> {
    let valid = match bytes {
        [1, 0, ..] if bytes.len() == 16 => {
            ids.state(bytes[14]).is_some() && usize::from(bytes[15]) < SLOTS
        }
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
    };
    if valid {
        Ok(())
    } else {
        Err(invalid("invalid legacy action receipt"))
    }
}

pub(super) fn default_inventory_bytes() -> Vec<u8> {
    let mut bytes = Vec::with_capacity(4 + 2 + 8 + SLOTS * 3 + 4);
    bytes.extend(b"BGIN");
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(0u64.to_le_bytes());
    bytes.resize(bytes.len() + SLOTS * 3, 0);
    bytes.extend(checksum(&bytes).to_le_bytes());
    bytes
}

pub(super) fn checksum(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c9dc5u32, |hash, &byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x01000193)
    })
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
