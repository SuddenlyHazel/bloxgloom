//! Public workstation summary. Item components stay private on the server;
//! slot identities/counts and cooking status are visible to nearby players.
use crate::inventory::{STACK_LIMIT, Stack};
use crate::items::ItemId;

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct KilnView {
    pub facing: u8,
    pub lit: bool,
    pub progress: u8,
    pub fuel: u16,
    pub slots: [Option<Stack>; 3],
}

impl KilnView {
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = vec![1, self.facing, u8::from(self.lit), self.progress];
        bytes.extend(self.fuel.to_le_bytes());
        for slot in &self.slots {
            bytes.extend(slot.as_ref().map_or(0, |s| s.item.0).to_le_bytes());
            bytes.extend(slot.as_ref().map_or(0, |s| s.count).to_le_bytes());
        }
        bytes
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 24 || bytes[0] != 1 || bytes[1] > 3 || bytes[2] > 1 {
            return None;
        }
        let fuel = u16::from_le_bytes(bytes[4..6].try_into().ok()?);
        if fuel > 240 || (fuel > 0) != (bytes[2] == 1) {
            return None;
        }
        let mut slots = std::array::from_fn(|_| None);
        for (slot, value) in slots.iter_mut().zip(bytes[6..].chunks_exact(6)) {
            let item = u32::from_le_bytes(value[..4].try_into().ok()?);
            let count = u16::from_le_bytes(value[4..].try_into().ok()?);
            if count > STACK_LIMIT || (item == 0) != (count == 0) {
                return None;
            }
            if count > 0 {
                *slot = Some(Stack::new(ItemId(item), count));
            }
        }
        Some(Self {
            facing: bytes[1],
            lit: bytes[2] == 1,
            progress: bytes[3],
            fuel,
            slots,
        })
    }
}

#[cfg(test)]
mod tests;
