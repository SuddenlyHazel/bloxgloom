//! Server-owned item stacks. Slot moves are atomic, bounded, and never create items.

use crate::items::{ItemId, valid_item};

mod store;
pub use store::InventoryStore;

#[cfg(test)]
mod tests;

pub const SLOTS: usize = 36;
pub const HOTBAR_SLOTS: usize = 9;
pub const STACK_LIMIT: u16 = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stack {
    pub item: ItemId,
    pub count: u16,
}

impl Stack {
    pub fn valid(self) -> bool {
        valid_item(self.item) && (1..=STACK_LIMIT).contains(&self.count)
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
            slots: [None; SLOTS],
            revision: 0,
        }
    }
}

impl Inventory {
    pub fn insert(&mut self, item: ItemId, count: u16) -> u16 {
        if !valid_item(item) || self.revision == u64::MAX {
            return count;
        }
        let mut remaining = count;
        for slot in &mut self.slots {
            if let Some(stack) = slot
                && stack.item == item
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
                    *slot = Some(Stack { item, count: added });
                    remaining -= added;
                    if remaining == 0 {
                        break;
                    }
                }
            }
        }
        if remaining != count {
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
        let Some(source) = self.slots[from] else {
            return false;
        };
        if amount > source.count {
            return false;
        }
        match self.slots[to] {
            None => {
                self.slots[to] = Some(Stack {
                    item: source.item,
                    count: amount,
                });
                self.slots[from] = (source.count > amount).then_some(Stack {
                    item: source.item,
                    count: source.count - amount,
                });
            }
            Some(target) if target.item == source.item => {
                let moved = amount.min(STACK_LIMIT - target.count);
                if moved == 0 {
                    return false;
                }
                self.slots[to] = Some(Stack {
                    count: target.count + moved,
                    ..target
                });
                self.slots[from] = (source.count > moved).then_some(Stack {
                    count: source.count - moved,
                    ..source
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
