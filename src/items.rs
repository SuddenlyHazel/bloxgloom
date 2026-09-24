//! Inventory item identifiers. Block items retain their matching world block ID.

use crate::world::BlockId;

pub use crate::content::ItemId;

pub const SEEDS: ItemId = ItemId(128);
pub const SAPLING: ItemId = ItemId(129);
pub const STICK: ItemId = ItemId(130);

#[inline]
pub fn valid_item(item: ItemId) -> bool {
    valid_item_in(item, crate::content::catalog())
}

#[inline]
pub fn valid_item_in(item: ItemId, catalog: &crate::content::Catalog) -> bool {
    catalog.item(item).is_some()
}

#[inline]
pub fn placeable_block(item: ItemId) -> Option<BlockId> {
    placeable_block_in(item, crate::content::catalog())
}

#[inline]
pub fn placeable_block_in(item: ItemId, catalog: &crate::content::Catalog) -> Option<BlockId> {
    catalog
        .item(item)
        .and_then(|definition| definition.placeable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::MAX_BUILTIN_BLOCK;

    #[test]
    fn only_block_items_are_placeable() {
        for block in 1..=MAX_BUILTIN_BLOCK.0 {
            assert!(valid_item(ItemId(block)));
            assert_eq!(
                placeable_block(ItemId(block)),
                Some(crate::content::BlockStateId(block))
            );
        }
        for item in [SEEDS, SAPLING, STICK] {
            assert!(valid_item(item));
            assert_eq!(placeable_block(item), None);
        }
        for item in [
            ItemId(0),
            ItemId(MAX_BUILTIN_BLOCK.0 + 1),
            ItemId(127),
            ItemId(131),
            ItemId(u32::MAX),
        ] {
            assert!(!valid_item(item));
        }
    }
}
