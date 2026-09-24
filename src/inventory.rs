//! Server-owned item stacks. Slot moves are atomic, bounded, and never create items.

use crate::items::{ItemId, valid_item};
use std::sync::Arc;

mod store;
pub use store::InventoryStore;

#[cfg(test)]
mod tests;

pub const SLOTS: usize = 36;
pub const HOTBAR_SLOTS: usize = 9;
pub const STACK_LIMIT: u16 = 128;
pub const MAX_COMPONENT_BYTES: usize = 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentPayload {
    pub version: u16,
    pub bytes: Box<[u8]>,
}

impl ComponentPayload {
    pub fn new(version: u16, bytes: impl Into<Box<[u8]>>) -> Option<Self> {
        let bytes = bytes.into();
        if version == 0 || bytes.is_empty() || bytes.len() > MAX_COMPONENT_BYTES {
            return None;
        }
        Some(Self { version, bytes })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stack {
    pub item: ItemId,
    pub count: u16,
    /// None has no heap allocation and represents the canonical empty payload.
    pub components: Option<Arc<ComponentPayload>>,
}

impl Stack {
    pub fn new(item: ItemId, count: u16) -> Self {
        Self {
            item,
            count,
            components: None,
        }
    }

    pub fn with_components(
        item: ItemId,
        count: u16,
        version: u16,
        bytes: impl Into<Box<[u8]>>,
    ) -> Option<Self> {
        let components = Arc::new(ComponentPayload::new(version, bytes)?);
        Some(Self {
            item,
            count,
            components: Some(components),
        })
    }

    pub fn valid(&self) -> bool {
        valid_item(self.item) && (1..=STACK_LIMIT).contains(&self.count) && self.valid_components()
    }

    pub fn valid_in(&self, catalog: &crate::content::Catalog) -> bool {
        catalog.item(self.item).is_some()
            && (1..=STACK_LIMIT).contains(&self.count)
            && self.valid_components()
    }

    fn valid_components(&self) -> bool {
        self.components.as_ref().is_none_or(|payload| {
            payload.version != 0
                && !payload.bytes.is_empty()
                && payload.bytes.len() <= MAX_COMPONENT_BYTES
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inventory {
    pub slots: [Option<Stack>; SLOTS],
    pub revision: u64,
}

impl Default for Inventory {
    fn default() -> Self {
        Self {
            slots: std::array::from_fn(|_| None),
            revision: 0,
        }
    }
}

impl Inventory {
    pub fn insert(&mut self, item: ItemId, count: u16) -> u16 {
        self.insert_with_catalog(item, count, crate::content::catalog())
    }

    pub fn insert_with_catalog(
        &mut self,
        item: ItemId,
        count: u16,
        catalog: &crate::content::Catalog,
    ) -> u16 {
        self.insert_stack(&Stack::new(item, count), catalog)
    }

    /// Inserts one stack, preserving its component payload and matching it
    /// exactly when merging with existing stacks.
    pub fn insert_stack(&mut self, incoming: &Stack, catalog: &crate::content::Catalog) -> u16 {
        if catalog.item(incoming.item).is_none()
            || !incoming.valid_components()
            || self.revision == u64::MAX
        {
            return incoming.count;
        }
        let mut remaining = incoming.count;
        for slot in &mut self.slots {
            if let Some(stack) = slot
                && stack.item == incoming.item
                && stack.components == incoming.components
                && stack.count < STACK_LIMIT
            {
                let added = remaining.min(STACK_LIMIT - stack.count);
                stack.count += added;
                remaining -= added;
                if remaining == 0 {
                    break;
                }
            }
        }
        if remaining > 0 {
            for slot in &mut self.slots {
                if slot.is_none() {
                    let added = remaining.min(STACK_LIMIT);
                    let mut stack = incoming.clone();
                    stack.count = added;
                    *slot = Some(stack);
                    remaining -= added;
                    if remaining == 0 {
                        break;
                    }
                }
            }
        }
        if remaining != incoming.count {
            self.revision += 1;
        }
        remaining
    }

    pub fn consume(&mut self, slot: u8, item: ItemId) -> bool {
        if self.revision == u64::MAX {
            return false;
        }
        let Some(entry) = self.slots.get_mut(slot as usize) else {
            return false;
        };
        let Some(stack) = entry.as_mut() else {
            return false;
        };
        if stack.item != item {
            return false;
        }
        stack.count -= 1;
        if stack.count == 0 {
            *entry = None;
        }
        self.revision += 1;
        true
    }

    /// Transfer an exact amount; a full-stack move onto another item swaps slots.
    pub fn transfer(&mut self, from: u8, to: u8, amount: u16) -> bool {
        if self.revision == u64::MAX {
            return false;
        }
        let (from, to) = (from as usize, to as usize);
        if from >= SLOTS || to >= SLOTS || from == to || amount == 0 {
            return false;
        }
        let Some(source) = self.slots[from].clone() else {
            return false;
        };
        if amount > source.count {
            return false;
        }
        match self.slots[to].clone() {
            None => {
                let mut moved = source.clone();
                moved.count = amount;
                self.slots[to] = Some(moved);
                self.slots[from] = (source.count > amount).then(|| {
                    let mut rest = source;
                    rest.count -= amount;
                    rest
                });
            }
            Some(target)
                if target.item == source.item && target.components == source.components =>
            {
                let moved = amount.min(STACK_LIMIT - target.count);
                if moved == 0 {
                    return false;
                }
                let mut merged = target;
                merged.count += moved;
                self.slots[to] = Some(merged);
                self.slots[from] = (source.count > moved).then(|| {
                    let mut rest = source;
                    rest.count -= moved;
                    rest
                });
            }
            Some(target) if amount == source.count => {
                self.slots[to] = Some(source);
                self.slots[from] = Some(target);
            }
            Some(_) => return false,
        }
        self.revision += 1;
        true
    }
}
