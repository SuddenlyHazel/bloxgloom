use super::*;

#[test]
fn inventory_survives_restart_and_corruption_is_rejected() {
    let root = std::env::temp_dir().join(format!(
        "bloxgloom-inventory-{}-{}",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let store = InventoryStore::new(&root).unwrap();
    let mut inventory = Inventory::default();
    inventory.insert(crate::items::ItemId(3), 129);
    inventory.insert(crate::items::SAPLING, 2);
    store.save(42, &inventory).unwrap();
    assert_eq!(store.load(42).unwrap(), inventory);
    let path = store.path(42);
    let mut bytes = fs::read(&path).unwrap();
    bytes[20] ^= 1;
    fs::write(&path, bytes).unwrap();
    assert!(store.load(42).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn v2_inventory_preserves_wide_item_id_and_rejects_v1() {
    let root = std::env::temp_dir().join(format!(
        "bloxgloom-inventory-wide-{}-{}",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let local = crate::content::Catalog::builtins();
    let mut manifest = crate::content::ContentManifest::from_catalog(&local);
    let stone = manifest
        .entries
        .iter_mut()
        .find(|entry| entry.kind == b'I' && entry.key == "bloxgloom:stone")
        .unwrap();
    stone.id = 65_536;
    manifest
        .entries
        .sort_unstable_by_key(|entry| (entry.kind, entry.id));
    let catalog = Arc::new(manifest.resolve_catalog(&local).unwrap());
    let store = InventoryStore::with_catalog(&root, Arc::clone(&catalog)).unwrap();
    let mut inventory = Inventory::default();
    inventory.slots[0] =
        Some(Stack::with_components(crate::items::ItemId(65_536), 128, 3, vec![4, 5, 6]).unwrap());
    store.save(43, &inventory).unwrap();
    assert_eq!(store.load(43).unwrap(), inventory);
    let mut bytes = fs::read(store.path(43)).unwrap();
    assert_eq!(&bytes[..6], b"BGIN\x02\x00");
    let mut invalid_component = bytes.clone();
    invalid_component[20..22].copy_from_slice(&0u16.to_le_bytes());
    let checksum_at = invalid_component.len() - 4;
    let sum = checksum(&invalid_component[..checksum_at]);
    invalid_component[checksum_at..].copy_from_slice(&sum.to_le_bytes());
    assert!(InventoryStore::decode_snapshot_with_catalog(&invalid_component, &catalog).is_err());
    bytes[4] = 1;
    assert!(InventoryStore::decode_snapshot_with_catalog(&bytes, &catalog).is_err());
    fs::remove_dir_all(root).unwrap();
}
