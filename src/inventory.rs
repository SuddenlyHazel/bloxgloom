//! Server-owned item stacks. Slot moves are atomic, bounded, and never create items.

use crate::items::{ItemId, valid_item};

mod store;
pub use store::InventoryStore;

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
        if !valid_item(item) {
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
            self.revision = self.revision.wrapping_add(1);
        }
        remaining
    }

    pub fn consume(&mut self, slot: u8, item: ItemId) -> bool {
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
        self.revision = self.revision.wrapping_add(1);
        true
    }

    /// Transfer an exact amount; a full-stack move onto another item swaps slots.
    pub fn transfer(&mut self, from: u8, to: u8, amount: u16) -> bool {
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
        self.revision = self.revision.wrapping_add(1);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stacks_cap_at_128_and_remainder_survives_full_inventory() {
        let mut inventory = Inventory::default();
        assert_eq!(inventory.insert(1, 300), 0);
        assert_eq!(
            inventory.slots[0],
            Some(Stack {
                item: 1,
                count: 128
            })
        );
        assert_eq!(
            inventory.slots[1],
            Some(Stack {
                item: 1,
                count: 128
            })
        );
        assert_eq!(inventory.slots[2], Some(Stack { item: 1, count: 44 }));
        for slot in &mut inventory.slots {
            *slot = Some(Stack {
                item: 2,
                count: 128,
            });
        }
        assert_eq!(inventory.insert(1, 1), 1);
    }

    #[test]
    fn transfer_split_merge_swap_never_duplicates() {
        let mut inventory = Inventory::default();
        inventory.insert(1, 128);
        inventory.insert(2, 5);
        assert!(inventory.transfer(0, 2, 64));
        assert_eq!(inventory.slots[0].unwrap().count, 64);
        assert_eq!(inventory.slots[2].unwrap().count, 64);
        assert!(!inventory.transfer(0, 2, 65)); // more than source
        assert!(inventory.transfer(0, 2, 64));
        assert_eq!(inventory.slots[2].unwrap().count, 128);
        assert!(inventory.transfer(1, 2, 5));
        assert_eq!(inventory.slots[1].unwrap().count, 128);
        assert_eq!(inventory.slots[2].unwrap().count, 5);
        assert!(!inventory.transfer(2, 1, 1));
    }

    #[test]
    fn merging_fills_target_and_leaves_overflow_in_source() {
        let mut inventory = Inventory::default();
        inventory.slots[0] = Some(Stack { item: 3, count: 80 });
        inventory.slots[1] = Some(Stack {
            item: 3,
            count: 100,
        });
        assert!(inventory.transfer(0, 1, 80));
        assert_eq!(inventory.slots[0].unwrap().count, 52);
        assert_eq!(inventory.slots[1].unwrap().count, 128);
    }

    #[test]
    fn non_block_items_stack_and_move_without_creating_items() {
        let mut inventory = Inventory::default();
        assert_eq!(inventory.insert(crate::items::SEEDS, 200), 0);
        assert_eq!(inventory.slots[0].unwrap().count, 128);
        assert_eq!(inventory.slots[1].unwrap().count, 72);
        assert!(inventory.transfer(0, 2, 40));
        assert_eq!(inventory.slots[2].unwrap().item, crate::items::SEEDS);
        assert_eq!(inventory.slots[0].unwrap().count, 88);
        assert!(inventory.consume(2, crate::items::SEEDS));
        let total: u16 = inventory
            .slots
            .iter()
            .flatten()
            .map(|stack| stack.count)
            .sum();
        assert_eq!(total, 199);
        assert_eq!(inventory.insert(127, 1), 1);
    }
}
