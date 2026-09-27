//! Bounded registered inventory projection. Components stay server-private;
//! layout, access roles, and status meanings come from the frozen catalog.
use crate::inventory::{STACK_LIMIT, Stack};
use crate::items::ItemId;
use bloxgloom_host_api::{
    InventoryScreen,
    inventory::{MAX_SLOTS, MAX_STATUS_FIELDS},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WorkstationView {
    pub slots: Vec<Option<Stack>>,
    pub status: Vec<u32>,
}
impl Default for WorkstationView {
    fn default() -> Self {
        Self {
            slots: vec![None; 3],
            status: vec![],
        }
    }
}
impl WorkstationView {
    pub fn encode(&self) -> Vec<u8> {
        assert!(
            !self.slots.is_empty()
                && self.slots.len() <= MAX_SLOTS
                && self.status.len() <= MAX_STATUS_FIELDS
        );
        let mut bytes = vec![1, self.slots.len() as u8, self.status.len() as u8];
        for slot in &self.slots {
            bytes.extend(slot.as_ref().map_or(0, |s| s.item.0).to_le_bytes());
            bytes.extend(slot.as_ref().map_or(0, |s| s.count).to_le_bytes());
        }
        for value in &self.status {
            bytes.extend(value.to_le_bytes());
        }
        bytes
    }
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 3 || bytes[0] != 1 {
            return None;
        }
        let slots = usize::from(bytes[1]);
        let fields = usize::from(bytes[2]);
        if slots == 0
            || slots > MAX_SLOTS
            || fields > MAX_STATUS_FIELDS
            || bytes.len() != 3 + slots * 6 + fields * 4
        {
            return None;
        }
        let slots_end = 3 + slots * 6;
        let slots = bytes[3..slots_end]
            .chunks_exact(6)
            .map(|b| {
                let item = u32::from_le_bytes(b[..4].try_into().ok()?);
                let count = u16::from_le_bytes(b[4..].try_into().ok()?);
                if count > STACK_LIMIT || (item == 0) != (count == 0) {
                    return None;
                }
                Some((count > 0).then(|| Stack::new(ItemId(item), count)))
            })
            .collect::<Option<Vec<_>>>()?;
        let status = bytes[slots_end..]
            .chunks_exact(4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        Some(Self { slots, status })
    }
    pub fn valid_for(&self, screen: &InventoryScreen, catalog: &crate::content::Catalog) -> bool {
        self.slots.len() == usize::from(screen.slots)
            && self.status.len() == screen.status.len()
            && self
                .slots
                .iter()
                .flatten()
                .all(|s| s.valid_in(catalog) && s.components.is_none())
            && self
                .status
                .iter()
                .zip(&screen.status)
                .all(|(v, field)| *v <= field.maximum)
    }
}
#[cfg(test)]
mod tests;
