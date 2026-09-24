//! Inventory item identifiers. Block items retain their matching world block ID.

use crate::world::{BlockId, MAX_BLOCK};

pub type ItemId = u8;

pub const SEEDS: ItemId = 128;
pub const SAPLING: ItemId = 129;
pub const STICK: ItemId = 130;

#[inline]
pub const fn valid_item(item: ItemId) -> bool {
    (item >= 1 && item <= MAX_BLOCK) || matches!(item, SEEDS | SAPLING | STICK)
}

#[inline]
pub const fn placeable_block(item: ItemId) -> Option<BlockId> {
    if item >= 1 && item <= MAX_BLOCK {
        Some(item)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_block_items_are_placeable() {
        for block in 1..=MAX_BLOCK {
            assert!(valid_item(block));
            assert_eq!(placeable_block(block), Some(block));
        }
        for item in [SEEDS, SAPLING, STICK] {
            assert!(valid_item(item));
            assert_eq!(placeable_block(item), None);
        }
        for item in [0, MAX_BLOCK + 1, 127, 131, u8::MAX] {
            assert!(!valid_item(item));
        }
    }
}
