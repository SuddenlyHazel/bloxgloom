use super::*;
use crate::content::Catalog;
use crate::items::{SEEDS, STICK};

fn old_inventory() -> Vec<u8> {
    let mut bytes = b"BGIN".to_vec();
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(7u64.to_le_bytes());
    for index in 0..SLOTS {
        match index {
            0 => {
                bytes.push(SEEDS.0 as u8);
                bytes.extend(128u16.to_le_bytes());
            }
            35 => {
                bytes.push(STICK.0 as u8);
                bytes.extend(1u16.to_le_bytes());
            }
            _ => bytes.extend([0; 3]),
        }
    }
    bytes.extend(checksum(&bytes).to_le_bytes());
    bytes
}

#[test]
fn v1_inventory_decodes_without_components_or_losing_counts() {
    let ids = LegacyIdMap::from_content_map_v1(None, &Catalog::builtins()).unwrap();
    let bytes = old_inventory();
    let inventory = InventoryStore::decode_legacy_snapshot(&bytes, &ids).unwrap();
    assert_eq!(inventory.revision, 7);
    assert_eq!(inventory.slots[0], Some(Stack::new(SEEDS, 128)));
    assert_eq!(inventory.slots[35], Some(Stack::new(STICK, 1)));
    assert_eq!(inventory.slots[1], None);
    assert!(inventory.slots[0].as_ref().unwrap().components.is_none());
}

#[test]
fn v1_inventory_rejects_corruption_unknown_ids_and_overfull_stacks() {
    let ids = LegacyIdMap::from_content_map_v1(None, &Catalog::builtins()).unwrap();
    let bytes = old_inventory();
    assert!(InventoryStore::decode_legacy_snapshot(&bytes[..bytes.len() - 1], &ids).is_err());
    let mut changed = bytes.clone();
    changed[14] = 255;
    assert!(InventoryStore::decode_legacy_snapshot(&changed, &ids).is_err());
    changed.truncate(14);
    changed.extend([0, 1, 0]);
    changed.resize(LEN - 4, 0);
    changed.extend(checksum(&changed).to_le_bytes());
    assert!(InventoryStore::decode_legacy_snapshot(&changed, &ids).is_err());
    let mut overfull = bytes;
    overfull[15..17].copy_from_slice(&129u16.to_le_bytes());
    overfull.truncate(LEN - 4);
    overfull.extend(checksum(&overfull).to_le_bytes());
    assert!(InventoryStore::decode_legacy_snapshot(&overfull, &ids).is_err());
}
