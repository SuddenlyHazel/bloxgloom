//! Read-only BGIN v1 decoder used only by explicit offline conversion.

use std::io;

use crate::inventory::{Inventory, SLOTS, Stack};
use crate::storage::legacy::LegacyIdMap;

use super::{InventoryStore, checksum, invalid};

const MAGIC: &[u8; 4] = b"BGIN";
const LEN: usize = 4 + 2 + 8 + SLOTS * 3 + 4;

impl InventoryStore {
    /// BGIN v1: magic[4], version u16, revision u64, 36 ×
    /// `(old item byte, count u16)`, FNV-1a checksum u32. No components exist.
    pub fn decode_legacy_snapshot(bytes: &[u8], ids: &LegacyIdMap) -> io::Result<Inventory> {
        if bytes.len() != LEN
            || &bytes[..4] != MAGIC
            || u16::from_le_bytes(bytes[4..6].try_into().unwrap()) != 1
        {
            return Err(invalid("invalid legacy inventory file"));
        }
        let checksum_at = LEN - 4;
        if u32::from_le_bytes(bytes[checksum_at..].try_into().unwrap())
            != checksum(&bytes[..checksum_at])
        {
            return Err(invalid("legacy inventory checksum mismatch"));
        }
        let mut inventory = Inventory {
            revision: u64::from_le_bytes(bytes[6..14].try_into().unwrap()),
            ..Inventory::default()
        };
        for (slot, entry) in inventory
            .slots
            .iter_mut()
            .zip(bytes[14..checksum_at].chunks_exact(3))
        {
            let (old_item, count) = (
                entry[0],
                u16::from_le_bytes(entry[1..3].try_into().unwrap()),
            );
            if old_item == 0 {
                if count != 0 {
                    return Err(invalid("invalid empty legacy inventory slot"));
                }
                continue;
            }
            let item = ids
                .item(old_item)
                .ok_or_else(|| invalid("unknown legacy item ID"))?;
            let stack = Stack::new(item, count);
            if !(1..=crate::inventory::STACK_LIMIT).contains(&stack.count) {
                return Err(invalid("invalid legacy inventory stack"));
            }
            *slot = Some(stack);
        }
        Ok(inventory)
    }
}

#[cfg(test)]
#[path = "legacy/tests.rs"]
mod tests;
