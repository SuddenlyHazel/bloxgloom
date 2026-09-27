//! Public workstation summary. Item components stay private on the server;
//! slot identities/counts and cooking status are visible to nearby players.
use crate::inventory::{STACK_LIMIT, Stack};
use crate::items::ItemId;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum WorkstationKind {
    #[default]
    Kiln,
    Hopper,
    Chest,
}

impl WorkstationKind {
    pub fn slot_count(self) -> usize {
        if self == Self::Chest { 27 } else { 3 }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WorkstationView {
    pub kind: WorkstationKind,
    pub facing: u8,
    pub lit: bool,
    pub progress: u8,
    pub fuel: u16,
    pub slots: Vec<Option<Stack>>,
}

impl Default for WorkstationView {
    fn default() -> Self {
        Self {
            kind: WorkstationKind::Kiln,
            facing: 0,
            lit: false,
            progress: 0,
            fuel: 0,
            slots: vec![None; 3],
        }
    }
}

impl WorkstationView {
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = vec![
            match self.kind {
                WorkstationKind::Kiln => 1,
                WorkstationKind::Hopper => 2,
                WorkstationKind::Chest => 3,
            },
            self.facing,
            u8::from(self.lit),
            self.progress,
        ];
        bytes.extend(self.fuel.to_le_bytes());
        for slot in &self.slots {
            bytes.extend(slot.as_ref().map_or(0, |s| s.item.0).to_le_bytes());
            bytes.extend(slot.as_ref().map_or(0, |s| s.count).to_le_bytes());
        }
        bytes
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let kind = match bytes.first()? {
            1 => WorkstationKind::Kiln,
            2 => WorkstationKind::Hopper,
            3 => WorkstationKind::Chest,
            _ => return None,
        };
        if bytes.len() != 6 + 6 * kind.slot_count() || bytes[1] > 3 || bytes[2] > 1 {
            return None;
        }
        let fuel = u16::from_le_bytes(bytes[4..6].try_into().ok()?);
        if fuel > 240 || (fuel > 0) != (bytes[2] == 1) {
            return None;
        }
        if kind != WorkstationKind::Kiln && bytes[1..6].iter().any(|byte| *byte != 0) {
            return None;
        }
        let mut slots = vec![None; kind.slot_count()];
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
            kind,
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
