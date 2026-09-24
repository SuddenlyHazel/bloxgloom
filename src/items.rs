//! Inventory item identifiers. Block items retain their matching world block ID.

use crate::world::BlockId;

pub type ItemId = u8;

pub const SEEDS: ItemId = 128;
pub const SAPLING: ItemId = 129;
pub const STICK: ItemId = 130;

#[inline]
pub fn valid_item(item: ItemId) -> bool {
    crate::content::item_def(item).is_some()
}

#[inline]
pub fn placeable_block(item: ItemId) -> Option<BlockId> {
    crate::content::item_def(item).and_then(|definition| definition.placeable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::MAX_BUILTIN_BLOCK;

    #[test]
    fn only_block_items_are_placeable() {
        for block in 1..=MAX_BUILTIN_BLOCK {
            assert!(valid_item(block));
            assert_eq!(placeable_block(block), Some(block));
        }
        for item in [SEEDS, SAPLING, STICK] {
            assert!(valid_item(item));
            assert_eq!(placeable_block(item), None);
        }
        for item in [0, MAX_BUILTIN_BLOCK + 1, 127, 131, u8::MAX] {
            assert!(!valid_item(item));
        }
    }
}
