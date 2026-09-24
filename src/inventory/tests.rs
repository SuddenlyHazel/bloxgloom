use super::*;

#[test]
fn stacks_cap_at_128_and_remainder_survives_full_inventory() {
    let mut inventory = Inventory::default();
    assert_eq!(inventory.insert(ItemId(1), 300), 0);
    assert_eq!(inventory.slots[0], Some(Stack::new(ItemId(1), 128)));
    assert_eq!(inventory.slots[1], Some(Stack::new(ItemId(1), 128)));
    assert_eq!(inventory.slots[2], Some(Stack::new(ItemId(1), 44)));
    for slot in &mut inventory.slots {
        *slot = Some(Stack::new(ItemId(2), 128));
    }
    assert_eq!(inventory.insert(ItemId(1), 1), 1);
}

#[test]
fn transfer_split_merge_swap_never_duplicates() {
    let mut inventory = Inventory::default();
    inventory.insert(ItemId(1), 128);
    inventory.insert(ItemId(2), 5);
    assert!(inventory.transfer(0, 2, 64));
    assert_eq!(inventory.slots[0].as_ref().unwrap().count, 64);
    assert_eq!(inventory.slots[2].as_ref().unwrap().count, 64);
    assert!(!inventory.transfer(0, 2, 65));
    assert!(inventory.transfer(0, 2, 64));
    assert_eq!(inventory.slots[2].as_ref().unwrap().count, 128);
    assert!(inventory.transfer(1, 2, 5));
    assert_eq!(inventory.slots[1].as_ref().unwrap().count, 128);
    assert_eq!(inventory.slots[2].as_ref().unwrap().count, 5);
    assert!(!inventory.transfer(2, 1, 1));
}

#[test]
fn merging_fills_target_and_leaves_overflow_in_source() {
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(Stack::new(ItemId(3), 80));
    inventory.slots[1] = Some(Stack::new(ItemId(3), 100));
    assert!(inventory.transfer(0, 1, 80));
    assert_eq!(inventory.slots[0].as_ref().unwrap().count, 52);
    assert_eq!(inventory.slots[1].as_ref().unwrap().count, 128);
}

#[test]
fn non_block_items_move_without_creating_items() {
    let mut inventory = Inventory::default();
    assert_eq!(inventory.insert(crate::items::SEEDS, 200), 0);
    assert!(inventory.transfer(0, 2, 40));
    assert!(inventory.consume(2, crate::items::SEEDS));
    let total: u16 = inventory
        .slots
        .iter()
        .flatten()
        .map(|stack| stack.count)
        .sum();
    assert_eq!(total, 199);
    assert_eq!(inventory.insert(ItemId(127), 1), 1);
}

#[test]
fn exhausted_revision_rejects_mutations_without_changing_inventory() {
    let mut inventory = Inventory {
        revision: u64::MAX,
        ..Inventory::default()
    };
    inventory.slots[0] = Some(Stack::new(ItemId(1), 4));
    inventory.slots[1] = Some(Stack::new(ItemId(2), 7));
    let unchanged = inventory.clone();
    assert_eq!(inventory.insert(ItemId(1), 3), 3);
    assert!(!inventory.consume(0, ItemId(1)));
    assert!(!inventory.transfer(0, 2, 2));
    assert_eq!(inventory, unchanged);
}

#[test]
fn component_payload_is_bounded_and_prevents_cross_variant_merge() {
    assert!(Stack::with_components(ItemId(1), 1, 0, vec![1]).is_none());
    assert!(Stack::with_components(ItemId(1), 1, 1, Vec::<u8>::new()).is_none());
    assert!(Stack::with_components(ItemId(1), 1, 1, vec![1; MAX_COMPONENT_BYTES + 1]).is_none());
    let red = Stack::with_components(ItemId(1), 40, 1, vec![1, 2]).unwrap();
    let blue = Stack::with_components(ItemId(1), 20, 1, vec![1, 3]).unwrap();
    let mut inventory = Inventory::default();
    let catalog = crate::content::Catalog::builtins();
    assert_eq!(inventory.insert_stack(&red, &catalog), 0);
    assert_eq!(inventory.insert_stack(&blue, &catalog), 0);
    assert_eq!(inventory.insert_stack(&red, &catalog), 0);
    assert_eq!(inventory.slots[0].as_ref().unwrap().count, 80);
    assert_eq!(inventory.slots[1].as_ref().unwrap().count, 20);
    assert!(!inventory.transfer(0, 1, 10));
    assert!(inventory.transfer(1, 0, 20));
    assert_eq!(
        inventory.slots[0].as_ref().unwrap().components,
        blue.components
    );
    assert_eq!(
        inventory.slots[1].as_ref().unwrap().components,
        red.components
    );
    assert_eq!(
        inventory
            .slots
            .iter()
            .flatten()
            .map(|stack| stack.count)
            .sum::<u16>(),
        100
    );
}
