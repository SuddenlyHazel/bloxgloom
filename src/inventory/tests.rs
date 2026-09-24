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

#[test]
fn exhausted_revision_rejects_mutations_without_changing_inventory() {
    let mut inventory = Inventory {
        revision: u64::MAX,
        ..Inventory::default()
    };
    inventory.slots[0] = Some(Stack { item: 1, count: 4 });
    inventory.slots[1] = Some(Stack { item: 2, count: 7 });
    let unchanged = inventory.clone();

    assert_eq!(inventory.insert(1, 3), 3);
    assert_eq!(inventory, unchanged);

    assert!(!inventory.consume(0, 1));
    assert_eq!(inventory, unchanged);

    assert!(!inventory.transfer(0, 2, 2));
    assert_eq!(inventory, unchanged);
}
