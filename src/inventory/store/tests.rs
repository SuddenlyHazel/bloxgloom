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
    inventory.insert(3, 129);
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
